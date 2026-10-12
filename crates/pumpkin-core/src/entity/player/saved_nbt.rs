//! Player compound tags for health, gamemode, and the respawn point.
//!
//! Spawn tags live in this module so bed-spawn changes can update how a
//! respawn point is chosen without editing the tag layout.

use pumpkin_data::dimension::Dimension;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use super::RespawnPoint;

pub(super) fn write_player_health(nbt: &mut NbtCompound, health: f32) {
    nbt.put_float("Health", health);
}

#[must_use]
pub(super) fn read_player_health(nbt: &NbtCompound) -> Option<f32> {
    nbt.get_numeric_float("Health")
}

pub(super) fn write_player_gamemode(
    nbt: &mut NbtCompound,
    gamemode: GameMode,
    previous: Option<GameMode>,
) {
    nbt.put_int("playerGameType", gamemode as i32);
    if let Some(previous) = previous {
        nbt.put_int("previousPlayerGameType", previous as i32);
    }
}

#[must_use]
pub(super) fn read_player_gamemode(
    nbt: &NbtCompound,
    fallback: GameMode,
) -> (GameMode, Option<GameMode>) {
    let gamemode = read_game_mode_tag(nbt, "playerGameType").unwrap_or(fallback);
    let previous = read_game_mode_tag(nbt, "previousPlayerGameType");
    (gamemode, previous)
}

fn read_game_mode_tag(nbt: &NbtCompound, key: &str) -> Option<GameMode> {
    nbt.get_int(key)
        .or_else(|| nbt.get_byte(key).map(i32::from))
        .or_else(|| nbt.get_short(key).map(i32::from))
        .and_then(|value| GameMode::try_from(value).ok())
}

/// Writes the 26.3 `respawn` compound and the older `Spawn*` keys.
///
/// The compound stores `yaw` and `pitch`. `angle` is the same yaw, kept so a
/// reader that still looks for the previous name does not lose the facing.
pub(super) fn write_spawn_point(nbt: &mut NbtCompound, point: Option<&RespawnPoint>) {
    let Some(point) = point else {
        return;
    };

    let mut respawn = NbtCompound::new();
    respawn.put_string("dimension", point.dimension.minecraft_name.to_owned());
    respawn.put(
        "pos",
        NbtTag::IntArray(vec![
            point.position.0.x,
            point.position.0.y,
            point.position.0.z,
        ]),
    );
    respawn.put_float("yaw", point.yaw);
    respawn.put_float("pitch", point.pitch);
    respawn.put_float("angle", point.yaw);
    respawn.put_bool("forced", point.force);
    nbt.put_compound("respawn", respawn);

    nbt.put_int("SpawnX", point.position.0.x);
    nbt.put_int("SpawnY", point.position.0.y);
    nbt.put_int("SpawnZ", point.position.0.z);
    nbt.put_float("SpawnAngle", point.yaw);
    nbt.put_string("SpawnDimension", point.dimension.minecraft_name.to_owned());
    nbt.put_bool("SpawnForced", point.force);
}

#[must_use]
pub(super) fn read_spawn_point(
    nbt: &NbtCompound,
    fallback_dimension: &Dimension,
) -> Option<RespawnPoint> {
    read_respawn_compound(nbt, fallback_dimension)
        .or_else(|| read_legacy_spawn(nbt, fallback_dimension))
}

fn read_respawn_compound(
    nbt: &NbtCompound,
    fallback_dimension: &Dimension,
) -> Option<RespawnPoint> {
    let respawn = nbt.get_compound("respawn")?;
    let position = read_block_pos(respawn, "pos")?;
    Some(RespawnPoint {
        dimension: dimension_or_fallback(respawn, "dimension", fallback_dimension),
        position,
        yaw: respawn
            .get_numeric_float("yaw")
            .or_else(|| respawn.get_numeric_float("angle"))
            .unwrap_or(0.0),
        pitch: respawn.get_numeric_float("pitch").unwrap_or(0.0),
        force: respawn.get_bool("forced").unwrap_or(false),
    })
}

fn read_legacy_spawn(nbt: &NbtCompound, fallback_dimension: &Dimension) -> Option<RespawnPoint> {
    let x = nbt.get_int("SpawnX")?;
    let y = nbt.get_int("SpawnY")?;
    let z = nbt.get_int("SpawnZ")?;
    Some(RespawnPoint {
        dimension: dimension_or_fallback(nbt, "SpawnDimension", fallback_dimension),
        position: BlockPos(Vector3::new(x, y, z)),
        yaw: nbt.get_numeric_float("SpawnAngle").unwrap_or(0.0),
        pitch: 0.0,
        force: nbt.get_bool("SpawnForced").unwrap_or(false),
    })
}

fn dimension_or_fallback(nbt: &NbtCompound, key: &str, fallback: &Dimension) -> Dimension {
    nbt.get_string(key)
        .and_then(Dimension::from_name)
        .cloned()
        .unwrap_or_else(|| fallback.clone())
}

