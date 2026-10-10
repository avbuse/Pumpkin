//! Restores and writes the clock, weather, border, and scoreboard.
//!
//! Gamerules stay on `LevelData`. The world-info writer rewrites the overworld
//! clock's tick count and the clear-weather timer from that struct, so those
//! fields are copied back before it runs.

use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arc_swap::ArcSwap;
use pumpkin_data::dimension::Dimension;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::nbt_compress::{read_gzip_compound_tag, write_gzip_compound_tag};
use pumpkin_world::world_info::LevelData;
use pumpkin_world::world_info::data_files::{self, WeatherData, minecraft_data_dir};
use tracing::warn;

use super::border::Worldborder;
use super::scoreboard::Scoreboard;
use super::time::LevelTime;
use super::weather::Weather;

/// Clock, weather, border, and scoreboard read from a world folder.
pub(super) struct LoadedLevelState {
    pub(super) time: LevelTime,
    pub(super) weather: Weather,
    pub(super) border: Worldborder,
    pub(super) scoreboard: Scoreboard,
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
        scoreboard: load_scoreboard(root),
    }
}

/// Writes time, weather, the border, and the scoreboard into the world folder.
///
/// The overworld copy is also stored on `level_info`. The world-info writer
/// later rewrites the overworld clock and `clear_weather_time` from there.
pub(super) fn persist_level_state(
    root: &Path,
    dimension_name: &str,
    time: &LevelTime,
    weather: &Weather,
    border: &Worldborder,
    scoreboard: &Scoreboard,
    level_info: &Arc<ArcSwap<LevelData>>,
) {
    let version = level_info.load().data_version;
    write_border(root, dimension_name, border, version);
    if let Some(clock_id) = clock_id(dimension_name) {
        write_clock(root, clock_id, time, version);
    }
    if !is_overworld(dimension_name) {
        return;
    }
    write_saved_weather(root, weather, version);
    write_scoreboard(root, scoreboard, version);
    stamp_world_age(root, time.world_age);
    store_overworld_level_info(level_info, time, weather, border);
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
        .and_then(|data| saved_i64(data, &["Time"]))
        .unwrap_or(0);
    let mut time = LevelTime::new();
    if let Some(clock_id) = clock_id(dimension_name)
        && let Some(clock) = read_clock(root, clock_id)
    {
        let (total_ticks, partial_tick, rate, paused) = clock_state(&clock);
        time.restore(total_ticks, world_age, partial_tick, rate, paused);
        return time;
    }
    let time_of_day = if is_overworld(dimension_name) {
        level_dat
            .and_then(|data| saved_i64(data, &["DayTime"]))
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

fn clock_state(clock: &NbtCompound) -> (i64, f32, f32, bool) {
    let total_ticks = saved_i64(clock, &["total_ticks", "totalTicks"]).unwrap_or(0);
    let partial_tick = saved_f32(clock, &["partial_tick", "partialTick"]).unwrap_or(0.0);
    let rate = saved_f32(clock, &["rate"]).unwrap_or(1.0);
    let paused = saved_bool(clock, &["paused"]).unwrap_or(false);
    (total_ticks, partial_tick, rate, paused)
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
            saved_i32(data, &["clear_weather_time"]).unwrap_or(0),
            saved_i32(data, &["rain_time"]).unwrap_or(0),
            saved_i32(data, &["thunder_time"]).unwrap_or(0),
            saved_bool(data, &["raining"]).unwrap_or(false),
            saved_bool(data, &["thundering"]).unwrap_or(false),
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
        "RainTime",
        "rain_time",
        "thunderTime",
        "ThunderTime",
        "thunder_time",
        "clearWeatherTime",
        "clear_weather_time",
    ];
    if !has_any(data, KEYS) {
        return None;
    }
    Some(Weather::from_saved(
        saved_i32(data, &["clearWeatherTime", "clear_weather_time"]).unwrap_or(0),
        saved_i32(data, &["rainTime", "RainTime", "rain_time"]).unwrap_or(0),
        saved_i32(data, &["thunderTime", "ThunderTime", "thunder_time"]).unwrap_or(0),
        saved_bool(data, &["raining"]).unwrap_or(false),
        saved_bool(data, &["thundering"]).unwrap_or(false),
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
    let size = saved_f64(data, &["size"]).unwrap_or(fallback.new_diameter);
    border_from_parts(
        saved_f64(data, &["center_x"]).unwrap_or(fallback.center_x),
        saved_f64(data, &["center_z"]).unwrap_or(fallback.center_z),
        size,
        saved_f64(data, &["lerp_target"]).unwrap_or(size),
        saved_i64(data, &["lerp_time"]).unwrap_or(0),
        saved_i32(data, &["warning_blocks"]).unwrap_or(fallback.warning_blocks),
        saved_i32(data, &["warning_time"]).unwrap_or(fallback.warning_time),
        saved_f32(data, &["damage_per_block"]).unwrap_or(fallback.damage_per_block),
        saved_f32(data, &["safe_zone"]).unwrap_or(fallback.buffer),
    )
}

fn border_from_level_dat(data: &NbtCompound) -> Option<Worldborder> {
    if !has_any(data, &["BorderSize", "BorderCenterX", "BorderCenterZ"]) {
        return None;
    }
    let fallback = Worldborder::vanilla_default();
    let size = saved_f64(data, &["BorderSize"]).unwrap_or(fallback.new_diameter);
    Some(border_from_parts(
        saved_f64(data, &["BorderCenterX"]).unwrap_or(fallback.center_x),
        saved_f64(data, &["BorderCenterZ"]).unwrap_or(fallback.center_z),
        size,
        saved_f64(data, &["BorderSizeLerpTarget"]).unwrap_or(size),
        saved_i64(data, &["BorderSizeLerpTime"]).unwrap_or(0),
        saved_i32(data, &["BorderWarningBlocks"]).unwrap_or(fallback.warning_blocks),
        saved_i32(data, &["BorderWarningTime"]).unwrap_or(fallback.warning_time),
        saved_f32(data, &["BorderDamagePerBlock"]).unwrap_or(fallback.damage_per_block),
        saved_f32(data, &["BorderSafeZone"]).unwrap_or(fallback.buffer),
    ))
}

#[expect(
    clippy::too_many_arguments,
    reason = "one argument per field stored in the border save"
)]
const fn border_from_parts(
    center_x: f64,
    center_z: f64,
    size: f64,
    lerp_target: f64,
    lerp_time: i64,
    warning_blocks: i32,
    warning_time: i32,
    damage_per_block: f32,
    safe_zone: f32,
) -> Worldborder {
    Worldborder::from_saved(
        center_x,
        center_z,
        size,
        lerp_target,
        lerp_time,
        warning_blocks,
        warning_time,
        damage_per_block,
        safe_zone,
    )
}

fn load_scoreboard(root: &Path) -> Scoreboard {
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

fn write_border(root: &Path, dimension_name: &str, border: &Worldborder, data_version: i32) {
    let mut data = NbtCompound::new();
    data.put_double("center_x", border.center_x);
    data.put_double("center_z", border.center_z);
    data.put_double("size", border.saved_size());
    data.put_double("lerp_target", border.new_diameter);
    data.put_long("lerp_time", border.speed);
    data.put_double("damage_per_block", f64::from(border.damage_per_block));
    data.put_double("safe_zone", f64::from(border.buffer));
    data.put_int("warning_blocks", border.warning_blocks);
    data.put_int("warning_time", border.warning_time);
    write_saved_file(
        &dimension_border_path(root, dimension_name),
        data,
        data_version,
    );
}

fn write_clock(root: &Path, clock_id: &str, time: &LevelTime, data_version: i32) {
    let path = minecraft_data_dir(root).join("world_clocks.dat");
    let mut data = match read_gzip(&path) {
        ReadGzip::Compound(file) => file.get_compound("data").cloned().unwrap_or_default(),
        ReadGzip::Missing => NbtCompound::new(),
        ReadGzip::Failed => {
            warn!("Leaving world_clocks.dat unchanged because it could not be read");
            return;
        }
    };
    let mut clock = data.get_compound(clock_id).cloned().unwrap_or_default();
    clock.put_long("total_ticks", time.time_of_day);
    clock.put_float("partial_tick", time.partial_tick);
    clock.put_float("rate", time.rate);
    clock.put_bool("paused", time.paused);
    data.put_compound(clock_id, clock);
    data.put_int("DataVersion", data_version);
    let mut root_tag = NbtCompound::new();
    root_tag.put_compound("data", data);
    if let Err(error) = write_gzip(&path, root_tag) {
        warn!("Failed to write {}: {error}", path.display());
    }
}

fn write_saved_weather(root: &Path, weather: &Weather, data_version: i32) {
    let data = WeatherData {
        rain_time: weather.rain_time,
        raining: weather.raining,
        thundering: weather.thundering,
        thunder_time: weather.thunder_time,
        clear_weather_time: weather.clear_weather_time,
        data_version,
    };
    if let Err(error) = data_files::write_weather(root, &data) {
        warn!("Failed to write weather.dat: {error}");
    }
}

fn write_scoreboard(root: &Path, scoreboard: &Scoreboard, data_version: i32) {
    let path = minecraft_data_dir(root).join("scoreboard.dat");
    if let Err(error) = write_gzip(&path, scoreboard.to_nbt(data_version)) {
        warn!("Failed to write {}: {error}", path.display());
    }
}

fn stamp_world_age(root: &Path, world_age: i64) {
    let path = root.join("level.dat");
    let mut root_tag = match read_gzip(&path) {
        ReadGzip::Compound(root_tag) => root_tag,
        ReadGzip::Missing => NbtCompound::new(),
        ReadGzip::Failed => {
            warn!("Leaving level.dat unchanged because it could not be read");
            return;
        }
    };
    let mut data = root_tag.get_compound("Data").cloned().unwrap_or_default();
    data.put_long("Time", world_age);
    root_tag.put_compound("Data", data);
    if let Err(error) = write_gzip(&path, root_tag) {
        warn!("Failed to write {}: {error}", path.display());
    }
}

fn store_overworld_level_info(
    level_info: &Arc<ArcSwap<LevelData>>,
    time: &LevelTime,
    weather: &Weather,
    border: &Worldborder,
) {
    let updated = {
        let current = level_info.load();
        let mut updated = (**current).clone();
        updated.day_time = time.time_of_day;
        updated.clear_weather_time = weather.clear_weather_time;
        updated.border_center_x = border.center_x;
        updated.border_center_z = border.center_z;
        updated.border_size = border.saved_size();
        updated.border_size_lerp_target = border.new_diameter;
        updated.border_size_lerp_time = border.speed;
        updated.border_damage_per_block = f64::from(border.damage_per_block);
        updated.border_safe_zone = f64::from(border.buffer);
        updated.border_warning_blocks = f64::from(border.warning_blocks);
        updated.border_warning_time = f64::from(border.warning_time);
        updated
    };
    level_info.store(Arc::new(updated));
}

fn write_saved_file(path: &Path, data: NbtCompound, data_version: i32) {
    let mut root = NbtCompound::new();
    root.put_int("DataVersion", data_version);
    root.put_compound("data", data);
    if let Err(error) = write_gzip(path, root) {
        warn!("Failed to write {}: {error}", path.display());
    }
}

fn read_gzip(path: &Path) -> ReadGzip {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return ReadGzip::Missing,
        Err(error) => {
            warn!("Failed to open {}: {error}", path.display());
            return ReadGzip::Failed;
        }
    };
    match read_gzip_compound_tag(file) {
        Ok(compound) => ReadGzip::Compound(compound),
        Err(error) => {
            warn!("Failed to read {}: {error}", path.display());
            ReadGzip::Failed
        }
    }
}

fn write_gzip(path: &Path, compound: NbtCompound) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    let file = File::create(&temp_path)?;
    if let Err(error) = write_gzip_compound_tag(compound, file) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(std::io::Error::other(error));
    }
    std::fs::rename(&temp_path, path)
}

