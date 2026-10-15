use pumpkin_nbt::compound::NbtCompound;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions, create_dir_all};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Suffix vanilla appends to the live player file to keep the previous save.
const BACKUP_SUFFIX: &str = "_old";

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Manages the storage and retrieval of player data from disk.
///
/// Saves write a temp file in the player directory, sync it, and rename it
/// onto the live file. The previous file is kept beside it with a `_old`
/// suffix. Loads of one player are serialized with saves of that same player.
pub struct PlayerDataStorage {
    /// Path to the directory where player data is stored.
    data_path: PathBuf,
    /// Whether player data saving is enabled.
    save_enabled: bool,
    /// Per-player gates. The map lock is not held across disk IO.
    io_gates: Mutex<HashMap<Uuid, Arc<Mutex<()>>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PlayerDataError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("NBT error: {0}")]
    Nbt(String),
}

impl PlayerDataStorage {
    /// Creates a new `PlayerDataStorage` with the specified data path.
    pub fn new(data_path: impl Into<PathBuf>, enabled: bool) -> Self {
        let path = data_path.into();
        if !path.exists()
            && let Err(e) = create_dir_all(&path)
        {
            error!(
                "Failed to create player data directory at {}: {e}",
                path.display()
            );
        }

        Self {
            data_path: path,
            save_enabled: enabled,
            io_gates: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub const fn get_data_path(&self) -> &PathBuf {
        &self.data_path
    }

    #[must_use]
    pub const fn is_save_enabled(&self) -> bool {
        self.save_enabled
    }

    pub const fn set_save_enabled(&mut self, enabled: bool) {
        self.save_enabled = enabled;
    }

    /// Returns the path for a player's data file based on their UUID.
    #[must_use]
    pub fn get_player_data_path(&self, uuid: &Uuid) -> PathBuf {
        self.get_data_path().join(format!("{uuid}.dat"))
    }

    /// Loads player data from disk.
    ///
    /// Returns `(false, empty)` when saving is disabled or the player has no
    /// file. A corrupt live file, or a missing live file with a backup
    /// present, is read from `<uuid>.dat_old`.
    pub fn load_player_data(&self, uuid: &Uuid) -> Result<(bool, NbtCompound), PlayerDataError> {
        if !self.is_save_enabled() {
            return Ok((false, NbtCompound::new()));
        }

        self.with_player_lock(uuid, || self.load_player_data_locked(uuid))
    }

    /// Saves player data to disk.
    ///
    /// Writes a temp file in the same directory as the live file, syncs it,
    /// then renames it onto `<uuid>.dat`. The previous live file is kept as
    /// `<uuid>.dat_old`.
    pub fn save_player_data(&self, uuid: &Uuid, data: NbtCompound) -> Result<(), PlayerDataError> {
        if !self.is_save_enabled() {
            return Ok(());
        }

        self.with_player_lock(uuid, || self.save_player_data_locked(uuid, data))
    }

    fn with_player_lock<T>(&self, uuid: &Uuid, body: impl FnOnce() -> T) -> T {
        let gate = self.io_gate(uuid);
        let _guard = gate.lock().unwrap_or_else(PoisonError::into_inner);
        body()
    }

    fn io_gate(&self, uuid: &Uuid) -> Arc<Mutex<()>> {
        let mut gates = self.io_gates.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(gates.entry(*uuid).or_default())
    }

    fn load_player_data_locked(&self, uuid: &Uuid) -> Result<(bool, NbtCompound), PlayerDataError> {
        let path = self.get_player_data_path(uuid);
        let backup = path_with_suffix(&path, BACKUP_SUFFIX);

        if path.is_file() {
            match read_player_compound(&path) {
                Ok(nbt) => {
                    debug!("Loaded player data for {uuid} from disk");
                    return Ok((true, nbt));
                }
                Err(err) => {
                    if backup.is_file() {
                        warn!("Player data for {uuid} is unreadable ({err}), trying the backup");
                    } else {
                        error!("Failed to read player data for {uuid}: {err}");
                        return Err(err);
                    }
                }
            }
        } else if backup.is_file() {
            warn!("Player data for {uuid} is missing, loading the backup");
        } else {
            debug!("No player data file found for {uuid}");
            return Ok((false, NbtCompound::new()));
        }

        match read_player_compound(&backup) {
            Ok(nbt) => {
                debug!("Loaded backup player data for {uuid}");
                Ok((true, nbt))
            }
            Err(err) => {
                error!("Failed to read backup player data for {uuid}: {err}");
                Err(err)
            }
        }
    }

    fn save_player_data_locked(
        &self,
        uuid: &Uuid,
        data: NbtCompound,
    ) -> Result<(), PlayerDataError> {
        let path = self.get_player_data_path(uuid);
        if let Some(parent) = path.parent()
            && let Err(err) = create_dir_all(parent)
        {
            error!("Failed to create player data directory for {uuid}: {err}");
            return Err(PlayerDataError::Io(err));
        }

        let temp_path = path.parent().map_or_else(
            || temp_player_path(self.get_data_path(), uuid),
            |directory| temp_player_path(directory, uuid),
        );
        if let Err(err) = write_synced_compound(&temp_path, data) {
            discard_temp(&temp_path);
            error!("Failed to write player data for {uuid}: {err}");
            return Err(err);
        }

        let backup = path_with_suffix(&path, BACKUP_SUFFIX);
        // The live name is missing between these two renames. The per-player
        // lock is held across both, so a rejoin cannot observe that gap.
        if let Err(err) = safe_replace_file(&path, &temp_path, &backup) {
            discard_temp(&temp_path);
            error!("Failed to replace player data for {uuid}: {err}");
            return Err(PlayerDataError::Io(err));
        }

        debug!("Saved player data for {uuid} to disk");
        Ok(())
    }
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn temp_player_path(directory: &Path, uuid: &Uuid) -> PathBuf {
    let seq = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    directory.join(format!("{uuid}-{seq}.dat"))
}

fn read_player_compound(path: &Path) -> Result<NbtCompound, PlayerDataError> {
    let file = File::open(path)?;
    pumpkin_nbt::nbt_compress::read_gzip_compound_tag(file)
        .map_err(|err| PlayerDataError::Nbt(err.to_string()))
}

fn write_synced_compound(path: &Path, data: NbtCompound) -> Result<(), PlayerDataError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    pumpkin_nbt::nbt_compress::write_gzip_compound_tag(data, &mut file)
        .map_err(|err| PlayerDataError::Nbt(err.to_string()))?;
    // Sync the temp file before it is renamed onto the live name.
    file.sync_all()?;
    Ok(())
}

fn discard_temp(path: &Path) {
    if let Err(err) = fs::remove_file(path)
        && err.kind() != io::ErrorKind::NotFound
    {
        error!(
            "Failed to remove temp player file {}: {err}",
            path.display()
        );
    }
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Moves `temp` onto `target`, keeping the previous `target` at `backup`.
///
/// The previous file is renamed to `backup` first. The temp file is then
/// renamed onto the live name, which publishes it in one directory update.
/// If that rename fails, the previous file is moved back.
fn safe_replace_file(target: &Path, temp: &Path, backup: &Path) -> io::Result<()> {
    let moved_aside = if target.try_exists()? {
        remove_file_if_exists(backup)?;
        fs::rename(target, backup)?;
        true
    } else {
        false
    };

    if let Err(err) = fs::rename(temp, target) {
        let target_missing = target.try_exists().is_ok_and(|exists| !exists);
        if moved_aside
            && target_missing
            && let Err(restore_err) = fs::rename(backup, target)
        {
            error!(
                "Failed to restore player data {} from backup: {restore_err}",
                target.display()
            );
        }
        return Err(err);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BACKUP_SUFFIX, PlayerDataStorage, path_with_suffix, read_player_compound};
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_nbt::tag::NbtTag;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Barrier, Mutex, PoisonError};
    use std::thread;
    use tempfile::tempdir;
    use uuid::Uuid;

    const PAYLOAD_INTS: usize = 16_384;
    const READERS: usize = 2;
    const WRITERS: usize = 2;
    const READS_PER_THREAD: usize = 40;
    const WRITES_PER_THREAD: i32 = 12;

    fn sample_compound(generation: i32) -> NbtCompound {
        let mut state = (generation as u32).wrapping_add(1);
        let mut values = Vec::with_capacity(PAYLOAD_INTS);
        let mut checksum = 0i32;
        for _ in 0..PAYLOAD_INTS {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let value = state as i32;
            checksum = checksum.wrapping_add(value);
            values.push(value);
        }

        let mut nbt = NbtCompound::new();
        nbt.put_int("gen", generation);
        nbt.put_int("checksum", checksum);
        nbt.put("payload", NbtTag::IntArray(values));
        nbt
    }

    fn compound_is_complete(nbt: &NbtCompound) -> bool {
        let Some(checksum) = nbt.get_int("checksum") else {
            return false;
        };
        let Some(payload) = nbt.get_int_array("payload") else {
            return false;
        };
        if nbt.get_int("gen").is_none() || payload.len() != PAYLOAD_INTS {
            return false;
        }
        let sum = payload
            .iter()
            .fold(0i32, |acc, value| acc.wrapping_add(*value));
        sum == checksum
    }

    fn temp_names(directory: &std::path::Path, uuid: &Uuid) -> Vec<String> {
        let prefix = format!("{uuid}-");
        let mut names = Vec::new();
        let entries = fs::read_dir(directory).unwrap();
        for entry in entries {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&prefix) && name.ends_with(".dat") {
                names.push(name.into_owned());
            }
        }
        names
    }

    #[test]
    fn truncated_main_file_falls_back_to_backup() {
        let temp_dir = tempdir().unwrap();
        let storage = PlayerDataStorage::new(temp_dir.path(), true);
        let uuid = Uuid::new_v4();

        storage.save_player_data(&uuid, sample_compound(1)).unwrap();
        storage.save_player_data(&uuid, sample_compound(2)).unwrap();

        let main = storage.get_player_data_path(&uuid);
        let backup = path_with_suffix(&main, BACKUP_SUFFIX);
        let backup_nbt = read_player_compound(&backup).unwrap();
        assert_eq!(backup_nbt.get_int("gen"), Some(1));
        assert!(compound_is_complete(&backup_nbt));

        let (loaded, live) = storage.load_player_data(&uuid).unwrap();
        assert!(loaded);
        assert_eq!(live.get_int("gen"), Some(2));

        fs::write(&main, b"truncated").unwrap();
        let (loaded, restored) = storage.load_player_data(&uuid).unwrap();
        assert!(loaded);
        assert_eq!(restored.get_int("gen"), Some(1));
        assert!(compound_is_complete(&restored));
        assert_eq!(fs::read(&main).unwrap(), b"truncated");
        assert!(temp_names(temp_dir.path(), &uuid).is_empty());
    }

    #[test]
    fn missing_main_file_falls_back_to_backup() {
        let temp_dir = tempdir().unwrap();
        let storage = PlayerDataStorage::new(temp_dir.path(), true);
        let uuid = Uuid::new_v4();

        storage.save_player_data(&uuid, sample_compound(1)).unwrap();
        storage.save_player_data(&uuid, sample_compound(2)).unwrap();

        let main = storage.get_player_data_path(&uuid);
        fs::remove_file(&main).unwrap();

        let (loaded, restored) = storage.load_player_data(&uuid).unwrap();
        assert!(loaded);
        assert_eq!(restored.get_int("gen"), Some(1));
        assert!(compound_is_complete(&restored));
    }

    #[test]
    fn concurrent_save_and_load_never_reads_a_partial_file() {
        let temp_dir = tempdir().unwrap();
        let storage = PlayerDataStorage::new(temp_dir.path(), true);
        let uuid = Uuid::new_v4();
        storage.save_player_data(&uuid, sample_compound(0)).unwrap();

        let barrier = Barrier::new(READERS + WRITERS);
        let bad = AtomicUsize::new(0);
        let reads = AtomicUsize::new(0);
        let failures = Mutex::new(Vec::new());
        let storage = &storage;
        let barrier = &barrier;
        let bad = &bad;
        let reads = &reads;
        let failures = &failures;

        thread::scope(|scope| {
            for _ in 0..READERS {
                scope.spawn(move || {
                    barrier.wait();
                    for _ in 0..READS_PER_THREAD {
                        let loaded = storage.load_player_data(&uuid);
                        let good = match &loaded {
                            Ok((true, nbt)) => compound_is_complete(nbt),
                            _ => false,
                        };
                        if good {
                            reads.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        bad.fetch_add(1, Ordering::Relaxed);
                        let mut recorded = failures.lock().unwrap_or_else(PoisonError::into_inner);
                        if recorded.len() < 4 {
                            recorded.push(describe_load(&loaded));
                        }
                    }
                });
            }

            for writer in 0..WRITERS {
                scope.spawn(move || {
                    barrier.wait();
                    let start = 1 + i32::try_from(writer).unwrap() * WRITES_PER_THREAD;
                    for generation in start..start + WRITES_PER_THREAD {
                        storage
                            .save_player_data(&uuid, sample_compound(generation))
                            .unwrap();
                    }
                });
            }
        });

        let recorded = failures.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            bad.load(Ordering::Relaxed),
            0,
            "partial or missing reads: {recorded:?}"
        );
        assert_eq!(reads.load(Ordering::Relaxed), READERS * READS_PER_THREAD);

        let (loaded, nbt) = storage.load_player_data(&uuid).unwrap();
        assert!(loaded);
        assert!(compound_is_complete(&nbt));
        assert!(temp_names(temp_dir.path(), &uuid).is_empty());
    }

    fn describe_load(loaded: &Result<(bool, NbtCompound), super::PlayerDataError>) -> String {
        match loaded {
            Ok((false, _)) => "missing file".to_string(),
            Ok((true, nbt)) => {
                let payload_len = nbt.get_int_array("payload").map_or(0, <[i32]>::len);
                format!(
                    "gen={:?} checksum={:?} payload_len={payload_len}",
                    nbt.get_int("gen"),
                    nbt.get_int("checksum"),
                )
            }
            Err(err) => err.to_string(),
        }
    }
}
