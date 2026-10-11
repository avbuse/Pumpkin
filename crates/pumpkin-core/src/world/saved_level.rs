//! Restores the clock, weather, and border, and snapshots them for the writer.
//!
//! Gamerules stay on `LevelData`. The scoreboard is one server-wide board, loaded
//! once and shared by every dimension. File writes live in the world-info writer.

use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pumpkin_data::dimension::Dimension;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::nbt_compress::read_gzip_compound_tag;
use pumpkin_world::world_info::LevelData;
use pumpkin_world::world_info::data_files::{
    DimensionClock, SavedBorder, SavedLevelSnapshot, WeatherData, minecraft_data_dir,
};

use super::World;
use super::border::Worldborder;
use super::scoreboard::Scoreboard;
use super::time::LevelTime;
use super::weather::Weather;

/// Clock, weather, and border read from a world folder.
pub(super) struct LoadedLevelState {
    pub(super) time: LevelTime,
    pub(super) weather: Weather,
    pub(super) border: Worldborder,
}

enum ReadGzip {
    Missing,
    Compound(NbtCompound),
    Failed,
}

/// Fills the live level objects from the world folder.
///
/// A missing section keeps the defaults `World::load` used before this restore.
#[must_use]
pub(super) fn load_level_state(
    root: &Path,
    dimension_name: &str,
    level_data: &LevelData,
) -> LoadedLevelState {
    let level_dat = match read_gzip(&root.join("level.dat")) {
        ReadGzip::Compound(root_tag) => root_tag.get_compound("Data").cloned(),
        ReadGzip::Missing | ReadGzip::Failed => None,
    };
    LoadedLevelState {
        time: load_time(root, dimension_name, level_data, level_dat.as_ref()),
        weather: load_weather(root, dimension_name, level_dat.as_ref()),
        border: load_border(root, dimension_name, level_dat.as_ref()),
    }
}

/// The one scoreboard for the server. Every dimension shares the result.
#[must_use]
pub fn load_scoreboard(root: &Path) -> Scoreboard {
    let paths = [
        minecraft_data_dir(root).join("scoreboard.dat"),
        root.join("data").join("scoreboard.dat"),
    ];
    for path in &paths {
        if let ReadGzip::Compound(file) = read_gzip(path) {
            return Scoreboard::from_nbt(&file);
        }
    }
    Scoreboard::default()
}

/// Copies the live clocks, weather, borders, and the shared scoreboard.
///
/// An empty world list leaves the snapshot unset, so a save during startup
/// does not invent weather or clocks.
#[must_use]
pub fn snapshot_saved_level(
    worlds: &[Arc<World>],
    scoreboard: &Scoreboard,
) -> Option<SavedLevelSnapshot> {
    if worlds.is_empty() {
        return None;
    }
    let mut clocks = Vec::new();
    let mut borders = Vec::new();
    let mut weather = None;
    let mut world_age = 0;
    for world in worlds {
        let time = world
            .level_time
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let border = world
            .worldborder
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let name = world.dimension.minecraft_name;
        if let Some(clock_id) = clock_id(name) {
            clocks.push((
                clock_id.to_string(),
                DimensionClock {
                    total_ticks: time.time_of_day,
                    partial_tick: Some(time.partial_tick),
                    rate: Some(time.rate),
                    paused: Some(time.paused),
                },
            ));
        }
        borders.push(saved_border(name, &border));
        if is_overworld(name) {
            world_age = time.world_age;
            let live = world
                .weather
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            weather = Some(WeatherData {
                rain_time: live.rain_time,
                raining: live.raining,
                thundering: live.thundering,
                thunder_time: live.thunder_time,
                clear_weather_time: live.clear_weather_time,
                data_version: 0,
            });
        }
    }
    Some(SavedLevelSnapshot {
        world_age,
        clocks,
        weather: weather.unwrap_or_default(),
        scoreboard: scoreboard.saved_data(),
        borders,
    })
}

fn saved_border(dimension_name: &str, border: &Worldborder) -> SavedBorder {
    SavedBorder {
        dimension: dimension_name.to_string(),
        center_x: border.center_x,
        center_z: border.center_z,
        size: border.saved_size(),
        lerp_target: border.new_diameter,
        lerp_time: border.saved_lerp_time(),
        damage_per_block: f64::from(border.damage_per_block),
        safe_zone: f64::from(border.buffer),
        warning_blocks: border.warning_blocks,
        warning_time: border.warning_time,
    }
}