fn has_any(compound: &NbtCompound, keys: &[&str]) -> bool {
    keys.iter().any(|key| compound.has(key))
}

fn saved_bool(compound: &NbtCompound, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(value) = compound.get_bool(key) {
            return Some(value);
        }
    }
    None
}

fn saved_i64(compound: &NbtCompound, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(value) = compound.get_long(key) {
            return Some(value);
        }
        if let Some(value) = compound.get_int(key) {
            return Some(i64::from(value));
        }
    }
    None
}

fn saved_i32(compound: &NbtCompound, keys: &[&str]) -> Option<i32> {
    for key in keys {
        if let Some(value) = compound.get_int(key) {
            return Some(value);
        }
        if let Some(value) = compound.get_long(key)
            && let Ok(value) = i32::try_from(value)
        {
            return Some(value);
        }
        if let Some(value) = compound.get_double(key)
            && let Some(value) = f64_to_i32(value)
        {
            return Some(value);
        }
        if let Some(value) = compound.get_float(key)
            && let Some(value) = f64_to_i32(f64::from(value))
        {
            return Some(value);
        }
    }
    None
}

fn saved_f64(compound: &NbtCompound, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(value) = compound.get_double(key) {
            return Some(value);
        }
        if let Some(value) = compound.get_float(key) {
            return Some(f64::from(value));
        }
        if let Some(value) = compound.get_int(key) {
            return Some(f64::from(value));
        }
        if let Some(value) = compound.get_long(key) {
            return Some(i64_to_f64(value));
        }
    }
    None
}

