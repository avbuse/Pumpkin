use pumpkin_nbt::compound::NbtCompound;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions, create_dir_all};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, warn};
use uuid::Uuid;

/// Suffix vanilla appends to the live player file to keep the previous save.
const BACKUP_SUFFIX: &str = "_old";

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// One player's save counter and the lock that covers its disk IO.
///
/// `gate` stores the last sequence successfully written. The map lock is not
/// held across that IO.
struct PlayerSlot {
    gate: Arc<Mutex<u64>>,
    next_sequence: u64,
    inflight: u32,
}

struct SaveState {
    slots: HashMap<Uuid, PlayerSlot>,
    /// Stays until the next load. The slot itself is removed after the leave
    /// save, and a late periodic snapshot must not allocate a fresh sequence.
    departed: HashSet<Uuid>,
}

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
    saves: Mutex<SaveState>,
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
            saves: Mutex::new(SaveState {
                slots: HashMap::new(),
                departed: HashSet::new(),
            }),
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

        self.clear_player_left(uuid);
        let gate = self.io_gate(uuid);
        let _guard = gate.lock().unwrap_or_else(PoisonError::into_inner);
        self.load_player_data_locked(uuid)
    }

    /// Saves player data to disk.
    ///
    /// Assigns the next per-player sequence and writes that snapshot. A later
    /// call with an older sequence does not replace it.
    pub fn save_player_data(&self, uuid: &Uuid, data: NbtCompound) -> Result<(), PlayerDataError> {
        let Some(sequence) = self.begin_snapshot(uuid) else {
            return Ok(());
        };
        self.save_player_data_at(uuid, sequence, data)
    }

    /// Reserves the next save sequence for `uuid`.
    ///
    /// Returns `None` when saving is disabled or the player has left and not
    /// loaded again. The sequence is consumed even if the write is later skipped.
    #[must_use]
    pub fn begin_snapshot(&self, uuid: &Uuid) -> Option<u64> {
        if !self.is_save_enabled() {
            return None;
        }

        let mut state = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        if state.departed.contains(uuid) {
            return None;
        }

        let slot = state.slots.entry(*uuid).or_insert_with(|| PlayerSlot {
            gate: Arc::new(Mutex::new(0)),
            next_sequence: 1,
            inflight: 0,
        });
        let sequence = slot.next_sequence;
        slot.next_sequence = slot.next_sequence.saturating_add(1);
        slot.inflight = slot.inflight.saturating_add(1);
        Some(sequence)
    }

    /// Writes one snapshot, then releases it.
    ///
    /// A sequence older than the last one written for this player is skipped.
    /// If the slot was already removed, the write is skipped and the gate is
    /// not created again.
    pub fn save_player_data_at(
        &self,
        uuid: &Uuid,
        sequence: u64,
        data: NbtCompound,
    ) -> Result<(), PlayerDataError> {
        let _finish = SnapshotFinish {
            storage: self,
            uuid: *uuid,
        };
        if !self.is_save_enabled() {
            return Ok(());
        }

        let Some(gate) = self.existing_gate(uuid) else {
            debug!("Skipping player data save for {uuid}: the save slot is gone");
            return Ok(());
        };
        let mut last_written = gate.lock().unwrap_or_else(PoisonError::into_inner);
        if sequence < *last_written {
            debug!(
                "Skipping stale player data save for {uuid} (sequence {sequence}, last written {last_written})"
            );
            return Ok(());
        }

        let result = self.save_player_data_locked(uuid, data);
        if result.is_ok() {
            *last_written = sequence;
        }
        result
    }

    /// Records that `uuid` has left. A later [`begin_snapshot`] returns `None`
    /// until the next load. The slot is dropped once nothing is in flight.
    pub fn mark_player_left(&self, uuid: &Uuid) {
        let mut state = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        state.departed.insert(*uuid);
        let inflight = state.slots.get(uuid).map_or(0, |slot| slot.inflight);
        if inflight == 0 {
            state.slots.remove(uuid);
        }
    }

    /// Whether `uuid` has left and not loaded since.
    #[must_use]
    pub fn player_has_left(&self, uuid: &Uuid) -> bool {
        let state = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        state.departed.contains(uuid)
    }

    /// Forgets a previous leave so the next snapshot can be saved.
    pub fn clear_player_left(&self, uuid: &Uuid) {
        let mut state = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        state.departed.remove(uuid);
    }

    /// Drops one in-flight snapshot reserved by [`begin_snapshot`].
    ///
    /// [`save_player_data_at`] does this itself. Call it only when that write
    /// is not going to run.
    pub fn finish_snapshot(&self, uuid: &Uuid) {
        let mut state = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(slot) = state.slots.get_mut(uuid) else {
            return;
        };
        slot.inflight = slot.inflight.saturating_sub(1);
        if slot.inflight == 0 && state.departed.contains(uuid) {
            state.slots.remove(uuid);
        }
    }

    fn existing_gate(&self, uuid: &Uuid) -> Option<Arc<Mutex<u64>>> {
        let state = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        state.slots.get(uuid).map(|slot| Arc::clone(&slot.gate))
    }

    fn io_gate(&self, uuid: &Uuid) -> Arc<Mutex<u64>> {
        let mut state = self.saves.lock().unwrap_or_else(PoisonError::into_inner);
        let slot = state.slots.entry(*uuid).or_insert_with(|| PlayerSlot {
            gate: Arc::new(Mutex::new(0)),
            next_sequence: 1,
            inflight: 0,
        });
        Arc::clone(&slot.gate)
    }

    fn load_player_data_locked(&self, uuid: &Uuid) -> Result<(bool, NbtCompound), PlayerDataError> {
        let path = self.get_player_data_path(uuid);
        let backup = path_with_suffix(&path, BACKUP_SUFFIX);
        let mut quarantine_main = false;

        if path.is_file() {
            match read_player_compound(&path) {
                Ok(nbt) => {
                    debug!("Loaded player data for {uuid} from disk");
                    return Ok((true, nbt));
                }
                Err(err) => {
                    if backup.is_file() {
                        warn!("Player data for {uuid} is unreadable ({err}), trying the backup");
                        quarantine_main = true;
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
                if quarantine_main {
                    // The next save renames the live file onto `_old`. A corrupt
                    // live file has to leave that path first, or it replaces the
                    // backup that just loaded.
                    quarantine_corrupt_player_file(&path, uuid);
                }
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

struct SnapshotFinish<'a> {
    storage: &'a PlayerDataStorage,
    uuid: Uuid,
}

impl Drop for SnapshotFinish<'_> {
    fn drop(&mut self) {
        self.storage.finish_snapshot(&self.uuid);
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

/// Moves an unreadable live file off `<uuid>.dat` so the next save cannot
/// rename it onto the backup. A dated name is kept beside it. If the rename
/// fails, the live file is deleted.
fn quarantine_corrupt_player_file(path: &Path, uuid: &Uuid) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    let quarantine = path.with_file_name(format!("{uuid}.dat.{millis}.corrupt"));
    if let Err(err) = fs::rename(path, &quarantine) {
        warn!("Failed to quarantine player data for {uuid} ({err}), deleting it");
        if let Err(delete_err) = fs::remove_file(path)
            && delete_err.kind() != io::ErrorKind::NotFound
        {
            error!(
                "Failed to delete unreadable player data {}: {delete_err}",
                path.display()
            );
        }
        return;
    }
    warn!(
        "Moved unreadable player data for {uuid} to {}",
        quarantine.display()
    );
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

    fn corrupt_names(directory: &std::path::Path, uuid: &Uuid) -> Vec<String> {
        let prefix = format!("{uuid}.dat.");
        let mut names = Vec::new();
        let entries = fs::read_dir(directory).unwrap();
        for entry in entries {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&prefix) && name.ends_with(".corrupt") {
                names.push(name.into_owned());
            }
        }
        names
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
        assert!(!main.exists());
        let quarantined = corrupt_names(temp_dir.path(), &uuid);
        assert_eq!(quarantined.len(), 1);
        assert!(quarantined[0].ends_with(".corrupt"));
        assert!(temp_names(temp_dir.path(), &uuid).is_empty());

        storage.save_player_data(&uuid, sample_compound(3)).unwrap();
        let (loaded, live) = storage.load_player_data(&uuid).unwrap();
        assert!(loaded);
        assert_eq!(live.get_int("gen"), Some(3));
        let backup_after = read_player_compound(&backup).unwrap();
        assert_eq!(backup_after.get_int("gen"), Some(1));
        assert!(compound_is_complete(&backup_after));
        assert_ne!(fs::read(&backup).unwrap(), b"truncated");
    }

    #[test]
    fn older_snapshot_does_not_overwrite_a_newer_save() {
        let temp_dir = tempdir().unwrap();
        let storage = PlayerDataStorage::new(temp_dir.path(), true);
        let uuid = Uuid::new_v4();

        let older = storage.begin_snapshot(&uuid).unwrap();
        let newer = storage.begin_snapshot(&uuid).unwrap();
        storage
            .save_player_data_at(&uuid, newer, sample_compound(2))
            .unwrap();
        storage
            .save_player_data_at(&uuid, older, sample_compound(1))
            .unwrap();

        let (loaded, nbt) = storage.load_player_data(&uuid).unwrap();
        assert!(loaded);
        assert_eq!(nbt.get_int("gen"), Some(2));
        assert!(compound_is_complete(&nbt));
    }

    #[test]
    fn leave_removes_the_player_gate() {
        let temp_dir = tempdir().unwrap();
        let storage = PlayerDataStorage::new(temp_dir.path(), true);
        let uuid = Uuid::new_v4();

        storage.save_player_data(&uuid, sample_compound(1)).unwrap();
        storage.mark_player_left(&uuid);
        assert!(storage.player_has_left(&uuid));
        assert!(storage.begin_snapshot(&uuid).is_none());

        storage
            .save_player_data_at(&uuid, 1, sample_compound(9))
            .unwrap();
        storage.save_player_data(&uuid, sample_compound(9)).unwrap();

        let (loaded, nbt) = storage.load_player_data(&uuid).unwrap();
        assert!(loaded);
        assert_eq!(nbt.get_int("gen"), Some(1));
        assert!(!storage.player_has_left(&uuid));
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