fn is_overworld(dimension_name: &str) -> bool {
    dimension_name == Dimension::OVERWORLD.minecraft_name
}

fn clock_id(dimension_name: &str) -> Option<&'static str> {
    if is_overworld(dimension_name) {
        Some(Dimension::OVERWORLD.minecraft_name)
    } else if dimension_name == Dimension::THE_END.minecraft_name {
        Some(Dimension::THE_END.minecraft_name)
    } else {
        None
    }
}

fn dimension_border_path(root: &Path, dimension_name: &str) -> PathBuf {
    let (namespace, name) = dimension_name
        .split_once(':')
        .unwrap_or(("minecraft", dimension_name));
    root.join("dimensions")
        .join(namespace)
        .join(name)
        .join("data")
        .join("minecraft")
        .join("world_border.dat")
}

fn load_time(
    root: &Path,
    dimension_name: &str,
    level_data: &LevelData,
    level_dat: Option<&NbtCompound>,
) -> LevelTime {
    let world_age = level_dat
        .and_then(|data| data.get_long("Time"))
        .unwrap_or(0);
    let mut time = LevelTime::new();
    if let Some(clock_id) = clock_id(dimension_name)
        && let Some(clock) = read_clock(root, clock_id)
    {
        time.restore(
            clock.get_long("total_ticks").unwrap_or(0),
            world_age,
            clock.get_float("partial_tick").unwrap_or(0.0),
            clock.get_float("rate").unwrap_or(1.0),
            clock.get_bool("paused").unwrap_or(false),
        );
        return time;
    }
    let time_of_day = if is_overworld(dimension_name) {
        level_dat
            .and_then(|data| data.get_long("DayTime"))
            .unwrap_or(level_data.day_time)
    } else {
        0
    };
    time.restore(time_of_day, world_age, 0.0, 1.0, false);
    time
}

fn read_clock(root: &Path, clock_id: &str) -> Option<NbtCompound> {
    let path = minecraft_data_dir(root).join("world_clocks.dat");
    let ReadGzip::Compound(file) = read_gzip(&path) else {
        return None;
    };
    file.get_compound("data")
        .and_then(|data| data.get_compound(clock_id))
        .cloned()
}

fn load_weather(root: &Path, dimension_name: &str, level_dat: Option<&NbtCompound>) -> Weather {
    if !is_overworld(dimension_name) {
        return Weather::new();
    }
    let path = minecraft_data_dir(root).join("weather.dat");
    if let ReadGzip::Compound(file) = read_gzip(&path) {
        let owned = file.get_compound("data").cloned();
        let data = owned.as_ref().unwrap_or(&file);
        return Weather::from_saved(
            data.get_int("clear_weather_time").unwrap_or(0),
            data.get_int("rain_time").unwrap_or(0),
            data.get_int("thunder_time").unwrap_or(0),
            data.get_bool("raining").unwrap_or(false),
            data.get_bool("thundering").unwrap_or(false),
        );
    }
    level_dat
        .and_then(weather_from_level_dat)
        .unwrap_or_default()
}

fn weather_from_level_dat(data: &NbtCompound) -> Option<Weather> {
    const KEYS: &[&str] = &[
        "raining",
        "thundering",
        "rainTime",
        "thunderTime",
        "clearWeatherTime",
    ];
    if !KEYS.iter().any(|key| data.has(key)) {
        return None;
    }
    Some(Weather::from_saved(
        data.get_int("clearWeatherTime").unwrap_or(0),
        data.get_int("rainTime").unwrap_or(0),
        data.get_int("thunderTime").unwrap_or(0),
        data.get_bool("raining").unwrap_or(false),
        data.get_bool("thundering").unwrap_or(false),
    ))
}

fn load_border(root: &Path, dimension_name: &str, level_dat: Option<&NbtCompound>) -> Worldborder {
    let path = dimension_border_path(root, dimension_name);
    if let ReadGzip::Compound(file) = read_gzip(&path) {
        let owned = file.get_compound("data").cloned();
        let data = owned.as_ref().unwrap_or(&file);
        return border_from_settings(data);
    }
    if is_overworld(dimension_name)
        && let Some(data) = level_dat
        && let Some(border) = border_from_level_dat(data)
    {
        return border;
    }
    Worldborder::vanilla_default()
}