fn saved_f32(compound: &NbtCompound, keys: &[&str]) -> Option<f32> {
    for key in keys {
        if let Some(value) = compound.get_float(key) {
            return Some(value);
        }
        if let Some(value) = compound.get_double(key)
            && let Some(value) = f64_to_f32(value)
        {
            return Some(value);
        }
        if let Some(value) = compound.get_int(key) {
            return Some(i32_to_f32(value));
        }
    }
    None
}

fn f64_to_i32(value: f64) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    let rounded = value.round();
    if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "rounded value was checked against i32 limits"
    )]
    Some(rounded as i32)
}

fn f64_to_f32(value: f64) -> Option<f32> {
    if !value.is_finite() {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "clock rate and border damage are f32 on the live objects"
    )]
    let narrowed = value as f32;
    narrowed.is_finite().then_some(narrowed)
}

const fn i64_to_f64(value: i64) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "border coordinates and sizes fit in the f64 mantissa"
    )]
    {
        value as f64
    }
}

const fn i32_to_f32(value: i32) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "warning distances and short timers fit in the f32 mantissa"
    )]
    {
        value as f32
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::sync::Arc;

    use arc_swap::ArcSwap;
    use pumpkin_data::dimension::Dimension;
    use pumpkin_data::scoreboard::ScoreboardDisplaySlot;
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_nbt::nbt_compress::write_gzip_compound_tag;
    use pumpkin_nbt::tag::NbtTag;
    use pumpkin_protocol::java::client::play::RenderType;
    use pumpkin_util::text::TextComponent;
    use pumpkin_util::text::color::NamedColor;
    use pumpkin_util::world_seed::Seed;
    use pumpkin_world::world_info::LevelData;
    use pumpkin_world::world_info::data_files::{
        DimensionClock, game_rules_from_nbt, minecraft_data_dir, read_game_rules, read_weather,
        read_world_clocks, write_game_rules, write_scoreboard_stub, write_weather,
        write_world_clocks,
    };

    use super::{load_level_state, persist_level_state};
    use crate::world::border::Worldborder;
    use crate::world::scoreboard::{
        CollisionRule, NameTagVisibility, NoTarget, Scoreboard, ScoreboardObjective, Team,
    };
    use crate::world::time::LevelTime;
    use crate::world::weather::Weather;

    const FRIENDLY_FIRE: i8 = 1;
    const SEE_FRIENDLY_INVISIBLES: i8 = 2;

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

    fn level_dat(time: i64, day_time: i64, border_size: f64) -> NbtCompound {
        let mut data = NbtCompound::new();
        data.put_long("Time", time);
        data.put_long("DayTime", day_time);
        data.put_double("BorderCenterX", 1.0);
        data.put_double("BorderCenterZ", 2.0);
        data.put_double("BorderSize", border_size);
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
        end_clock.put_long("totalTicks", 100);
        let mut clocks = NbtCompound::new();
        clocks.put_compound("minecraft:overworld", overworld_clock);
        clocks.put_compound("minecraft:the_end", end_clock);
        clocks.put_int("DataVersion", 1);
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
        border.put_double("size", 128.0);
        border.put_double("lerp_target", 128.0);
        border.put_long("lerp_time", 0);
        border.put_int("warning_blocks", 3);
        border.put_double("warning_time", 10.0);
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
        objective.put_string("DisplayName", r#"{"text":"Kills"}"#.to_string());
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
        write_nbt(&root.join("level.dat"), level_dat(50_000, 1, 999.0));
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
        assert_eq!(overworld.weather.thunder_level, 1.0);
        assert_eq!(overworld.border.center_x, 10.5);
        assert_eq!(overworld.border.center_z, -20.0);
        assert_eq!(overworld.border.new_diameter, 128.0);
        assert_eq!(overworld.border.warning_blocks, 3);
        assert_eq!(overworld.border.warning_time, 10);
        assert_eq!(overworld.border.damage_per_block, 0.5);
        assert_eq!(overworld.border.buffer, 1.0);
        let kills = overworld.scoreboard.get_objective("kills").unwrap();
        assert_eq!(kills.criterion, "dummy");
        assert_eq!(kills.render_type, RenderType::Integer);
        assert_eq!(kills.display_name.clone().get_text(), "Kills");
        assert_eq!(
            overworld.scoreboard.get_score_value("Steve", "kills"),
            Some(7)
        );
        assert!(
            !overworld
                .scoreboard
                .get_score("Steve", "kills")
                .unwrap()
                .locked
        );
        assert_eq!(
            overworld
                .scoreboard
                .get_display_objective(ScoreboardDisplaySlot::Sidebar),
            Some("kills")
        );

        let nether = load_level_state(root, Dimension::THE_NETHER.minecraft_name, &info);
        assert!(!nether.weather.raining);
        assert_eq!(nether.weather.clear_weather_time, 0);
        assert_eq!(nether.border.new_diameter, 5.999_996_8E7);
        assert_eq!(nether.border.center_x, 0.0);
        assert_eq!(nether.time.time_of_day, 0);
        assert_eq!(nether.time.world_age, 50_000);
        assert!(nether.scoreboard.get_objective("kills").is_some());

        let end = load_level_state(root, Dimension::THE_END.minecraft_name, &info);
        assert_eq!(end.time.time_of_day, 100);
        assert_eq!(end.time.world_age, 50_000);
        assert_eq!(end.border.new_diameter, 5.999_996_8E7);
    }

    #[test]
    fn game_rules_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut rules_data = NbtCompound::new();
        rules_data.put("minecraft:keep_inventory", NbtTag::Byte(1));
        rules_data.put("minecraft:random_tick_speed", NbtTag::Int(20));
        let rules = game_rules_from_nbt(&saved_root(rules_data));
        assert!(rules.keep_inventory);
        assert_eq!(rules.random_tick_speed, 20);

        let version = LevelData::default(Seed(1)).data_version;
        write_game_rules(dir.path(), &rules, version).unwrap();
        let loaded_rules = read_game_rules(dir.path());
        assert!(loaded_rules.keep_inventory);
        assert_eq!(loaded_rules.random_tick_speed, 20);
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
        assert!(!loaded.weather.thundering);
        assert_eq!(loaded.weather.clear_weather_time, 0);
        assert_eq!(loaded.border.center_x, 0.0);
        assert_eq!(loaded.border.center_z, 0.0);
        assert_eq!(loaded.border.new_diameter, 5.999_996_8E7);
        assert_eq!(loaded.border.old_diameter, 5.999_996_8E7);
        assert_eq!(loaded.border.warning_blocks, 5);
        assert_eq!(loaded.border.warning_time, 300);
        assert_eq!(loaded.border.damage_per_block, 0.0);
        assert_eq!(loaded.border.buffer, 0.0);
        assert_eq!(loaded.border.speed, 0);
        assert!(loaded.scoreboard.get_objectives().is_empty());

        let rules = read_game_rules(dir.path());
        assert!(!rules.keep_inventory);
        assert_eq!(rules.random_tick_speed, 3);
    }

    fn sample_scoreboard() -> Scoreboard {
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
        scoreboard.set_score_value(&NoTarget, "Steve", "kills", 7);
        scoreboard.add_team(
            &NoTarget,
            Team {
                name: "red".to_string(),
                display_name: TextComponent::text("Red"),
                options: FRIENDLY_FIRE | SEE_FRIENDLY_INVISIBLES,
                nametag_visibility: NameTagVisibility::Never,
                collision_rule: CollisionRule::Never,
                color: NamedColor::Red,
                player_prefix: TextComponent::text(""),
                player_suffix: TextComponent::text(""),
                players: vec!["Steve".to_string()],
            },
        );
        scoreboard
    }

    fn rewrite_like_level_writer(root: &std::path::Path, info: &LevelData) {
        let mut clocks = read_world_clocks(root);
        clocks.data_version = info.data_version;
        clocks
            .clocks
            .entry(Dimension::OVERWORLD.minecraft_name.to_string())
            .and_modify(|clock| clock.total_ticks = info.day_time)
            .or_insert(DimensionClock {
                total_ticks: info.day_time,
            });
        write_world_clocks(root, &clocks).unwrap();
        let mut saved_weather = read_weather(root);
        saved_weather.clear_weather_time = info.clear_weather_time;
        saved_weather.data_version = info.data_version;
        write_weather(root, &saved_weather).unwrap();
        write_scoreboard_stub(root, info.data_version).unwrap();
    }

    #[test]
    fn round_trip_survives_level_writer() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut level_data = LevelData::default(Seed(1));
        level_data.game_rules.keep_inventory = true;
        let level_info = Arc::new(ArcSwap::from_pointee(level_data));

        let mut time = LevelTime::new();
        time.restore(18_000, 50_000, 0.25, 2.0, true);
        let weather = Weather::from_saved(40, 1_200, 800, true, false);
        let border = Worldborder::from_saved(4.0, 8.0, 256.0, 256.0, 0, 5, 300, 0.5, 1.0);
        persist_level_state(
            root,
            Dimension::OVERWORLD.minecraft_name,
            &time,
            &weather,
            &border,
            &sample_scoreboard(),
            &level_info,
        );
        let mut end_time = LevelTime::new();
        end_time.restore(100, 50_000, 0.0, 1.0, false);
        persist_level_state(
            root,
            Dimension::THE_END.minecraft_name,
            &end_time,
            &Weather::new(),
            &Worldborder::vanilla_default(),
            &Scoreboard::default(),
            &level_info,
        );
        persist_level_state(
            root,
            Dimension::THE_NETHER.minecraft_name,
            &LevelTime::new(),
            &Weather::new(),
            &Worldborder::vanilla_default(),
            &Scoreboard::default(),
            &level_info,
        );

        let info = level_info.load();
        assert!(info.game_rules.keep_inventory);
        assert_eq!(info.day_time, 18_000);
        assert_eq!(info.clear_weather_time, 40);
        assert_eq!(info.border_center_x, 4.0);
        assert_eq!(info.border_center_z, 8.0);
        assert_eq!(info.border_size, 256.0);
        assert_eq!(info.border_size_lerp_target, 256.0);
        assert_eq!(info.border_size_lerp_time, 0);
        rewrite_like_level_writer(root, &info);
        drop(info);

        let loaded = load_level_state(
            root,
            Dimension::OVERWORLD.minecraft_name,
            &level_info.load(),
        );
        assert_eq!(loaded.time.time_of_day, 18_000);
        assert_eq!(loaded.time.world_age, 50_000);
        assert_eq!(loaded.time.partial_tick, 0.25);
        assert_eq!(loaded.time.rate, 2.0);
        assert!(loaded.time.paused);
        assert!(loaded.weather.raining);
        assert!(!loaded.weather.thundering);
        assert_eq!(loaded.weather.rain_time, 1_200);
        assert_eq!(loaded.weather.clear_weather_time, 40);
        assert_eq!(loaded.border.center_x, 4.0);
        assert_eq!(loaded.border.center_z, 8.0);
        assert_eq!(loaded.border.new_diameter, 256.0);
        assert_eq!(loaded.border.damage_per_block, 0.5);
        assert_eq!(loaded.border.buffer, 1.0);
        assert_eq!(loaded.scoreboard.get_score_value("Steve", "kills"), Some(7));
        let team = loaded.scoreboard.get_team("red").unwrap();
        assert_eq!(team.options, FRIENDLY_FIRE | SEE_FRIENDLY_INVISIBLES);
        assert_eq!(team.nametag_visibility, NameTagVisibility::Never);
        assert_eq!(team.players, vec!["Steve".to_string()]);
        assert!(level_info.load().game_rules.keep_inventory);

        let end = load_level_state(root, Dimension::THE_END.minecraft_name, &level_info.load());
        assert_eq!(end.time.time_of_day, 100);
        let nether = load_level_state(
            root,
            Dimension::THE_NETHER.minecraft_name,
            &level_info.load(),
        );
        assert_eq!(nether.border.new_diameter, 5.999_996_8E7);
        assert_eq!(nether.time.time_of_day, 0);
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
        assert_eq!(loaded.weather.clear_weather_time, 0);
        assert_eq!(loaded.border.center_x, 12.0);
        assert_eq!(loaded.border.center_z, -4.0);
        assert_eq!(loaded.border.new_diameter, 512.0);
        assert!(
            !root
                .join("dimensions")
                .join("minecraft")
                .join("overworld")
                .join("data")
                .join("minecraft")
                .join("world_border.dat")
                .exists()
        );

        let nether = load_level_state(root, Dimension::THE_NETHER.minecraft_name, &info);
        assert_eq!(nether.time.time_of_day, 0);
        assert_eq!(nether.time.world_age, 90_000);
        assert!(!nether.weather.raining);
        assert_eq!(nether.border.new_diameter, 5.999_996_8E7);
    }
}
