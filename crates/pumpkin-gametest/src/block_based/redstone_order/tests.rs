//! Drives the fixture on a minimal pumpkin world.
//!
//! Tick 0 is the trigger placement. Each later tick runs due block ticks, then synced
//! block events, then block-entity ticks. Fluid ticks are left queued.

use std::path::PathBuf;
use std::sync::{Arc, Weak};

use arc_swap::ArcSwap;
use pumpkin_config::lighting::LightingEngineConfig;
use pumpkin_config::world::LevelConfig;
use pumpkin_core::block::entities::BlockEntity;
use pumpkin_core::block::registry::default_registry;
use pumpkin_core::block::{BlockBehaviour, OnScheduledTickArgs};
use pumpkin_core::server::Server;
use pumpkin_core::world::World;
use pumpkin_data::block_properties::{
    Facing, HorizontalFacing, RepeaterLikeProperties, StickyPistonLikeProperties,
};
use pumpkin_data::dimension::Dimension;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::{Block, BlockStateId};
use pumpkin_inventory::Inventory;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_util::world_seed::Seed;
use pumpkin_world::chunk::ChunkData;
use pumpkin_world::level::Level;
use pumpkin_world::world::BlockFlags;
use pumpkin_world::world_info::LevelData;

use super::{RedstoneProbe, RelPos, ScheduledHit, cases, piston, repeater, run_case, states};

const ORIGIN: BlockPos = BlockPos::new(8, 80, 8);

struct PumpkinProbe {
    runtime: tokio::runtime::Runtime,
    world: Arc<World>,
    level: Arc<Level>,
    root: PathBuf,
}

impl PumpkinProbe {
    #[expect(
        clippy::expect_used,
        reason = "the fixture cannot place blocks without a runtime and a temp world folder"
    )]
    fn boot() -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_time()
            .build()
            .expect("redstone fixture runtime");
        let root = std::env::temp_dir().join(format!("pumpkin-a36-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("redstone fixture world folder");
        let folder = root.clone();
        let (world, level) = runtime.block_on(async move { Self::open_world(folder) });
        let probe = Self {
            runtime,
            world,
            level,
            root,
        };
        probe.preload_chunks();
        probe
    }

    fn open_world(root: PathBuf) -> (Arc<World>, Arc<Level>) {
        let config = LevelConfig {
            lighting: LightingEngineConfig::Full,
            ..LevelConfig::default()
        };
        let dimension = Dimension::OVERWORLD.clone();
        let level = Level::from_root_folder(&config, root, 1, dimension.clone());
        let level_info = Arc::new(ArcSwap::new(Arc::new(LevelData::default(Seed(1)))));
        let world = Arc::new(World::load(
            level.clone(),
            level_info,
            dimension,
            default_registry(),
            Weak::<Server>::new(),
        ));
        (world, level)
    }

    fn preload_chunks(&self) {
        for x in 0..=2 {
            for z in 0..=1 {
                self.level
                    .loaded_chunks
                    .insert(Vector2::new(x, z), ChunkData::empty_sync(x, z));
            }
        }
    }
}

const fn absolute(pos: RelPos) -> BlockPos {
    BlockPos::new(ORIGIN.0.x + pos.x, ORIGIN.0.y + pos.y, ORIGIN.0.z + pos.z)
}