fn border_from_settings(data: &NbtCompound) -> Worldborder {
    let fallback = Worldborder::vanilla_default();
    let size = data.get_double("size").unwrap_or(fallback.new_diameter);
    Worldborder::from_saved(
        data.get_double("center_x").unwrap_or(fallback.center_x),
        data.get_double("center_z").unwrap_or(fallback.center_z),
        size,
        data.get_double("lerp_target").unwrap_or(size),
        data.get_long("lerp_time").unwrap_or(0),
        data.get_int("warning_blocks")
            .unwrap_or(fallback.warning_blocks),
        data.get_int("warning_time")
            .unwrap_or(fallback.warning_time),
        double_as_f32(
            data.get_double("damage_per_block")
                .unwrap_or(f64::from(fallback.damage_per_block)),
        ),
        double_as_f32(
            data.get_double("safe_zone")
                .unwrap_or(f64::from(fallback.buffer)),
        ),
    )
}

fn border_from_level_dat(data: &NbtCompound) -> Option<Worldborder> {
    if !["BorderSize", "BorderCenterX", "BorderCenterZ"]
        .iter()
        .any(|key| data.has(key))
    {
        return None;
    }
    let fallback = Worldborder::vanilla_default();
    let size = data
        .get_double("BorderSize")
        .unwrap_or(fallback.new_diameter);
    Some(Worldborder::from_saved(
        data.get_double("BorderCenterX")
            .unwrap_or(fallback.center_x),
        data.get_double("BorderCenterZ")
            .unwrap_or(fallback.center_z),
        size,
        data.get_double("BorderSizeLerpTarget").unwrap_or(size),
        data.get_long("BorderSizeLerpTime").unwrap_or(0),
        data.get_int("BorderWarningBlocks")
            .unwrap_or(fallback.warning_blocks),
        data.get_int("BorderWarningTime")
            .unwrap_or(fallback.warning_time),
        double_as_f32(
            data.get_double("BorderDamagePerBlock")
                .unwrap_or(f64::from(fallback.damage_per_block)),
        ),
        double_as_f32(
            data.get_double("BorderSafeZone")
                .unwrap_or(f64::from(fallback.buffer)),
        ),
    ))
}

const fn double_as_f32(value: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "border damage and the safe zone are f32 on the live border"
    )]
    let narrowed = value as f32;
    if narrowed.is_finite() { narrowed } else { 0.0 }
}