fn read_block_pos(nbt: &NbtCompound, key: &str) -> Option<BlockPos> {
    if let Some(pos) = nbt.get_int_array(key)
        && pos.len() >= 3
    {
        return Some(BlockPos(Vector3::new(pos[0], pos[1], pos[2])));
    }
    let list = nbt.get_list(key)?;
    if list.len() < 3 {
        return None;
    }
    Some(BlockPos(Vector3::new(
        list[0].extract_int()?,
        list[1].extract_int()?,
        list[2].extract_int()?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(
        dimension: &Dimension,
        x: i32,
        y: i32,
        z: i32,
        yaw: f32,
        pitch: f32,
        force: bool,
    ) -> RespawnPoint {
        RespawnPoint {
            dimension: dimension.clone(),
            position: BlockPos(Vector3::new(x, y, z)),
            yaw,
            pitch,
            force,
        }
    }

    #[test]
    fn spawn_health_and_gamemode_round_trip() {
        let spawn = point(&Dimension::THE_NETHER, 10, 64, -3, 90.0, -15.0, true);
        let mut nbt = NbtCompound::new();
        write_player_health(&mut nbt, 7.5);
        write_player_gamemode(&mut nbt, GameMode::Creative, Some(GameMode::Survival));
        write_spawn_point(&mut nbt, Some(&spawn));

        assert_eq!(nbt.get_float("Health"), Some(7.5));
        assert_eq!(
            nbt.get_int("playerGameType"),
            Some(GameMode::Creative as i32)
        );
        assert_eq!(
            nbt.get_int("previousPlayerGameType"),
            Some(GameMode::Survival as i32)
        );
        let respawn = nbt.get_compound("respawn").expect("respawn compound");
        assert_eq!(respawn.get_float("yaw"), Some(90.0));
        assert_eq!(respawn.get_float("pitch"), Some(-15.0));
        assert_eq!(respawn.get_float("angle"), Some(90.0));
        assert_eq!(nbt.get_float("SpawnAngle"), Some(90.0));

        assert_eq!(read_player_health(&nbt), Some(7.5));
        assert_eq!(
            read_player_gamemode(&nbt, GameMode::Survival),
            (GameMode::Creative, Some(GameMode::Survival))
        );
        assert_eq!(read_spawn_point(&nbt, &Dimension::OVERWORLD), Some(spawn));
    }

    #[test]
    fn vanilla_respawn_compound_keeps_yaw_and_pitch() {
        let mut respawn = NbtCompound::new();
        respawn.put_string("dimension", "minecraft:the_nether".to_string());
        respawn.put("pos", NbtTag::IntArray(vec![1, 2, 3]));
        respawn.put_float("yaw", 45.0);
        respawn.put_float("pitch", 10.0);
        respawn.put_bool("forced", true);
        let mut nbt = NbtCompound::new();
        nbt.put_compound("respawn", respawn);

        let loaded = read_spawn_point(&nbt, &Dimension::OVERWORLD).expect("spawn");
        assert_eq!(loaded.dimension.minecraft_name, "minecraft:the_nether");
        assert_eq!(loaded.position, BlockPos(Vector3::new(1, 2, 3)));
        assert_eq!(loaded.yaw, 45.0);
        assert_eq!(loaded.pitch, 10.0);
        assert!(loaded.force);
    }

    #[test]
    fn older_angle_and_legacy_spawn_keys_still_load() {
        let mut angled = NbtCompound::new();
        angled.put_string("dimension", "minecraft:overworld".to_string());
        angled.put(
            "pos",
            NbtTag::List(vec![NbtTag::Int(4), NbtTag::Int(5), NbtTag::Int(6)]),
        );
        angled.put_float("angle", 12.5);
        let mut with_angle = NbtCompound::new();
        with_angle.put_compound("respawn", angled);

        let from_angle = read_spawn_point(&with_angle, &Dimension::OVERWORLD).expect("angle");
        assert_eq!(from_angle.yaw, 12.5);
        assert_eq!(from_angle.pitch, 0.0);
        assert_eq!(from_angle.position, BlockPos(Vector3::new(4, 5, 6)));

        let mut legacy = NbtCompound::new();
        legacy.put_int("SpawnX", 8);
        legacy.put_int("SpawnY", 70);
        legacy.put_int("SpawnZ", -2);
        legacy.put_float("SpawnAngle", -90.0);
        legacy.put_string("SpawnDimension", "minecraft:the_end".to_string());
        let from_legacy = read_spawn_point(&legacy, &Dimension::OVERWORLD).expect("legacy");
        assert_eq!(from_legacy.dimension.minecraft_name, "minecraft:the_end");
        assert_eq!(from_legacy.position, BlockPos(Vector3::new(8, 70, -2)));
        assert_eq!(from_legacy.yaw, -90.0);
        assert_eq!(from_legacy.pitch, 0.0);
        assert!(!from_legacy.force);
    }

    #[test]
    fn partial_spawn_and_gamemode_keep_a_new_survival_player() {
        let empty = NbtCompound::new();
        assert!(read_spawn_point(&empty, &Dimension::OVERWORLD).is_none());
        assert_eq!(read_player_health(&empty), None);
        assert_eq!(
            read_player_gamemode(&empty, GameMode::Survival),
            (GameMode::Survival, None)
        );

        let mut respawn_only = NbtCompound::new();
        let mut respawn = NbtCompound::new();
        respawn.put_string("dimension", "minecraft:overworld".to_string());
        respawn_only.put_compound("respawn", respawn);
        assert!(read_spawn_point(&respawn_only, &Dimension::OVERWORLD).is_none());

        let mut byte_mode = NbtCompound::new();
        byte_mode.put_byte("playerGameType", 2);
        assert_eq!(
            read_player_gamemode(&byte_mode, GameMode::Survival),
            (GameMode::Adventure, None)
        );

        let mut health = NbtCompound::new();
        health.put_double("Health", 7.5);
        assert_eq!(read_player_health(&health), Some(7.5));
    }
}
