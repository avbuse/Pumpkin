use std::{
    path::PathBuf,
    str::FromStr,
    sync::{
        RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use bytes::Bytes;
use pumpkin_data::{Block, BlockStateId, chunk::ChunkStatus, fluid::Fluid};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::resource_location::{FromResourceLocation, ResourceLocation, ToResourceLocation};
use rustc_hash::FxHashMap;

use crate::{
    chunk::{
        ChunkEntityData, ChunkReadingError, ChunkSerializingError,
        format::anvil::{SingleChunkDataSerializer, WORLD_DATA_VERSION},
        io::{Dirtiable, file_manager::PathFromLevelFolder},
    },
    generation::section_coords,
    level::LevelFolder,
    tick::{ScheduledTick, TickPriority, scheduler::ChunkTickScheduler},
};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector2::Vector2;

use super::{
    ChunkData, ChunkHeightmaps, ChunkLight, ChunkParsingError, ChunkSections,
    palette::{BiomePalette, BlockPalette},
};
pub mod anvil;
pub mod linear;
pub mod pump;

impl SingleChunkDataSerializer for ChunkData {
    #[inline]
    fn from_bytes(bytes: &Bytes, pos: Vector2<i32>) -> Result<Self, ChunkReadingError> {
        Self::internal_from_bytes(bytes, pos).map_err(ChunkReadingError::ParsingError)
    }

    #[inline]
    fn to_bytes(&self) -> Result<Bytes, ChunkSerializingError> {
        Ok(self.internal_to_bytes())
    }

    #[inline]
    fn position(&self) -> (i32, i32) {
        (self.x, self.z)
    }
}

impl PathFromLevelFolder for ChunkData {
    #[inline]
    fn file_path(folder: &LevelFolder, file_name: &str) -> PathBuf {
        folder.region_folder.join(file_name)
    }
}

impl Dirtiable for ChunkData {
    #[inline]
    fn mark_dirty(&self, flag: bool) {
        self.dirty.store(flag, Ordering::Relaxed);
    }

    #[inline]
    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }
}

/// The section stores `Y` as a byte, short, int or long depending on who wrote
/// the file. The datafixer writes ints. Reading only bytes would map every int
/// section to `Y = 0`, and they would overwrite each other.
fn section_y(section: &NbtCompound) -> i32 {
    use pumpkin_nbt::tag::NbtTag;

    match section.get("Y") {
        Some(NbtTag::Byte(value)) => i32::from(*value),
        Some(NbtTag::Short(value)) => i32::from(*value),
        Some(NbtTag::Int(value)) => *value,
        Some(NbtTag::Long(value)) => i32::try_from(*value).unwrap_or(0),
        _ => 0,
    }
}

/// What vanilla does when `yPos` is missing, for example after it upgrades a
/// world in place: the lowest section that stores biomes, and never above 0.
/// That gives `-4` in the Overworld and `0` in the Nether and End, which are
/// the section minimums of those dimensions.
fn lowest_biome_section_y(root_tag: &NbtCompound) -> Option<i32> {
    let sections = root_tag.get_list("sections")?;
    sections
        .iter()
        .filter_map(|tag| match tag {
            pumpkin_nbt::tag::NbtTag::Compound(compound) if compound.has("biomes") => {
                Some(section_y(compound))
            }
            _ => None,
        })
        .min()
        .map(|y| y.min(0))
}

fn minecraft_block_id(block: &Block) -> String {
    if block.name.starts_with("minecraft:") {
        block.name.to_string()
    } else {
        format!("minecraft:{}", block.name)
    }
}

/// A 5023 default state is stored as the block id alone.
fn block_state_id_from_default_name(name: &str) -> BlockStateId {
    Block::from_name(name).map_or(BlockStateId::AIR, |block| block.default_state.id)
}

/// Reads one palette entry.
///
/// 4903 entries are compounds with `Name` and optional `Properties`, including
/// a default state that still carries its properties. 5023 non-default entries
/// are compounds with `id` and `properties`. A 5023 default entry inside a
/// compound list has one field: an empty name whose value is the block id.
/// That is how a string is stored in a compound list, and the NBT reader may
/// already have turned it back into a string before this runs.
fn block_state_id_from_palette_compound(compound: &NbtCompound) -> BlockStateId {
    if compound.child_tags.len() == 1
        && let Some(pumpkin_nbt::tag::NbtTag::String(name)) = compound.get("")
    {
        return block_state_id_from_default_name(name);
    }

    if let Ok(entry) =
        crate::generation::structure::template::PaletteEntry::from_nbt_compound(compound)
        && let Some(state) =
            crate::generation::structure::template::BlockStateResolver::resolve_simple(&entry)
    {
        return state.id;
    }
    BlockStateId::AIR
}

fn block_state_id_from_palette_tag(tag: &pumpkin_nbt::tag::NbtTag) -> BlockStateId {
    match tag {
        pumpkin_nbt::tag::NbtTag::String(name) => block_state_id_from_default_name(name),
        pumpkin_nbt::tag::NbtTag::Compound(compound) => {
            block_state_id_from_palette_compound(compound)
        }
        pumpkin_nbt::tag::NbtTag::Int(value) => BlockStateId::new_or_air(*value as u16),
        pumpkin_nbt::tag::NbtTag::Short(value) => BlockStateId::new_or_air(*value as u16),
        pumpkin_nbt::tag::NbtTag::Byte(value) => BlockStateId::new_or_air(*value as u16),
        pumpkin_nbt::tag::NbtTag::Long(value) => BlockStateId::new_or_air(*value as u16),
        _ => BlockStateId::AIR,
    }
}

/// Writes one 5023 palette entry.
///
/// A default state is the block id. Any other state is `id` plus every
/// property. When the palette mixes the two, the NBT list codec stores each
/// block id under an empty name so the list can stay a compound list.
fn block_state_palette_tag(id: BlockStateId) -> pumpkin_nbt::tag::NbtTag {
    use pumpkin_nbt::tag::NbtTag;

    let block = Block::from_state_id(id);
    let name = minecraft_block_id(block);
    if id == block.default_state.id {
        return NbtTag::String(name.into());
    }

    let mut compound = NbtCompound::new();
    compound.put_string("id", name);
    if let Some(properties) = block.properties(id) {
        let props = properties.to_props();
        if !props.is_empty() {
            let mut props_compound = NbtCompound::new();
            for (key, value) in props {
                props_compound.put_string(key, value.to_string());
            }
            compound.put_compound("properties", props_compound);
        }
    }
    NbtTag::Compound(compound)
}

fn extract_u16_array(tag: &pumpkin_nbt::tag::NbtTag) -> Option<Box<[BlockStateId]>> {
    match tag {
        pumpkin_nbt::tag::NbtTag::IntArray(arr) => Some(
            arr.iter()
                .map(|&x| BlockStateId::new_or_air(x as u16))
                .collect(),
        ),
        pumpkin_nbt::tag::NbtTag::ByteArray(arr) => Some(
            arr.iter()
                .map(|&x| BlockStateId::new_or_air(x as u16))
                .collect(),
        ),
        pumpkin_nbt::tag::NbtTag::LongArray(arr) => Some(
            arr.iter()
                .map(|&x| BlockStateId::new_or_air(x as u16))
                .collect(),
        ),
        pumpkin_nbt::tag::NbtTag::List(list) => {
            Some(list.iter().map(block_state_id_from_palette_tag).collect())
        }
        _ => None,
    }
}

fn extract_u8_array(tag: &pumpkin_nbt::tag::NbtTag) -> Option<Box<[u8]>> {
    match tag {
        pumpkin_nbt::tag::NbtTag::ByteArray(arr) => Some(arr.iter().map(|&x| x as u8).collect()),
        pumpkin_nbt::tag::NbtTag::IntArray(arr) => Some(arr.iter().map(|&x| x as u8).collect()),
        pumpkin_nbt::tag::NbtTag::List(list) => {
            let bytes: Box<[u8]> = list
                .iter()
                .map(|t| match t {
                    pumpkin_nbt::tag::NbtTag::Byte(x) => *x as u8,
                    pumpkin_nbt::tag::NbtTag::Int(x) => *x as u8,
                    pumpkin_nbt::tag::NbtTag::Short(x) => *x as u8,
                    pumpkin_nbt::tag::NbtTag::String(s) => {
                        let name = s.strip_prefix("minecraft:").unwrap_or(s);
                        pumpkin_data::biome::Biome::from_name(name).map_or(0, |b| b.id)
                    }
                    _ => 0,
                })
                .collect();
            Some(bytes)
        }
        _ => None,
    }
}

fn parse_scheduled_tick<T>(nbt: &pumpkin_nbt::compound::NbtCompound) -> Option<ScheduledTick<T>>
where
    T: FromResourceLocation,
{
    let x = nbt.get_int("x")?;
    let y = nbt.get_int("y")?;
    let z = nbt.get_int("z")?;
    let delay = nbt.get_int("t")? as u8;
    let priority = TickPriority::try_from(nbt.get_int("p")?).ok()?;
    let res_loc_str = nbt.get_string("i")?;
    let res_loc = ResourceLocation::from_str(res_loc_str).ok()?;
    let value = T::from_resource_location(&res_loc)?;
    Some(ScheduledTick {
        delay,
        priority,
        position: BlockPos::new(x, y, z),
        value,
    })
}

impl ChunkData {
    #[allow(clippy::too_many_lines)]
    pub fn internal_from_bytes(
        chunk_data: &[u8],
        position: Vector2<i32>,
    ) -> Result<Self, ChunkParsingError> {
        let is_named = chunk_data.len() >= 3
            && chunk_data[0] == 0x0a
            && chunk_data[1] == 0x00
            && chunk_data[2] == 0x00;

        let mut cursor = std::io::Cursor::new(chunk_data);
        let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
        let nbt = if is_named {
            pumpkin_nbt::Nbt::read(&mut reader)
        } else {
            pumpkin_nbt::Nbt::read_unnamed(&mut reader)
        }
        .map_err(|e| ChunkParsingError::ErrorDeserializingChunk(e.to_string()))?;

        let root_tag = nbt.root_tag;

        let x_pos = root_tag.get_int("xPos").ok_or_else(|| {
            ChunkParsingError::ErrorDeserializingChunk("Missing xPos".to_string())
        })?;
        let z_pos = root_tag.get_int("zPos").ok_or_else(|| {
            ChunkParsingError::ErrorDeserializingChunk("Missing zPos".to_string())
        })?;

        if x_pos != position.x || z_pos != position.y {
            return Err(ChunkParsingError::ErrorDeserializingChunk(format!(
                "Expected data for chunk {},{} but got it for {},{}!",
                position.x, position.y, x_pos, z_pos,
            )));
        }

        // Vanilla omits yPos when it upgrades a world in place. It uses the
        // dimension minimum for such chunks, which is the lowest section that
        // stores biomes. Do the same instead of rejecting the chunk.
        let min_y_section = match root_tag.get_int("yPos") {
            Some(y_pos) => y_pos,
            None => lowest_biome_section_y(&root_tag).ok_or_else(|| {
                ChunkParsingError::ErrorDeserializingChunk("Missing yPos".to_string())
            })?,
        };

        // Section indices are bytes in this format, and the loops below already
        // treat them as such. Reject a yPos outside that range instead of
        // truncating it, because the untruncated value sizes the vectors below.
        let Ok(min_y_section) = i8::try_from(min_y_section) else {
            return Err(ChunkParsingError::ErrorDeserializingChunk(format!(
                "yPos {min_y_section} is outside the range of a section index"
            )));
        };

        let mut max_y_section = min_y_section;
        if let Some(sections_list) = root_tag.get_list("sections") {
            for section_tag in sections_list {
                if let pumpkin_nbt::tag::NbtTag::Compound(section_compound) = section_tag {
                    let y = section_y(section_compound) as i8;
                    if y > max_y_section {
                        max_y_section = y;
                    }
                }
            }
        }

        let section_count =
            (i32::from(max_y_section) - i32::from(min_y_section) + 1).max(0) as usize;
        let mut block_lights = vec![LightContainer::Empty(0); section_count];
        let mut sky_lights = vec![LightContainer::Empty(0); section_count];
        let mut block_palettes = vec![BlockPalette::default(); section_count];
        let mut biome_palettes = vec![BiomePalette::default(); section_count];

        if let Some(sections_list) = root_tag.get_list("sections") {
            for section_tag in sections_list {
                if let pumpkin_nbt::tag::NbtTag::Compound(section_compound) = section_tag {
                    let y = section_y(section_compound);
                    let Ok(index) = usize::try_from(y.saturating_sub(i32::from(min_y_section)))
                    else {
                        continue;
                    };
                    if index >= section_count {
                        continue;
                    }

                    let block_light = section_compound
                        .get("BlockLight")
                        .and_then(|tag| tag.extract_byte_array())
                        .map(|arr| -> Box<[u8]> {
                            // SAFETY: `arr` is an `i8` slice (`&[i8]`). `u8` and `i8` have identical memory layout, alignment (1 byte), and lifetime.
                            unsafe {
                                Box::from(std::slice::from_raw_parts(
                                    arr.as_ptr().cast::<u8>(),
                                    arr.len(),
                                ))
                            }
                        });

                    let sky_light = section_compound
                        .get("SkyLight")
                        .and_then(|tag| tag.extract_byte_array())
                        .map(|arr| -> Box<[u8]> {
                            // SAFETY: `arr` is an `i8` slice (`&[i8]`). `u8` and `i8` have identical memory layout, alignment (1 byte), and lifetime.
                            unsafe {
                                Box::from(std::slice::from_raw_parts(
                                    arr.as_ptr().cast::<u8>(),
                                    arr.len(),
                                ))
                            }
                        });

                    // `Full` skips the length check `LightContainer::new` makes,
                    // and every reader indexes it up to `ARRAY_SIZE`, so a
                    // short array from disk is dropped instead of stored.
                    block_lights[index] = block_light
                        .filter(|data| data.len() == LightContainer::ARRAY_SIZE)
                        .map_or(LightContainer::Empty(0), LightContainer::Full);
                    sky_lights[index] = sky_light
                        .filter(|data| data.len() == LightContainer::ARRAY_SIZE)
                        .map_or(LightContainer::Empty(0), LightContainer::Full);

                    if let Some(bs_compound) = section_compound.get_compound("block_states") {
                        let data = bs_compound
                            .get_long_array("data")
                            .map(|arr| arr.to_vec().into_boxed_slice());
                        let palette = bs_compound
                            .get("palette")
                            .and_then(extract_u16_array)
                            .unwrap_or_else(|| vec![BlockStateId::AIR].into_boxed_slice());

                        block_palettes[index] =
                            BlockPalette::from_disk_nbt(ChunkSectionBlockStates { data, palette });
                    } else {
                        block_palettes[index] = BlockPalette::default();
                    }

                    if let Some(b_compound) = section_compound.get_compound("biomes") {
                        let data = b_compound
                            .get_long_array("data")
                            .map(|arr| arr.to_vec().into_boxed_slice());
                        let palette = b_compound
                            .get("palette")
                            .and_then(extract_u8_array)
                            .unwrap_or_else(|| vec![0].into_boxed_slice());

                        biome_palettes[index] =
                            BiomePalette::from_disk_nbt(ChunkSectionBiomes { data, palette });
                    } else {
                        biome_palettes[index] = BiomePalette::default();
                    }
                }
            }
        }

        // Assemble the LightEngine
        let light_engine = ChunkLight {
            block_light: block_lights.into_boxed_slice(),
            sky_light: sky_lights.into_boxed_slice(),
        };

        // Assemble the ChunkSections
        let min_y = section_coords::section_to_block(i32::from(min_y_section));
        let (random_tick_sections, randomly_ticking_mask) =
            ChunkSections::build_random_tick_sections_cache(&block_palettes);
        let section = ChunkSections {
            count: block_palettes.len(),
            block_sections: RwLock::new(block_palettes.into_boxed_slice()),
            random_tick_sections: RwLock::new(random_tick_sections),
            randomly_ticking_mask: std::sync::atomic::AtomicU32::new(randomly_ticking_mask),
            biome_sections: RwLock::new(biome_palettes.into_boxed_slice()),
            min_y,
        };

        let heightmaps = root_tag.get_compound("Heightmaps").map_or(
            ChunkHeightmaps {
                world_surface: None,
                motion_blocking: None,
                motion_blocking_no_leaves: None,
            },
            // A heightmap of the wrong length is dropped rather than stored:
            // every reader indexes it by column and would run off the end.
            |h_compound| ChunkHeightmaps {
                world_surface: h_compound
                    .get_long_array("WORLD_SURFACE")
                    .filter(|a| a.len() == ChunkHeightmaps::LONGS)
                    .map(|a| a.to_vec().into_boxed_slice()),
                motion_blocking: h_compound
                    .get_long_array("MOTION_BLOCKING")
                    .filter(|a| a.len() == ChunkHeightmaps::LONGS)
                    .map(|a| a.to_vec().into_boxed_slice()),
                motion_blocking_no_leaves: h_compound
                    .get_long_array("MOTION_BLOCKING_NO_LEAVES")
                    .filter(|a| a.len() == ChunkHeightmaps::LONGS)
                    .map(|a| a.to_vec().into_boxed_slice()),
            },
        );
        let mut block_ticks = Vec::new();
        if let Some(list) = root_tag.get_list("block_ticks") {
            for tag in list {
                if let pumpkin_nbt::tag::NbtTag::Compound(compound) = tag
                    && let Some(tick) = parse_scheduled_tick::<&'static Block>(compound)
                {
                    block_ticks.push(tick);
                }
            }
        }

        let mut fluid_ticks = Vec::new();
        if let Some(list) = root_tag.get_list("fluid_ticks") {
            for tag in list {
                if let pumpkin_nbt::tag::NbtTag::Compound(compound) = tag
                    && let Some(tick) = parse_scheduled_tick::<&'static Fluid>(compound)
                {
                    fluid_ticks.push(tick);
                }
            }
        }

        let mut block_entities = FxHashMap::default();
        if let Some(list) = root_tag.get_list("block_entities") {
            for tag in list {
                if let pumpkin_nbt::tag::NbtTag::Compound(nbt) = tag
                    && let Some(x) = nbt.get_int("x")
                    && let Some(y) = nbt.get_int("y")
                    && let Some(z) = nbt.get_int("z")
                {
                    block_entities.insert(BlockPos::new(x, y, z), nbt.clone());
                }
            }
        }

        let light_correct = root_tag.get_bool("isLightOn").unwrap_or(false);

        let status_str = root_tag.get_string("Status").unwrap_or("minecraft:empty");
        let status = match status_str {
            "minecraft:structure_starts" => ChunkStatus::StructureStarts,
            "minecraft:structure_references" => ChunkStatus::StructureReferences,
            "minecraft:biomes" => ChunkStatus::Biomes,
            "minecraft:terrain" | "minecraft:noise" | "minecraft:surface" | "minecraft:carvers" => {
                ChunkStatus::Terrain
            }
            "minecraft:features" => ChunkStatus::Features,
            "minecraft:initialize_light" => ChunkStatus::InitializeLight,
            "minecraft:light" => ChunkStatus::Light,
            "minecraft:spawn" => ChunkStatus::Spawn,
            "minecraft:full" => ChunkStatus::Full,
            _ => ChunkStatus::Empty,
        };

        let custom_data = root_tag
            .get_compound("PumpkinCustomData")
            .or_else(|| root_tag.get_compound("BukkitValues"))
            .cloned()
            .unwrap_or_default();

        Ok(Self {
            section,
            heightmap: std::sync::Mutex::new(heightmaps),
            x: position.x,
            z: position.y,
            // This chunk is read from disk, so it has not been modified
            dirty: AtomicBool::new(false),
            block_ticks: ChunkTickScheduler::from_iter(block_ticks),
            fluid_ticks: ChunkTickScheduler::from_iter(fluid_ticks),
            pending_block_entities: std::sync::Mutex::new(block_entities),
            light_engine: std::sync::Mutex::new(light_engine),
            light_populated: AtomicBool::new(light_correct),
            status,
            blending_data: None,
            inhabited_time: AtomicU64::new(root_tag.get_long("InhabitedTime").unwrap_or(0) as u64),
            custom_data: std::sync::Mutex::new(custom_data),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn internal_to_bytes(&self) -> Bytes {
        use pumpkin_nbt::tag::NbtTag;

        fn extract_light_ref(light: Option<&LightContainer>) -> Option<&[u8]> {
            match light {
                Some(LightContainer::Full(data)) => Some(data.as_ref()),
                _ => None,
            }
        }

        let is_light_correct = self
            .light_populated
            .load(std::sync::atomic::Ordering::Relaxed);

        let block_entities_nbt = {
            let entities_guard = self
                .pending_block_entities
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            entities_guard.values().cloned().collect::<Vec<_>>()
        };

        let light_lock = self
            .light_engine
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let heightmap_lock = self
            .heightmap
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let block_lock = self
            .section
            .block_sections
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let biome_lock = self
            .section
            .biome_sections
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let min_section_y = (self.section.min_y >> 4) as i8;

        let mut root_compound = NbtCompound::new();
        root_compound.put_int("DataVersion", WORLD_DATA_VERSION);
        root_compound.put_int("xPos", self.x);
        root_compound.put_int("zPos", self.z);
        root_compound.put_int("yPos", section_coords::block_to_section(self.section.min_y));

        let status_str = match self.status {
            ChunkStatus::Empty => "minecraft:empty",
            ChunkStatus::StructureStarts => "minecraft:structure_starts",
            ChunkStatus::StructureReferences => "minecraft:structure_references",
            ChunkStatus::Biomes => "minecraft:biomes",
            ChunkStatus::Terrain => "minecraft:terrain",
            ChunkStatus::Features => "minecraft:features",
            ChunkStatus::InitializeLight => "minecraft:initialize_light",
            ChunkStatus::Light => "minecraft:light",
            ChunkStatus::Spawn => "minecraft:spawn",
            ChunkStatus::Full => "minecraft:full",
        };
        root_compound.put_string("Status", status_str.to_string());

        let mut heightmaps_compound = NbtCompound::new();
        if let Some(ref arr) = heightmap_lock.world_surface {
            heightmaps_compound.put("WORLD_SURFACE", NbtTag::LongArray(arr.to_vec()));
        }
        if let Some(ref arr) = heightmap_lock.motion_blocking {
            heightmaps_compound.put("MOTION_BLOCKING", NbtTag::LongArray(arr.to_vec()));
        }
        if let Some(ref arr) = heightmap_lock.motion_blocking_no_leaves {
            heightmaps_compound.put("MOTION_BLOCKING_NO_LEAVES", NbtTag::LongArray(arr.to_vec()));
        }
        root_compound.put_compound("Heightmaps", heightmaps_compound);

        let mut sections_list = Vec::new();
        for i in 0..self.section.count {
            let mut section_comp = NbtCompound::new();
            let y_val = i as i8 + min_section_y;
            section_comp.put_byte("Y", y_val);

            // block_states
            let block_states_nbt = block_lock[i].to_disk_nbt();
            let mut bs_comp = NbtCompound::new();
            if let Some(ref data_arr) = block_states_nbt.data {
                bs_comp.put("data", NbtTag::LongArray(data_arr.to_vec()));
            }
            let palette_tags: Vec<NbtTag> = block_states_nbt
                .palette
                .iter()
                .copied()
                .map(block_state_palette_tag)
                .collect();
            bs_comp.put_list("palette", palette_tags);
            section_comp.put_compound("block_states", bs_comp);

            // biomes
            let biomes_nbt = biome_lock[i].to_disk_nbt();
            let mut b_comp = NbtCompound::new();
            if let Some(ref data_arr) = biomes_nbt.data {
                b_comp.put("data", NbtTag::LongArray(data_arr.to_vec()));
            }
            let biome_palette_tags: Vec<NbtTag> = biomes_nbt
                .palette
                .iter()
                .map(|&val| {
                    let name = pumpkin_data::biome::Biome::from_id(val)
                        .map_or("plains", |b| b.registry_id);
                    let full_name = if name.starts_with("minecraft:") {
                        name.to_string()
                    } else {
                        format!("minecraft:{name}")
                    };
                    NbtTag::String(full_name.into())
                })
                .collect();
            b_comp.put_list("palette", biome_palette_tags);
            section_comp.put_compound("biomes", b_comp);

            // block_light
            if let Some(light_data) = extract_light_ref(light_lock.block_light.get(i)) {
                let bytes: Box<[i8]> = light_data.iter().map(|&x| x as i8).collect();
                section_comp.put("BlockLight", NbtTag::ByteArray(bytes));
            }

            // sky_light
            if let Some(light_data) = extract_light_ref(light_lock.sky_light.get(i)) {
                let bytes: Box<[i8]> = light_data.iter().map(|&x| x as i8).collect();
                section_comp.put("SkyLight", NbtTag::ByteArray(bytes));
            }

            sections_list.push(NbtTag::Compound(section_comp));
        }
        root_compound.put_list("sections", sections_list);

        let mut block_ticks_list = Vec::new();
        for tick in self.block_ticks.to_vec() {
            let mut tick_comp = NbtCompound::new();
            tick_comp.put_int("x", tick.position.0.x);
            tick_comp.put_int("y", tick.position.0.y);
            tick_comp.put_int("z", tick.position.0.z);
            tick_comp.put_int("t", tick.delay as i32);
            tick_comp.put_int("p", tick.priority as i32);
            tick_comp.put_string("i", tick.value.to_resource_location());
            block_ticks_list.push(NbtTag::Compound(tick_comp));
        }
        root_compound.put_list("block_ticks", block_ticks_list);

        let mut fluid_ticks_list = Vec::new();
        for tick in self.fluid_ticks.to_vec() {
            let mut tick_comp = NbtCompound::new();
            tick_comp.put_int("x", tick.position.0.x);
            tick_comp.put_int("y", tick.position.0.y);
            tick_comp.put_int("z", tick.position.0.z);
            tick_comp.put_int("t", tick.delay as i32);
            tick_comp.put_int("p", tick.priority as i32);
            tick_comp.put_string("i", tick.value.to_resource_location());
            fluid_ticks_list.push(NbtTag::Compound(tick_comp));
        }
        root_compound.put_list("fluid_ticks", fluid_ticks_list);

        let mut block_entities_list = Vec::new();
        for entity_comp in block_entities_nbt {
            block_entities_list.push(NbtTag::Compound(entity_comp));
        }
        root_compound.put_list("block_entities", block_entities_list);

        root_compound.put_bool("isLightOn", is_light_correct);
        root_compound.put_long(
            "InhabitedTime",
            self.inhabited_time.load(Ordering::Relaxed) as i64,
        );

        let custom_data = self
            .custom_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !custom_data.is_empty() {
            root_compound.put_compound("PumpkinCustomData", custom_data.clone());
        }

        let nbt = pumpkin_nbt::Nbt::from(root_compound);
        nbt.write()
    }

    pub fn set_custom_data(&self, namespace: &str, key: &str, value: pumpkin_nbt::tag::NbtTag) {
        let mut custom_data = self
            .custom_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut namespace_data = custom_data
            .child_tags
            .remove(namespace)
            .and_then(|tag| match tag {
                pumpkin_nbt::tag::NbtTag::Compound(compound) => Some(compound),
                _ => None,
            })
            .unwrap_or_default();

        namespace_data.child_tags.insert(key.into(), value);
        custom_data.child_tags.insert(
            namespace.into(),
            pumpkin_nbt::tag::NbtTag::Compound(namespace_data),
        );
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn get_custom_data(&self, namespace: &str, key: &str) -> Option<pumpkin_nbt::tag::NbtTag> {
        let custom_data = self
            .custom_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        custom_data
            .get(namespace)?
            .extract_compound()?
            .get(key)
            .cloned()
    }

    pub fn remove_custom_data(&self, namespace: &str, key: &str) {
        let mut custom_data = self
            .custom_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(pumpkin_nbt::tag::NbtTag::Compound(mut namespace_data)) =
            custom_data.child_tags.remove(namespace)
        else {
            return;
        };

        namespace_data.child_tags.remove(key);
        if !namespace_data.is_empty() {
            custom_data.child_tags.insert(
                namespace.into(),
                pumpkin_nbt::tag::NbtTag::Compound(namespace_data),
            );
        }
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn has_custom_data(&self, namespace: &str, key: &str) -> bool {
        self.get_custom_data(namespace, key).is_some()
    }
}

impl PathFromLevelFolder for ChunkEntityData {
    #[inline]
    fn file_path(folder: &LevelFolder, file_name: &str) -> PathBuf {
        folder.entities_folder.join(file_name)
    }
}

impl Dirtiable for ChunkEntityData {
    #[inline]
    fn mark_dirty(&self, flag: bool) {
        self.dirty.store(flag, Ordering::Relaxed);
    }

    #[inline]
    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }
}

impl SingleChunkDataSerializer for ChunkEntityData {
    #[inline]
    fn from_bytes(bytes: &Bytes, pos: Vector2<i32>) -> Result<Self, ChunkReadingError> {
        Self::internal_from_bytes(bytes, pos).map_err(ChunkReadingError::ParsingError)
    }

    #[inline]
    fn to_bytes(&self) -> Result<Bytes, ChunkSerializingError> {
        Ok(self.internal_to_bytes())
    }

    #[inline]
    fn position(&self) -> (i32, i32) {
        (self.x, self.z)
    }
}

impl ChunkEntityData {
    fn internal_from_bytes(
        chunk_data: &[u8],
        position: Vector2<i32>,
    ) -> Result<Self, ChunkParsingError> {
        let is_named = chunk_data.len() >= 3
            && chunk_data[0] == 0x0a
            && chunk_data[1] == 0x00
            && chunk_data[2] == 0x00;
        let mut cursor = std::io::Cursor::new(chunk_data);
        let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
            pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
        );
        let nbt = if is_named {
            pumpkin_nbt::Nbt::read(&mut reader)
        } else {
            pumpkin_nbt::Nbt::read_unnamed(&mut reader)
        }
        .map_err(|e| ChunkParsingError::ErrorDeserializingChunk(e.to_string()))?;

        let pos_array = match (nbt.get_int("Position-X"), nbt.get_int("Position-Z")) {
            (Some(x), Some(z)) => [x, z],
            _ => {
                if let Some(pumpkin_nbt::tag::NbtTag::IntArray(pos)) = nbt.get("Position") {
                    if pos.len() >= 2 {
                        [pos[0], pos[1]]
                    } else {
                        [0, 0]
                    }
                } else {
                    [0, 0]
                }
            }
        };

        if pos_array[0] != position.x || pos_array[1] != position.y {
            return Err(ChunkParsingError::ErrorDeserializingChunk(format!(
                "Expected data for entity chunk {},{} but got it for {},{}!",
                position.x, position.y, pos_array[0], pos_array[1],
            )));
        }

        let entities = match nbt.get("Entities") {
            Some(pumpkin_nbt::tag::NbtTag::List(list)) => list
                .iter()
                .filter_map(|t| match t {
                    pumpkin_nbt::tag::NbtTag::Compound(c) => Some(c.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };

        Ok(Self {
            x: position.x,
            z: position.y,
            data: std::sync::Mutex::new(entities),
            live: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
        })
    }

    fn internal_to_bytes(&self) -> Bytes {
        let mut root = NbtCompound::new();
        root.put_int("DataVersion", WORLD_DATA_VERSION);
        root.put(
            "Position",
            pumpkin_nbt::tag::NbtTag::IntArray(vec![self.x, self.z]),
        );
        let entities_tag: Vec<pumpkin_nbt::tag::NbtTag> = self
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|c| pumpkin_nbt::tag::NbtTag::Compound(c.clone()))
            .collect();
        root.put_list("Entities", entities_tag);

        let nbt = pumpkin_nbt::Nbt::from(root);
        nbt.write()
    }
}

#[derive(Clone)]
pub struct ChunkSectionBiomes {
    pub(crate) data: Option<Box<[i64]>>,
    pub(crate) palette: Box<[u8]>,
}

#[derive(Clone)]
pub struct ChunkSectionBlockStates {
    pub(crate) data: Option<Box<[i64]>>,
    pub(crate) palette: Box<[BlockStateId]>,
}

#[derive(Debug, Clone)]
pub enum LightContainer {
    Empty(u8),
    Full(Box<[u8]>),
}

impl LightContainer {
    pub const DIM: usize = 16;
    pub const ARRAY_SIZE: usize = Self::DIM * Self::DIM * Self::DIM / 2;

    #[must_use]
    pub fn new_empty(default: u8) -> Self {
        assert!(default <= 15, "Default value must be between 0 and 15");
        Self::Empty(default)
    }

    #[must_use]
    pub fn new(data: Box<[u8]>) -> Self {
        assert!(
            data.len() == Self::ARRAY_SIZE,
            "Data length must be {}",
            Self::ARRAY_SIZE
        );
        Self::Full(data)
    }

    #[must_use]
    pub fn new_filled(default: u8) -> Self {
        assert!(default <= 15, "Default value must be between 0 and 15");
        let value = default << 4 | default;
        Self::Full([value; Self::ARRAY_SIZE].into())
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty(_))
    }

    #[inline]
    const fn index(x: usize, y: usize, z: usize) -> usize {
        y * 16 * 16 + z * 16 + x
    }

    #[inline]
    #[must_use]
    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        match self {
            Self::Full(data) => {
                let index = Self::index(x, y, z);
                (data[index >> 1] >> (4 * (index & 1))) & 0x0F
            }
            Self::Empty(default) => *default,
        }
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, value: u8) {
        match self {
            Self::Full(data) => {
                let index = Self::index(x, y, z);
                let shift = 4 * (index & 1);
                let mask = 0x0F << shift;
                data[index >> 1] = (data[index >> 1] & !mask) | (value << shift);
            }
            Self::Empty(default) => {
                if value != *default {
                    *self = Self::new_filled(*default);
                    self.set(x, y, z, value);
                }
            }
        }
    }

    #[inline]
    pub fn set_column_y_range(
        &mut self,
        x: usize,
        z: usize,
        y_start: usize,
        y_end: usize,
        value: u8,
    ) {
        if y_start >= y_end {
            return;
        }
        match self {
            Self::Full(data) => {
                let shift = 4 * (x & 1);
                let mask = 0x0F << shift;
                let val = (value & 0x0F) << shift;
                let mut byte_idx = (y_start * 256 + z * 16 + x) >> 1;
                for _ in y_start..y_end {
                    data[byte_idx] = (data[byte_idx] & !mask) | val;
                    byte_idx += 128;
                }
            }
            Self::Empty(default) => {
                if value != *default {
                    *self = Self::new_filled(*default);
                    self.set_column_y_range(x, z, y_start, y_end, value);
                }
            }
        }
    }

    #[inline]
    pub fn fill(&mut self, value: u8) {
        *self = Self::new_filled(value);
    }
}

impl Default for LightContainer {
    fn default() -> Self {
        Self::new_empty(15)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::Block;
    use pumpkin_nbt::compound::NbtCompound;
    use pumpkin_nbt::tag::NbtTag;

    fn test_section(y: i32, block: &str, with_biomes: bool) -> NbtCompound {
        let mut block_states = NbtCompound::new();
        let mut air = NbtCompound::new();
        air.put_string("Name", "minecraft:air".to_string());
        let mut solid = NbtCompound::new();
        solid.put_string("Name", block.to_string());
        block_states.put(
            "palette",
            NbtTag::List(vec![NbtTag::Compound(air), NbtTag::Compound(solid)]),
        );
        block_states.put("data", NbtTag::LongArray(vec![1; 256]));

        let mut section = NbtCompound::new();
        section.put_int("Y", y);
        section.put("block_states", NbtTag::Compound(block_states));
        if with_biomes {
            let mut biomes = NbtCompound::new();
            biomes.put(
                "palette",
                NbtTag::List(vec![NbtTag::String("minecraft:plains".into())]),
            );
            section.put("biomes", NbtTag::Compound(biomes));
        }
        section
    }

    fn test_chunk(sections: Vec<NbtCompound>) -> pumpkin_nbt::Nbt {
        let mut root = NbtCompound::new();
        root.put_int("DataVersion", 4903);
        root.put_int("xPos", 0);
        root.put_int("zPos", 0);
        root.put_string("Status", "minecraft:full".to_string());
        root.put(
            "sections",
            NbtTag::List(sections.into_iter().map(NbtTag::Compound).collect()),
        );
        pumpkin_nbt::Nbt::new(String::new(), root)
    }

    #[test]
    fn chunk_without_y_pos_uses_the_lowest_biome_section() {
        use crate::chunk::ChunkData;
        use pumpkin_util::math::vector2::Vector2;

        // The datafixer writes Y as an int, so the test must too.
        let bytes = test_chunk(vec![test_section(-4, "minecraft:stone", true)]).write();
        let chunk =
            ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("chunk without yPos parses");
        assert_eq!(
            chunk.section.get_block_absolute_y(0, -64, 0),
            Some(Block::STONE.default_state.id)
        );
    }

    #[test]
    fn chunk_with_int_y_sections_keeps_every_section() {
        use crate::chunk::ChunkData;
        use pumpkin_util::math::vector2::Vector2;

        let bytes = test_chunk(vec![
            test_section(-4, "minecraft:stone", true),
            test_section(0, "minecraft:dirt", true),
        ])
        .write();
        let chunk = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("int Y chunk parses");
        assert_eq!(
            chunk.section.get_block_absolute_y(0, -64, 0),
            Some(Block::STONE.default_state.id)
        );
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(Block::DIRT.default_state.id)
        );
    }

    #[test]
    fn chunk_with_mixed_numeric_y_tags_keeps_sections() {
        use crate::chunk::ChunkData;
        use pumpkin_util::math::vector2::Vector2;

        // What a real upgraded chunk looks like: the map's own sections store Y
        // as a byte, the datafixer writes ints. The int section must still land
        // at its own height instead of collapsing to Y = 0.
        let mut byte_section = test_section(-4, "minecraft:air", true);
        byte_section.put_byte("Y", -4);

        let bytes = test_chunk(vec![
            byte_section,
            test_section(0, "minecraft:stone", true),
            test_section(4, "minecraft:dirt", true),
        ])
        .write();
        let chunk =
            ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("mixed Y chunk parses");
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(Block::STONE.default_state.id)
        );
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 64, 0),
            Some(Block::DIRT.default_state.id)
        );
    }

    #[test]
    fn fallback_ignores_light_only_sections_below_zero() {
        use crate::chunk::ChunkData;
        use pumpkin_util::math::vector2::Vector2;

        // Old worlds keep a light grid at Y = -1 after the upgrade. It has no
        // biomes and must not decide where the chunk starts.
        let mut light_section = NbtCompound::new();
        light_section.put_byte("Y", -1);
        light_section.put(
            "BlockLight",
            NbtTag::ByteArray(vec![0x0Fi8; 2048].into_boxed_slice()),
        );

        let bytes = test_chunk(vec![
            light_section,
            test_section(0, "minecraft:stone", true),
        ])
        .write();
        let chunk =
            ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("light section ignored");
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(Block::STONE.default_state.id)
        );
        assert_eq!(chunk.section.get_block_absolute_y(0, -1, 0), None);
    }

    #[test]
    fn fallback_never_goes_above_zero() {
        use crate::chunk::ChunkData;
        use pumpkin_util::math::vector2::Vector2;

        let bytes = test_chunk(vec![test_section(1, "minecraft:stone", true)]).write();
        let chunk = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("caps at zero");
        // The only section sits at Y=1, so the cap has to bring the chunk down
        // to Y=0. Without the cap the chunk would start at Y=1 and a block at
        // Y=0 would be out of range.
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(Block::AIR.default_state.id)
        );
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 16, 0),
            Some(Block::STONE.default_state.id)
        );
    }

    #[test]
    fn chunk_without_y_pos_or_biomes_still_fails() {
        use crate::chunk::ChunkData;
        use pumpkin_util::math::vector2::Vector2;

        let mut light_section = NbtCompound::new();
        light_section.put_byte("Y", -1);
        light_section.put(
            "BlockLight",
            NbtTag::ByteArray(vec![0x0Fi8; 2048].into_boxed_slice()),
        );

        let bytes = test_chunk(vec![light_section]).write();
        let Err(error) = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)) else {
            panic!("chunk without yPos and without biomes must fail");
        };
        assert!(format!("{error:?}").contains("Missing yPos"));
    }

    #[test]
    fn extract_u16_array_from_vanilla_compound_palette() {
        let mut entry1 = NbtCompound::new();
        entry1.put_string("Name", "minecraft:stone".to_string());

        let mut entry2 = NbtCompound::new();
        entry2.put_string("Name", "minecraft:repeater".to_string());
        let mut props = NbtCompound::new();
        props.put_string("facing", "north".to_string());
        props.put_string("delay", "2".to_string());
        props.put_string("locked", "false".to_string());
        props.put_string("powered", "false".to_string());
        entry2.put_compound("Properties", props);

        let list_tag = NbtTag::List(vec![NbtTag::Compound(entry1), NbtTag::Compound(entry2)]);
        let result = extract_u16_array(&list_tag).expect("should extract palette");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], Block::STONE.default_state.id);

        let repeater_state = Block::REPEATER
            .from_properties(&[
                ("facing", "north"),
                ("delay", "2"),
                ("locked", "false"),
                ("powered", "false"),
            ])
            .to_state_id(&Block::REPEATER);
        assert_eq!(result[1], repeater_state);
    }

    #[test]
    fn extract_u8_array_from_vanilla_string_palette() {
        let list_tag = NbtTag::List(vec![
            NbtTag::String("minecraft:plains".to_string().into()),
            NbtTag::String("minecraft:the_void".to_string().into()),
        ]);
        let result = extract_u8_array(&list_tag).expect("should extract biome palette");

        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0],
            pumpkin_data::biome::Biome::from_name("plains").unwrap().id
        );
        assert_eq!(
            result[1],
            pumpkin_data::biome::Biome::from_name("the_void")
                .unwrap()
                .id
        );
    }

    fn east_oak_stairs() -> BlockStateId {
        Block::OAK_STAIRS
            .from_properties(&[
                ("facing", "east"),
                ("half", "bottom"),
                ("shape", "straight"),
                ("waterlogged", "false"),
            ])
            .to_state_id(&Block::OAK_STAIRS)
    }

    fn stairs_properties() -> NbtCompound {
        let mut props = NbtCompound::new();
        props.put_string("facing", "east".to_string());
        props.put_string("half", "bottom".to_string());
        props.put_string("shape", "straight".to_string());
        props.put_string("waterlogged", "false".to_string());
        props
    }

    fn load_palette_section(palette: Vec<NbtTag>) -> crate::chunk::ChunkData {
        use crate::chunk::ChunkData;
        use pumpkin_util::math::vector2::Vector2;

        let mut block_states = NbtCompound::new();
        block_states.put("palette", NbtTag::List(palette));
        let mut biomes = NbtCompound::new();
        biomes.put(
            "palette",
            NbtTag::List(vec![NbtTag::String("minecraft:plains".into())]),
        );
        let mut section = NbtCompound::new();
        section.put_byte("Y", 0);
        section.put("block_states", NbtTag::Compound(block_states));
        section.put("biomes", NbtTag::Compound(biomes));

        let mut root = NbtCompound::new();
        root.put_int("DataVersion", 4903);
        root.put_int("xPos", 0);
        root.put_int("zPos", 0);
        root.put_int("yPos", 0);
        root.put_string("Status", "minecraft:full".to_string());
        root.put("sections", NbtTag::List(vec![NbtTag::Compound(section)]));
        let bytes = pumpkin_nbt::Nbt::from(root).write();
        ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("palette section parses")
    }

    fn read_root(bytes: &[u8]) -> NbtCompound {
        let mut cursor = std::io::Cursor::new(bytes);
        let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
        pumpkin_nbt::Nbt::read(&mut reader)
            .expect("chunk nbt")
            .root_tag
    }

    fn section_by_y(root: &NbtCompound, y: i32) -> &NbtCompound {
        root.get_list("sections")
            .expect("sections")
            .iter()
            .find_map(|tag| {
                let compound = tag.extract_compound()?;
                (section_y(compound) == y).then_some(compound)
            })
            .expect("section")
    }

    fn block_palette(section: &NbtCompound) -> &[NbtTag] {
        section
            .get_compound("block_states")
            .expect("block_states")
            .get_list("palette")
            .expect("palette")
    }

    #[test]
    fn reads_4903_palette_including_default_states_that_carry_properties() {
        let mut deepslate = NbtCompound::new();
        deepslate.put_string("Name", "minecraft:deepslate".to_string());
        let mut axis = NbtCompound::new();
        axis.put_string("axis", "y".to_string());
        deepslate.put_compound("Properties", axis);
        let chunk = load_palette_section(vec![NbtTag::Compound(deepslate)]);
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(Block::DEEPSLATE.default_state.id)
        );

        let mut furnace = NbtCompound::new();
        furnace.put_string("Name", "minecraft:furnace".to_string());
        let mut props = NbtCompound::new();
        props.put_string("facing", "north".to_string());
        props.put_string("lit", "false".to_string());
        furnace.put_compound("Properties", props);
        let chunk = load_palette_section(vec![NbtTag::Compound(furnace)]);
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(Block::FURNACE.default_state.id)
        );

        let mut stairs = NbtCompound::new();
        stairs.put_string("Name", "minecraft:oak_stairs".to_string());
        stairs.put_compound("Properties", stairs_properties());
        let chunk = load_palette_section(vec![NbtTag::Compound(stairs)]);
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(east_oak_stairs())
        );

        let mut air = NbtCompound::new();
        air.put_string("Name", "minecraft:air".to_string());
        let chunk = load_palette_section(vec![NbtTag::Compound(air)]);
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(Block::AIR.default_state.id)
        );
    }

    #[test]
    fn reads_5023_string_palette_as_default_states() {
        let chunk = load_palette_section(vec![NbtTag::String("minecraft:deepslate".into())]);
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(Block::DEEPSLATE.default_state.id)
        );
        let chunk = load_palette_section(vec![NbtTag::String("minecraft:bedrock".into())]);
        assert_eq!(
            chunk.section.get_block_absolute_y(0, 0, 0),
            Some(Block::BEDROCK.default_state.id)
        );

        let list = NbtTag::List(vec![
            NbtTag::String("minecraft:air".into()),
            NbtTag::String("minecraft:stone".into()),
            NbtTag::String("minecraft:furnace".into()),
        ]);
        let ids = extract_u16_array(&list).expect("string palette");
        assert_eq!(
            ids.as_ref(),
            [
                Block::AIR.default_state.id,
                Block::STONE.default_state.id,
                Block::FURNACE.default_state.id,
            ]
        );
    }

    #[test]
    fn reads_5023_empty_named_default_entries_in_a_mixed_palette() {
        let mut air = NbtCompound::new();
        air.put_string("", "minecraft:air".to_string());
        let mut stone = NbtCompound::new();
        stone.put_string("", "minecraft:stone".to_string());
        let mut furnace = NbtCompound::new();
        furnace.put_string("", "minecraft:furnace".to_string());
        let mut stairs = NbtCompound::new();
        stairs.put_string("id", "minecraft:oak_stairs".to_string());
        stairs.put_compound("properties", stairs_properties());

        let list = NbtTag::List(vec![
            NbtTag::Compound(air),
            NbtTag::Compound(stone),
            NbtTag::Compound(furnace),
            NbtTag::Compound(stairs),
        ]);
        let ids = extract_u16_array(&list).expect("mixed palette");
        assert_eq!(
            ids.as_ref(),
            [
                Block::AIR.default_state.id,
                Block::STONE.default_state.id,
                Block::FURNACE.default_state.id,
                east_oak_stairs(),
            ]
        );
        assert_ne!(east_oak_stairs(), Block::OAK_STAIRS.default_state.id);
    }

    #[test]
    fn writes_5023_palettes_and_round_trips_block_states() {
        use crate::chunk::ChunkData;
        use pumpkin_util::math::vector2::Vector2;

        let stairs = east_oak_stairs();
        let chunk = ChunkData::empty(2, -3);
        chunk.set_block_absolute_y(1, 0, 2, Block::STONE.default_state.id);
        chunk.set_block_absolute_y(2, 0, 2, Block::FURNACE.default_state.id);
        chunk.set_block_absolute_y(3, 0, 2, stairs);

        let bytes = ChunkData::to_bytes(&chunk).expect("serialize chunk");
        let root = read_root(&bytes);
        assert_eq!(root.get_int("DataVersion"), Some(5023));

        let air_section = section_by_y(&root, -4);
        let air_palette = block_palette(air_section);
        assert!(
            air_palette
                .iter()
                .all(|tag| matches!(tag, NbtTag::String(_))),
            "a section of only default states is a string list"
        );
        assert!(
            matches!(&air_palette[0], NbtTag::String(name) if name.as_ref() == "minecraft:air")
        );
        let biomes = air_section
            .get_compound("biomes")
            .expect("biomes")
            .get_list("palette")
            .expect("biome palette");
        assert!(biomes.iter().all(|tag| matches!(tag, NbtTag::String(_))));

        let mixed = block_palette(section_by_y(&root, 0));
        assert!(
            mixed.iter().any(
                |tag| matches!(tag, NbtTag::String(name) if name.as_ref() == "minecraft:stone")
            ),
            "stone stays a block id"
        );
        assert!(
            mixed.iter().any(
                |tag| matches!(tag, NbtTag::String(name) if name.as_ref() == "minecraft:furnace")
            ),
            "a default furnace is the block id, not its properties"
        );
        let stairs_entry = mixed
            .iter()
            .find_map(|tag| match tag {
                NbtTag::Compound(compound)
                    if compound.get_string("id") == Some("minecraft:oak_stairs") =>
                {
                    Some(compound)
                }
                _ => None,
            })
            .expect("oak stairs compound");
        assert!(stairs_entry.get_string("Name").is_none());
        assert!(stairs_entry.get_compound("Properties").is_none());
        let props = stairs_entry.get_compound("properties").expect("properties");
        assert_eq!(props.get_string("facing"), Some("east"));
        assert_eq!(props.get_string("half"), Some("bottom"));
        assert_eq!(props.get_string("shape"), Some("straight"));
        assert_eq!(props.get_string("waterlogged"), Some("false"));

        // The bytes still contain the empty-named wrapper the list codec writes
        // for a default state that shares a compound list with the stairs.
        let wrapped_air = empty_named_block_id("minecraft:air");
        assert!(
            bytes
                .windows(wrapped_air.len())
                .any(|window| window == wrapped_air),
            "mixed palette stores default states under an empty name"
        );

        let loaded =
            ChunkData::from_bytes(&bytes, Vector2::new(2, -3)).expect("written chunk reloads");
        assert_eq!(
            loaded.section.get_block_absolute_y(0, 0, 0),
            Some(Block::AIR.default_state.id)
        );
        assert_eq!(
            loaded.section.get_block_absolute_y(1, 0, 2),
            Some(Block::STONE.default_state.id)
        );
        assert_eq!(
            loaded.section.get_block_absolute_y(2, 0, 2),
            Some(Block::FURNACE.default_state.id)
        );
        assert_eq!(loaded.section.get_block_absolute_y(3, 0, 2), Some(stairs));
        assert_eq!(
            loaded.section.get_block_absolute_y(0, -64, 0),
            Some(Block::AIR.default_state.id)
        );
    }

    fn empty_named_block_id(block_id: &str) -> Vec<u8> {
        let mut bytes = vec![8, 0, 0];
        bytes.extend(
            u16::try_from(block_id.len())
                .expect("block id length")
                .to_be_bytes(),
        );
        bytes.extend(block_id.as_bytes());
        bytes
    }

    fn find_block_entity<'a>(root: &'a NbtCompound, id: &str, x: i32) -> &'a NbtCompound {
        root.get_list("block_entities")
            .expect("block_entities")
            .iter()
            .find_map(|tag| {
                let compound = tag.extract_compound()?;
                (compound.get_string("id") == Some(id) && compound.get_int("x") == Some(x))
                    .then_some(compound)
            })
            .expect("block entity")
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn block_entity_and_entity_compounds_keep_their_original_tag_types() {
        use std::sync::atomic::AtomicBool;

        use crate::chunk::{ChunkData, ChunkEntityData};
        use pumpkin_util::math::{position::BlockPos, vector2::Vector2};

        let mut old_furnace = NbtCompound::new();
        old_furnace.put_string("id", "minecraft:furnace".to_string());
        old_furnace.put_int("x", 1);
        old_furnace.put_int("y", 2);
        old_furnace.put_int("z", 3);
        old_furnace.put_short("cooking_time_spent", 4);
        old_furnace.put_short("cooking_total_time", 200);
        old_furnace.put_short("lit_time_remaining", 5);
        old_furnace.put_short("lit_total_time", 1600);

        let mut new_furnace = NbtCompound::new();
        new_furnace.put_string("id", "minecraft:furnace".to_string());
        new_furnace.put_int("x", 2);
        new_furnace.put_int("y", 2);
        new_furnace.put_int("z", 3);
        new_furnace.put_int("cooking_time_spent", 6);
        new_furnace.put_int("cooking_total_time", 200);
        new_furnace.put_int("lit_time_remaining", 7);
        new_furnace.put_int("lit_total_time", 1600);
        new_furnace.put_float("speed_multiplier", 1.0);

        let mut old_stand = NbtCompound::new();
        old_stand.put_string("id", "minecraft:brewing_stand".to_string());
        old_stand.put_int("x", 3);
        old_stand.put_int("y", 2);
        old_stand.put_int("z", 3);
        old_stand.put_short("BrewTime", 8);
        old_stand.put_byte("Fuel", 3);

        let mut new_stand = NbtCompound::new();
        new_stand.put_string("id", "minecraft:brewing_stand".to_string());
        new_stand.put_int("x", 4);
        new_stand.put_int("y", 2);
        new_stand.put_int("z", 3);
        new_stand.put_int("BrewTime", 9);
        new_stand.put_int("Fuel", 3);
        new_stand.put_int("total_brew_time", 400);
        new_stand.put_int("total_fuel", 20);
        new_stand.put_float("speed_multiplier", 1.5);

        let mut old_pot = NbtCompound::new();
        old_pot.put_string("id", "minecraft:decorated_pot".to_string());
        old_pot.put_int("x", 5);
        old_pot.put_int("y", 2);
        old_pot.put_int("z", 3);
        old_pot.put_list(
            "sherds",
            vec![
                NbtTag::String("minecraft:angler_pottery_sherd".into()),
                NbtTag::String("minecraft:brick".into()),
                NbtTag::String("minecraft:brick".into()),
                NbtTag::String("minecraft:brick".into()),
            ],
        );

        let mut back = NbtCompound::new();
        back.put_string("id", "minecraft:angler_pottery_sherd".to_string());
        let mut front = NbtCompound::new();
        front.put_string("id", "minecraft:brick".to_string());
        let mut sherds = NbtCompound::new();
        sherds.put_compound("back", back);
        sherds.put_compound("front", front);
        let mut new_pot = NbtCompound::new();
        new_pot.put_string("id", "minecraft:decorated_pot".to_string());
        new_pot.put_int("x", 6);
        new_pot.put_int("y", 2);
        new_pot.put_int("z", 3);
        new_pot.put_compound("sherds", sherds);

        let mut sign = NbtCompound::new();
        sign.put_string("id", "minecraft:sign".to_string());
        sign.put_int("x", 7);
        sign.put_int("y", 2);
        sign.put_int("z", 3);
        sign.put_byte("is_waxed", 0);

        let chunk = ChunkData::empty(0, 0);
        {
            let mut entities = chunk
                .pending_block_entities
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for entity in [
                old_furnace,
                new_furnace,
                old_stand,
                new_stand,
                old_pot,
                new_pot,
                sign,
            ] {
                let x = entity.get_int("x").expect("x");
                let y = entity.get_int("y").expect("y");
                let z = entity.get_int("z").expect("z");
                entities.insert(BlockPos::new(x, y, z), entity);
            }
        }

        let bytes = ChunkData::to_bytes(&chunk).expect("serialize chunk");
        let loaded = ChunkData::from_bytes(&bytes, Vector2::new(0, 0)).expect("chunk reloads");
        let saved = read_root(&ChunkData::to_bytes(&loaded).expect("serialize again"));

        let old_furnace = find_block_entity(&saved, "minecraft:furnace", 1);
        assert!(matches!(
            old_furnace.get("cooking_time_spent"),
            Some(NbtTag::Short(4))
        ));
        assert!(matches!(
            old_furnace.get("cooking_total_time"),
            Some(NbtTag::Short(200))
        ));
        assert!(matches!(
            old_furnace.get("lit_time_remaining"),
            Some(NbtTag::Short(5))
        ));
        assert!(matches!(
            old_furnace.get("lit_total_time"),
            Some(NbtTag::Short(1600))
        ));
        assert!(!old_furnace.has("speed_multiplier"));

        let new_furnace = find_block_entity(&saved, "minecraft:furnace", 2);
        assert!(matches!(
            new_furnace.get("cooking_time_spent"),
            Some(NbtTag::Int(6))
        ));
        assert!(matches!(
            new_furnace.get("lit_total_time"),
            Some(NbtTag::Int(1600))
        ));
        assert!(
            matches!(new_furnace.get("speed_multiplier"), Some(NbtTag::Float(value)) if *value == 1.0)
        );

        let old_stand = find_block_entity(&saved, "minecraft:brewing_stand", 3);
        assert!(matches!(old_stand.get("BrewTime"), Some(NbtTag::Short(8))));
        assert!(matches!(old_stand.get("Fuel"), Some(NbtTag::Byte(3))));
        assert!(!old_stand.has("total_brew_time"));
        assert!(!old_stand.has("total_fuel"));
        assert!(!old_stand.has("speed_multiplier"));

        let new_stand = find_block_entity(&saved, "minecraft:brewing_stand", 4);
        assert!(matches!(new_stand.get("BrewTime"), Some(NbtTag::Int(9))));
        assert!(matches!(new_stand.get("Fuel"), Some(NbtTag::Int(3))));
        assert!(matches!(
            new_stand.get("total_brew_time"),
            Some(NbtTag::Int(400))
        ));
        assert!(matches!(new_stand.get("total_fuel"), Some(NbtTag::Int(20))));
        assert!(
            matches!(new_stand.get("speed_multiplier"), Some(NbtTag::Float(value)) if *value == 1.5)
        );

        let old_pot = find_block_entity(&saved, "minecraft:decorated_pot", 5);
        let sherds = old_pot.get_list("sherds").expect("4903 sherds list");
        assert_eq!(sherds.len(), 4);
        assert!(
            matches!(&sherds[0], NbtTag::String(name) if name.as_ref() == "minecraft:angler_pottery_sherd")
        );

        let new_pot = find_block_entity(&saved, "minecraft:decorated_pot", 6);
        let sherds = new_pot
            .get_compound("sherds")
            .expect("5023 sherds compound");
        assert!(sherds.get_compound("left").is_none());
        assert_eq!(
            sherds
                .get_compound("back")
                .and_then(|stack| stack.get_string("id")),
            Some("minecraft:angler_pottery_sherd")
        );
        assert_eq!(
            sherds
                .get_compound("front")
                .and_then(|stack| stack.get_string("id")),
            Some("minecraft:brick")
        );

        let sign = find_block_entity(&saved, "minecraft:sign", 7);
        assert!(!sign.has("allow_op_features"));
        assert!(matches!(sign.get("is_waxed"), Some(NbtTag::Byte(0))));

        let mut pig = NbtCompound::new();
        pig.put_string("id", "minecraft:pig".to_string());
        pig.put_byte("Invulnerable", 1);
        let mut timed = NbtCompound::new();
        timed.put_string("id", "minecraft:pig".to_string());
        timed.put_byte("Invulnerable", 0);
        timed.put_int("invulnerable_time", 12);

        let entities = ChunkEntityData {
            x: 4,
            z: 5,
            data: std::sync::Mutex::new(vec![pig, timed]),
            live: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
        };
        let bytes = ChunkEntityData::to_bytes(&entities).expect("serialize entities");
        let loaded =
            ChunkEntityData::from_bytes(&bytes, Vector2::new(4, 5)).expect("entities reload");
        let saved =
            read_root(&ChunkEntityData::to_bytes(&loaded).expect("serialize entities again"));
        assert_eq!(saved.get_int("DataVersion"), Some(5023));
        let list = saved.get_list("Entities").expect("entities");
        assert!(matches!(
            list[0]
                .extract_compound()
                .and_then(|pig| pig.get("Invulnerable")),
            Some(NbtTag::Byte(1))
        ));
        assert!(
            !list[0]
                .extract_compound()
                .expect("pig")
                .has("invulnerable_time")
        );
        assert!(matches!(
            list[1]
                .extract_compound()
                .and_then(|pig| pig.get("invulnerable_time")),
            Some(NbtTag::Int(12))
        ));
    }
}