fn read_gzip(path: &Path) -> ReadGzip {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return ReadGzip::Missing,
        Err(error) => {
            tracing::warn!("Failed to open {}: {error}", path.display());
            return ReadGzip::Failed;
        }
    };
    match read_gzip_compound_tag(file) {
        Ok(compound) => ReadGzip::Compound(compound),
        Err(error) => {
            tracing::warn!("Failed to read {}: {error}", path.display());
            ReadGzip::Failed
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use pumpkin_data::dimension::Dimension;
    use pumpkin_data::scoreboard::ScoreboardDisplaySlot;
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_nbt::nbt_compress::write_gzip_compound_tag;
    use pumpkin_nbt::tag::NbtTag;
    use pumpkin_protocol::java::client::play::RenderType;
    use pumpkin_util::text::TextComponent;
    use pumpkin_util::world_seed::Seed;
    use pumpkin_world::world_info::LevelData;
    use pumpkin_world::world_info::data_files::minecraft_data_dir;

    use super::{load_level_state, load_scoreboard};
    use crate::world::scoreboard::{NoTarget, Scoreboard, ScoreboardObjective};

    fn write_nbt(path: &std::path::Path, compound: NbtCompound) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let file = File::create(path).unwrap();
        write_gzip_compound_tag(compound, file).unwrap();
    }

    fn saved_root(data: NbtCompound) -> NbtCompound {
        let mut root = NbtCompound::new();
        root.put_int("DataVersion", 1);
        root.put_compound("data", data);
        root
    }

    fn level_dat(time: i64, day_time: i64) -> NbtCompound {
        let mut data = NbtCompound::new();
        data.put_long("Time", time);
        data.put_long("DayTime", day_time);
        let mut root = NbtCompound::new();
        root.put_compound("Data", data);
        root
    }

    fn write_shaped_save(root: &std::path::Path) {
        let mut overworld_clock = NbtCompound::new();
        overworld_clock.put_long("total_ticks", 18_000);
        overworld_clock.put_float("partial_tick", 0.25);
        overworld_clock.put_float("rate", 2.0);
        overworld_clock.put_bool("paused", true);
        let mut end_clock = NbtCompound::new();
        end_clock.put_long("total_ticks", 100);
        let mut clocks = NbtCompound::new();
        clocks.put_compound("minecraft:overworld", overworld_clock);
        clocks.put_compound("minecraft:the_end", end_clock);
        write_nbt(
            &minecraft_data_dir(root).join("world_clocks.dat"),
            saved_root(clocks),
        );

        let mut weather = NbtCompound::new();
        weather.put_bool("raining", true);
        weather.put_bool("thundering", true);
        weather.put_int("rain_time", 1_200);
        weather.put_int("thunder_time", 800);
        weather.put_int("clear_weather_time", 15);
        write_nbt(
            &minecraft_data_dir(root).join("weather.dat"),
            saved_root(weather),
        );

        let mut border = NbtCompound::new();
        border.put_double("center_x", 10.5);
        border.put_double("center_z", -20.0);
        border.put_double("size", 0.0);
        border.put_double("lerp_target", 100.0);
        border.put_long("lerp_time", 4);
        border.put_int("warning_blocks", 3);
        border.put_int("warning_time", 10);
        border.put_double("damage_per_block", 0.5);
        border.put_double("safe_zone", 1.0);
        write_nbt(
            &root
                .join("dimensions")
                .join("minecraft")
                .join("overworld")
                .join("data")
                .join("minecraft")
                .join("world_border.dat"),
            saved_root(border),
        );

        let mut objective = NbtCompound::new();
        objective.put_string("Name", "kills".to_string());
        objective.put_string("CriteriaName", "dummy".to_string());
        objective.put_string("DisplayName", "Kills".to_string());
        objective.put_string("RenderType", "integer".to_string());
        let mut score = NbtCompound::new();
        score.put_string("Name", "Steve".to_string());
        score.put_string("Objective", "kills".to_string());
        score.put_int("Score", 7);
        score.put_bool("Locked", false);
        let mut slots = NbtCompound::new();
        slots.put_string("sidebar", "kills".to_string());
        let mut board = NbtCompound::new();
        board.put_list("Objectives", vec![NbtTag::Compound(objective)]);
        board.put_list("PlayerScores", vec![NbtTag::Compound(score)]);
        board.put_compound("DisplaySlots", slots);
        write_nbt(
            &minecraft_data_dir(root).join("scoreboard.dat"),
            saved_root(board),
        );
        write_nbt(&root.join("level.dat"), level_dat(50_000, 1));
    }

    #[test]
    fn loads_vanilla_shaped_save() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_shaped_save(root);

        let info = LevelData::default(Seed(1));
        let overworld = load_level_state(root, Dimension::OVERWORLD.minecraft_name, &info);
        assert_eq!(overworld.time.time_of_day, 18_000);
        assert_eq!(overworld.time.world_age, 50_000);
        assert_eq!(overworld.time.partial_tick, 0.25);
        assert_eq!(overworld.time.rate, 2.0);
        assert!(overworld.time.paused);
        assert!(overworld.weather.raining);
        assert!(overworld.weather.thundering);
        assert_eq!(overworld.weather.rain_time, 1_200);
        assert_eq!(overworld.weather.thunder_time, 800);
        assert_eq!(overworld.weather.clear_weather_time, 15);
        assert_eq!(overworld.weather.rain_level, 1.0);
        assert_eq!(overworld.border.center_x, 10.5);
        assert_eq!(overworld.border.center_z, -20.0);
        assert_eq!(overworld.border.saved_size(), 0.0);
        assert_eq!(overworld.border.new_diameter, 100.0);
        assert_eq!(overworld.border.saved_lerp_time(), 4);
        assert_eq!(overworld.border.warning_blocks, 3);
        assert_eq!(overworld.border.warning_time, 10);
        assert_eq!(overworld.border.damage_per_block, 0.5);
        assert_eq!(overworld.border.buffer, 1.0);

        let scoreboard = load_scoreboard(root);
        let kills = scoreboard.get_objective("kills").unwrap();
        assert_eq!(kills.criterion, "dummy");
        assert_eq!(kills.render_type, RenderType::Integer);
        assert_eq!(kills.display_name.clone().get_text(), "Kills");
        assert_eq!(scoreboard.get_score_value("Steve", "kills"), Some(7));
        assert_eq!(
            scoreboard.get_display_objective(ScoreboardDisplaySlot::Sidebar),
            Some("kills")
        );

        let nether = load_level_state(root, Dimension::THE_NETHER.minecraft_name, &info);
        assert!(!nether.weather.raining);
        assert_eq!(nether.weather.clear_weather_time, 0);
        assert_eq!(nether.border.new_diameter, 5.999_996_8E7);
        assert_eq!(nether.time.time_of_day, 0);
        assert_eq!(nether.time.world_age, 50_000);

        let end = load_level_state(root, Dimension::THE_END.minecraft_name, &info);
        assert_eq!(end.time.time_of_day, 100);
        assert_eq!(end.time.world_age, 50_000);
    }

    #[test]
    fn missing_sections_use_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let info = LevelData::default(Seed(1));
        assert_eq!(info.border_size, 60_000_000.0);
        assert_eq!(info.border_warning_time, 15.0);

        let loaded = load_level_state(dir.path(), Dimension::OVERWORLD.minecraft_name, &info);
        assert_eq!(loaded.time.time_of_day, 0);
        assert_eq!(loaded.time.world_age, 0);
        assert!(!loaded.time.paused);
        assert_eq!(loaded.time.rate, 1.0);
        assert!(!loaded.weather.raining);
        assert_eq!(loaded.weather.clear_weather_time, 0);
        assert_eq!(loaded.border.new_diameter, 5.999_996_8E7);
        assert_eq!(loaded.border.warning_blocks, 5);
        assert_eq!(loaded.border.warning_time, 300);
        assert_eq!(loaded.border.damage_per_block, 0.0);
        assert_eq!(loaded.border.buffer, 0.0);
        assert_eq!(loaded.border.saved_lerp_time(), 0);
        assert!(load_scoreboard(dir.path()).get_objectives().is_empty());
    }

    #[test]
    fn level_dat_legacy_fields() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut data = NbtCompound::new();
        data.put_long("DayTime", 6_000);
        data.put_long("Time", 90_000);
        data.put_double("BorderCenterX", 12.0);
        data.put_double("BorderCenterZ", -4.0);
        data.put_double("BorderSize", 512.0);
        data.put_bool("raining", true);
        data.put_bool("thundering", false);
        data.put_int("rainTime", 300);
        data.put_int("clearWeatherTime", 0);
        let mut level = NbtCompound::new();
        level.put_compound("Data", data);
        write_nbt(&root.join("level.dat"), level);

        let info = LevelData::default(Seed(1));
        let loaded = load_level_state(root, Dimension::OVERWORLD.minecraft_name, &info);
        assert_eq!(loaded.time.time_of_day, 6_000);
        assert_eq!(loaded.time.world_age, 90_000);
        assert!(loaded.weather.raining);
        assert!(!loaded.weather.thundering);
        assert_eq!(loaded.weather.rain_time, 300);
        assert_eq!(loaded.border.center_x, 12.0);
        assert_eq!(loaded.border.new_diameter, 512.0);

        let nether = load_level_state(root, Dimension::THE_NETHER.minecraft_name, &info);
        assert_eq!(nether.time.time_of_day, 0);
        assert!(!nether.weather.raining);
        assert_eq!(nether.border.new_diameter, 5.999_996_8E7);
    }

    #[test]
    fn plain_text_is_stored_as_a_string_tag() {
        let mut scoreboard = Scoreboard::default();
        scoreboard.add_objective(
            &NoTarget,
            ScoreboardObjective::new(
                "kills",
                TextComponent::text("Kills"),
                RenderType::Integer,
                None,
                "dummy",
            ),
        );
        let saved = scoreboard.to_nbt(1);
        let objective = saved
            .get_compound("data")
            .unwrap()
            .get_list("Objectives")
            .unwrap()[0]
            .extract_compound()
            .unwrap();
        assert_eq!(
            objective.get("DisplayName"),
            Some(&NbtTag::String("Kills".into()))
        );
    }

    #[test]
    fn a_string_tag_is_literal_text() {
        let mut objective = NbtCompound::new();
        objective.put_string("Name", "kills".to_string());
        objective.put_string("CriteriaName", "dummy".to_string());
        objective.put_string("DisplayName", r#"{"text":"Kills"}"#.to_string());
        objective.put_string("RenderType", "integer".to_string());
        let mut board = NbtCompound::new();
        board.put_list("Objectives", vec![NbtTag::Compound(objective)]);
        let loaded = Scoreboard::from_nbt(&saved_root(board));
        assert_eq!(
            loaded
                .get_objective("kills")
                .unwrap()
                .display_name
                .clone()
                .get_text(),
            r#"{"text":"Kills"}"#
        );
    }
}