impl Drop for PumpkinProbe {
    fn drop(&mut self) {
        self.runtime.block_on(self.level.shutdown());
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn quiet_flags() -> BlockFlags {
    BlockFlags::UPDATE_KNOWN_SHAPE
        | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK
        | BlockFlags::SKIP_DROPS
        | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK
}

impl RedstoneProbe for PumpkinProbe {
    fn reset(&self) {
        self.world.flush_synced_block_events();
        let air = Block::AIR.default_state.id;
        let flags = quiet_flags();
        for x in -4..=20 {
            for y in -2..=4 {
                for z in -4..=4 {
                    let pos = absolute(RelPos::new(x, y, z));
                    if self.world.get_block_state_id(&pos) != air {
                        self.world.set_block_state(&pos, air, flags);
                    }
                }
            }
        }
        let min = absolute(RelPos::new(-4, -2, -4));
        let max = absolute(RelPos::new(21, 5, 5));
        let chunks: Vec<Vector2<i32>> = self
            .level
            .loaded_chunks
            .iter()
            .map(|chunk| *chunk.key())
            .collect();
        for key in chunks {
            let Some(chunk) = self.level.loaded_chunks.get(&key) else {
                continue;
            };
            chunk.block_ticks.clear_area(&min, &max);
            chunk.fluid_ticks.clear_area(&min, &max);
            if !chunk.block_ticks.has_ticks() && !chunk.fluid_ticks.has_ticks() {
                self.level.chunks_with_scheduled_ticks.remove(&key);
            }
        }
        self.world.entities.store(Arc::new(Vec::new()));
    }

    fn place(&self, pos: RelPos, state: BlockStateId, notify: bool) {
        let flags = if notify {
            BlockFlags::NOTIFY_ALL
        } else {
            quiet_flags()
        };
        self.world.set_block_state(&absolute(pos), state, flags);
    }

    fn stock(&self, pos: RelPos, item_name: &str) -> bool {
        let Some(item) = Item::from_registry_key(item_name) else {
            return false;
        };
        let Some(block_entity) = self.world.get_block_entity(&absolute(pos)) else {
            return false;
        };
        let Some(inventory) = block_entity.get_inventory() else {
            return false;
        };
        Inventory::set_stack(inventory.as_ref(), 0, ItemStack::new(1, item));
        true
    }

    fn state_id(&self, pos: RelPos) -> BlockStateId {
        self.world.get_block_state_id(&absolute(pos))
    }

    fn schedule(&self) -> Vec<ScheduledHit> {
        // One chunk keeps insertion order. Chunks are appended in (x, z) order.
        let mut keys: Vec<Vector2<i32>> = self
            .level
            .loaded_chunks
            .iter()
            .map(|chunk| *chunk.key())
            .collect();
        keys.sort_by_key(|key| (key.x, key.y));
        let mut hits = Vec::new();
        for key in keys {
            let Some(chunk) = self.level.loaded_chunks.get(&key) else {
                continue;
            };
            for tick in chunk.block_ticks.to_vec() {
                hits.push(ScheduledHit {
                    delay: tick.delay,
                    pos: RelPos::new(
                        tick.position.0.x - ORIGIN.0.x,
                        tick.position.0.y - ORIGIN.0.y,
                        tick.position.0.z - ORIGIN.0.z,
                    ),
                    block: tick.value.name,
                });
            }
        }
        hits
    }

    fn step(&self) {
        let keys: Vec<Vector2<i32>> = self
            .level
            .chunks_with_scheduled_ticks
            .iter()
            .map(|pos| *pos)
            .collect();
        let mut due = Vec::new();
        for key in keys {
            let Some(chunk) = self.level.loaded_chunks.get(&key) else {
                self.level.chunks_with_scheduled_ticks.remove(&key);
                continue;
            };
            due.extend(chunk.block_ticks.step_tick());
            if !chunk.block_ticks.has_ticks() && !chunk.fluid_ticks.has_ticks() {
                self.level.chunks_with_scheduled_ticks.remove(&key);
            }
        }
        due.sort_unstable();
        for tick in due {
            let pos = tick.position;
            let block = self.world.get_block(&pos);
            if let Some(behaviour) = self.world.block_registry.get_pumpkin_block(block.id) {
                BlockBehaviour::on_scheduled_tick(
                    behaviour.as_ref(),
                    OnScheduledTickArgs {
                        world: &self.world,
                        block,
                        position: &pos,
                    },
                );
            }
        }
        self.world.flush_synced_block_events();
        let block_entities: Vec<Arc<dyn BlockEntity>> = self
            .world
            .block_entities
            .iter()
            .flat_map(|chunk| chunk.values().cloned().collect::<Vec<_>>())
            .collect();
        for entity in block_entities {
            entity.tick(&self.world);
        }
    }

    fn entity_count(&self) -> u32 {
        u32::try_from(self.world.entities.load().len()).unwrap_or(u32::MAX)
    }
}

#[test]
fn builds_dust_repeater_and_piston_states() {
    let dust_id = states::dust();
    assert_eq!(Block::from_state_id(dust_id).name, "redstone_wire");

    let repeater_id = repeater(HorizontalFacing::East, 4);
    let gate = RepeaterLikeProperties::from_state_id(repeater_id);
    assert_eq!(Block::from_state_id(repeater_id).name, "repeater");
    assert_eq!(gate.facing, HorizontalFacing::East);
    assert_eq!(gate.delay, 4);
    assert!(!gate.powered);

    let piston_id = piston(Facing::East);
    let arm = StickyPistonLikeProperties::from_state_id(piston_id);
    assert_eq!(Block::from_state_id(piston_id).name, "piston");
    assert_eq!(arm.facing, Facing::East);
    assert!(!arm.extended);
}

#[test]
fn recorded_layouts_match_or_stay_ignored() {
    let probe = PumpkinProbe::boot();
    let mut seen = std::collections::HashSet::new();
    let mut failures = Vec::new();
    let mut live = 0u32;
    for case in cases() {
        assert!(seen.insert(case.id), "duplicate case id {}", case.id);
        let report = run_case(case, &probe);
        if report.detail.starts_with("harness:") {
            failures.push(format!("{}: {}", case.id, report.detail));
            continue;
        }
        match case.ignored {
            None if report.matched => live += 1,
            None => failures.push(format!("{}: {}", case.id, report.detail)),
            Some(reason) if reason.is_empty() || reason.contains('\n') => {
                failures.push(format!(
                    "{id}: ignored reason must be one non-empty line",
                    id = case.id
                ));
            }
            Some(_) if report.matched => failures.push(format!(
                "{id} matches vanilla; clear its ignored note",
                id = case.id
            )),
            Some(_) => {}
        }
    }
    assert!(live > 0, "at least one redstone layout must match");
    assert!(
        failures.is_empty(),
        "redstone order fixture:\n{}",
        failures.join("\n")
    );
}
