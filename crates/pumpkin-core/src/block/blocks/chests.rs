use std::sync::Arc;

use crate::block::entities::BlockEntity;
use crate::block::entities::chest::ChestBlockEntity;
use pumpkin_data::BlockStateId;
use pumpkin_data::block_properties::{ChestLikeProperties, ChestType, HorizontalFacing};
use pumpkin_data::entity::EntityPose;
use pumpkin_data::loot_table::get_loot_table;
use pumpkin_data::{Block, BlockDirection, translation};
use pumpkin_inventory::Inventory;
use pumpkin_inventory::double::DoubleInventory;
use pumpkin_inventory::generic_container_screen_handler::{create_generic_9x3, create_generic_9x6};
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::screen_handler::{
    InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler,
};
use pumpkin_macros::{pumpkin_block, pumpkin_block_from_tag};
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::text::TextComponent;
use pumpkin_world::world::BlockFlags;
use std::sync::Mutex;

use crate::block::{
    BlockBehaviour, BrokenArgs, EmitsRedstonePowerArgs, GetComparatorOutputArgs,
    GetRedstonePowerArgs, GetScreenHandlerFactoryArgs, NormalUseArgs, OnPlaceArgs,
    OnSyncedBlockEventArgs, PathComputationType, PlacedArgs, RandomTickArgs,
    registry::BlockActionResult,
};
use crate::entity::EntityBase;
use crate::entity::player::Player;
use crate::world::World;
use crate::world::loot::fill_chest_inventory;
use pumpkin_data::BlockState;

struct ChestScreenFactory {
    inventory: Arc<dyn Inventory>,
    loot_pending: bool,
}

impl ScreenHandlerFactory for ChestScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        if self.loot_pending && player.is_spectator() {
            return None;
        }

        let concrete_handler = if self.inventory.size() > 27 {
            create_generic_9x6(sync_id, player_inventory, self.inventory.clone(), player)
        } else {
            create_generic_9x3(sync_id, player_inventory, self.inventory.clone(), player)
        };

        let concrete_arc = Arc::new(Mutex::new(concrete_handler));

        Some(concrete_arc as SharedScreenHandler)
    }

    fn get_display_name(&self) -> TextComponent {
        if self.inventory.size() > 27 {
            pumpkin_macros::translate_cross!(
                translation::java::CONTAINER_CHESTDOUBLE,
                translation::bedrock::CONTAINER_CHESTDOUBLE
            )
        } else {
            pumpkin_macros::translate_cross!(
                translation::java::CONTAINER_CHEST,
                translation::bedrock::CONTAINER_CHEST
            )
        }
    }
}

// Shared chest behavior implementations
const LID_ANIMATION_EVENT_TYPE: u8 = 1;

fn on_place_chest_impl(args: &OnPlaceArgs<'_>) -> BlockStateId {
    let mut chest_props = ChestLikeProperties::default(args.block);
    chest_props.waterlogged = args.replacing.water_source();

    let (r#type, facing) = compute_chest_props(
        args.world,
        args.player,
        args.block,
        args.position,
        args.direction,
    );
    chest_props.facing = facing;
    chest_props.r#type = r#type;

    chest_props.to_state_id(args.block)
}

fn placed_chest_impl<E: BlockEntity + 'static>(
    args: &PlacedArgs<'_>,
    create_entity: impl FnOnce(BlockPos) -> E,
) {
    let chest = create_entity(*args.position);
    args.world.add_block_entity(Arc::new(chest));

    let chest_props = ChestLikeProperties::from_state_id(args.state_id);
    let connected_towards = match chest_props.r#type {
        ChestType::Single => return,
        ChestType::Left => chest_props.facing.rotate_clockwise(),
        ChestType::Right => chest_props.facing.rotate_counter_clockwise(),
    };

    if let Some(mut neighbor_props) = get_chest_properties_if_can_connect(
        args.world,
        args.block,
        args.position,
        chest_props.facing,
        connected_towards,
        ChestType::Single,
    ) {
        neighbor_props.r#type = chest_props.r#type.opposite();

        args.world.set_block_state(
            &args.position.offset(connected_towards.to_offset()),
            neighbor_props.to_state_id(args.block),
            BlockFlags::NOTIFY_LISTENERS,
        );
    }
}

/// Computes the comparator output for a chest, combining both halves for double chests.
fn get_chest_comparator_output(args: &GetComparatorOutputArgs<'_>) -> Option<u8> {
    let state = args.world.get_block_state_id(args.position);
    let first_chest = args.world.get_block_entity(args.position);
    let first_inventory = first_chest.and_then(BlockEntity::get_inventory)?;

    let chest_props = ChestLikeProperties::from_state_id(state);
    let connected_towards = match chest_props.r#type {
        ChestType::Single => None,
        ChestType::Left => Some(chest_props.facing.rotate_clockwise()),
        ChestType::Right => Some(chest_props.facing.rotate_counter_clockwise()),
    };

    // Vanilla passes `ignoreBeingBlocked = false`, so a blocked half reads zero.
    if is_chest_blocked(args.world, args.position) {
        return Some(0);
    }

    if let Some(direction) = connected_towards
        && is_chest_blocked(args.world, &args.position.offset(direction.to_offset()))
    {
        return Some(0);
    }

    if let Some(direction) = connected_towards
        && let Some(second_inventory) = args
            .world
            .get_block_entity(&args.position.offset(direction.to_offset()))
            .and_then(BlockEntity::get_inventory)
    {
        let double_inventory = if matches!(chest_props.r#type, ChestType::Right) {
            DoubleInventory::new(first_inventory, second_inventory)
        } else {
            DoubleInventory::new(second_inventory, first_inventory)
        };
        Some(crate::block::calculate_comparator_output(
            double_inventory.as_ref(),
        ))
    } else {
        Some(crate::block::calculate_comparator_output(
            first_inventory.as_ref(),
        ))
    }
}

/// Returns a screen handler factory for opening a chest or double chest inventory.
///
/// Unpacks deferred loot tables on first open (for non-spectator players) and combines
/// connected inventories for double chests if neither chest half is obstructed.
fn get_chest_screen_handler_factory(
    args: GetScreenHandlerFactoryArgs<'_>,
) -> Option<Box<dyn ScreenHandlerFactory>> {
    let state = args.world.get_block_state_id(args.position);
    let first_chest = args.world.get_block_entity(args.position);

    let player_is_spectator = args.player.gamemode.load() == GameMode::Spectator;

    let chest_props = ChestLikeProperties::from_state_id(state);
    let connected_towards = match chest_props.r#type {
        ChestType::Single => None,
        ChestType::Left => Some(chest_props.facing.rotate_clockwise()),
        ChestType::Right => Some(chest_props.facing.rotate_counter_clockwise()),
    };

    let unpack = |entity: &Arc<dyn BlockEntity>| {
        if let Some((loot_key, seed)) = entity.take_loot_table()
            && let Some(table) = get_loot_table(&loot_key)
            && let Some(inv) = entity.clone().get_inventory()
        {
            fill_chest_inventory(&inv, table, seed);
            inv.mark_dirty();
        }
    };

    // Unpack deferred loot table on first open (non-spectator only).
    if !player_is_spectator && let Some(ref entity) = first_chest {
        unpack(entity);
    }

    let first_loot_pending = first_chest
        .as_ref()
        .is_some_and(|entity| entity.has_loot_table());
    let first_inventory = first_chest.and_then(BlockEntity::get_inventory)?;

    if is_chest_blocked(args.world, args.position) {
        return None;
    }

    if let Some(direction) = connected_towards {
        let neighbor_pos = args.position.offset(direction.to_offset());
        if is_chest_blocked(args.world, &neighbor_pos) {
            return None;
        }
    }

    // Both halves of a double chest are unpacked at once, like vanilla's CompoundContainer.
    if !player_is_spectator
        && let Some(direction) = connected_towards
        && let Some(second) = args
            .world
            .get_block_entity(&args.position.offset(direction.to_offset()))
    {
        unpack(&second);
    }

    let loot_pending = first_loot_pending
        || connected_towards.is_some_and(|direction| {
            args.world
                .get_block_entity(&args.position.offset(direction.to_offset()))
                .is_some_and(|second| second.has_loot_table())
        });

    let inventory = if let Some(direction) = connected_towards
        && let Some(second_inventory) = args
            .world
            .get_block_entity(&args.position.offset(direction.to_offset()))
            .and_then(BlockEntity::get_inventory)
    {
        // Vanilla: chestType == ChestType.RIGHT ? DoubleBlockProperties.Type.FIRST : DoubleBlockProperties.Type.SECOND;
        if matches!(chest_props.r#type, ChestType::Right) {
            DoubleInventory::new(first_inventory, second_inventory)
        } else {
            DoubleInventory::new(second_inventory, first_inventory)
        }
    } else {
        first_inventory
    };

    Some(Box::new(ChestScreenFactory {
        inventory,
        loot_pending,
    }))
}

fn normal_use_chest_impl(args: &NormalUseArgs<'_>) -> BlockActionResult {
    let stat = if args.block.id == Block::TRAPPED_CHEST.id {
        pumpkin_data::statistic::CustomStatistic::TriggerTrappedChest
    } else {
        pumpkin_data::statistic::CustomStatistic::OpenChest
    };
    args.player.increment_stat(
        pumpkin_data::statistic::StatisticCategory::Custom,
        stat as i32,
        1,
    );

    if let Some(factory) = get_chest_screen_handler_factory(GetScreenHandlerFactoryArgs {
        server: args.server,
        world: args.world,
        block: args.block,
        position: args.position,
        player: args.player,
    }) {
        args.player
            .open_handled_screen(factory.as_ref(), Some(*args.position));
    }

    BlockActionResult::Success
}

fn broken_chest_impl(args: &BrokenArgs<'_>) {
    let chest_props = ChestLikeProperties::from_state_id(args.state.id);
    let connected_towards = match chest_props.r#type {
        ChestType::Single => return,
        ChestType::Left => chest_props.facing.rotate_clockwise(),
        ChestType::Right => chest_props.facing.rotate_counter_clockwise(),
    };

    if let Some(mut neighbor_props) = get_chest_properties_if_can_connect(
        args.world,
        args.block,
        args.position,
        chest_props.facing,
        connected_towards,
        chest_props.r#type.opposite(),
    ) {
        neighbor_props.r#type = ChestType::Single;

        args.world.set_block_state(
            &args.position.offset(connected_towards.to_offset()),
            neighbor_props.to_state_id(args.block),
            BlockFlags::NOTIFY_LISTENERS,
        );
    }
}

#[pumpkin_block_from_tag("c:chests/wooden")]
pub struct ChestBlock;

impl BlockBehaviour for ChestBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        on_place_chest_impl(&args)
    }

    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        args.r#type == LID_ANIMATION_EVENT_TYPE
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        placed_chest_impl(&args, ChestBlockEntity::new);
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        normal_use_chest_impl(&args)
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        get_chest_screen_handler_factory(args)
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        broken_chest_impl(&args);
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        get_chest_comparator_output(&args)
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

/// Copper chests have the same behavior as wooden chests but also oxidize over time.
#[pumpkin_block_from_tag("minecraft:copper_chests")]
pub struct CopperChestBlock;

impl
    crate::block::blocks::weathering_copper::ChangeOverTimeBlock<
        crate::block::blocks::weathering_copper::WeatherState,
    > for CopperChestBlock
{
    fn get_age(
        &self,
        block: &Block,
    ) -> Option<crate::block::blocks::weathering_copper::WeatherState> {
        crate::block::blocks::weathering_copper::get_weather_state(block)
    }

    fn get_chance_modifier(
        &self,
        age: crate::block::blocks::weathering_copper::WeatherState,
    ) -> f32 {
        crate::block::blocks::weathering_copper::get_chance_modifier(age)
    }

    fn get_next(&self, block: &Block) -> Option<&'static Block> {
        crate::block::blocks::weathering_copper::get_next(block)
    }

    fn get_previous(&self, block: &Block) -> Option<&'static Block> {
        crate::block::blocks::weathering_copper::get_previous(block)
    }

    fn get_first(&self, block: &Block) -> Option<&'static Block> {
        crate::block::blocks::weathering_copper::get_first(block)
    }
}

impl crate::block::blocks::weathering_copper::WeatheringCopper for CopperChestBlock {}

impl BlockBehaviour for CopperChestBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        on_place_chest_impl(&args)
    }

    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        args.r#type == LID_ANIMATION_EVENT_TYPE
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        placed_chest_impl(&args, ChestBlockEntity::new);
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        normal_use_chest_impl(&args)
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        get_chest_screen_handler_factory(args)
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        broken_chest_impl(&args);
    }

    fn random_tick(&self, args: RandomTickArgs<'_>) {
        let current_state_id = args.world.get_block_state_id(args.position);
        let chest_props = ChestLikeProperties::from_state_id(current_state_id);

        // Only oxidize LEFT or SINGLE chests (not RIGHT) to prevent double oxidation
        if chest_props.r#type == ChestType::Right {
            return;
        }

        // Only oxidize if no players are viewing the chest
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(chest_entity) = block_entity.as_any().downcast_ref::<ChestBlockEntity>()
            && chest_entity.get_viewer_count() > 0
        {
            return;
        }

        crate::block::blocks::weathering_copper::change_over_time(
            args.world,
            args.position,
            args.block,
        );
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        get_chest_comparator_output(&args)
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

/// Trapped chests have the same behavior as wooden chests but also emit redstone power based on viewer count.
#[pumpkin_block("minecraft:trapped_chest")]
pub struct TrappedChestBlock;

impl BlockBehaviour for TrappedChestBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        on_place_chest_impl(&args)
    }

    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        args.r#type == LID_ANIMATION_EVENT_TYPE
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        use crate::block::entities::trapped_chest::TrappedChestBlockEntity;
        placed_chest_impl(&args, TrappedChestBlockEntity::new);
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        normal_use_chest_impl(&args)
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        get_chest_screen_handler_factory(args)
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        broken_chest_impl(&args);
    }

    fn emits_redstone_power(&self, _args: EmitsRedstonePowerArgs<'_>) -> bool {
        true
    }

    fn get_weak_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        use crate::block::entities::trapped_chest::TrappedChestBlockEntity;

        // Get viewer count from this chest
        let viewer_count = if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(trapped_chest) = block_entity
                .as_any()
                .downcast_ref::<TrappedChestBlockEntity>()
        {
            trapped_chest.get_viewer_count()
        } else {
            0
        };

        viewer_count.min(15) as u8
    }

    fn get_strong_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        // Strong power emitted to the block beneath the trapped chest
        // The block below queries with direction Up (from below looking up at the chest)
        if args.direction == BlockDirection::Up {
            self.get_weak_redstone_power(args)
        } else {
            0
        }
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        get_chest_comparator_output(&args)
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

fn compute_chest_props(
    world: &World,
    player: &Player,
    block: &Block,
    block_pos: &BlockPos,
    face: BlockDirection,
) -> (ChestType, HorizontalFacing) {
    let player_facing = player.get_entity().get_horizontal_facing();
    let chest_facing = player_facing.opposite();

    if player.get_entity().pose.load() == EntityPose::Crouching {
        let Some(face) = face.to_horizontal_facing() else {
            return (ChestType::Single, chest_facing);
        };

        let (clicked_block, clicked_block_state) =
            world.get_block_and_state_id(&block_pos.offset(face.to_offset()));

        if clicked_block == block {
            let clicked_props = ChestLikeProperties::from_state_id(clicked_block_state);

            if clicked_props.r#type != ChestType::Single {
                return (ChestType::Single, chest_facing);
            }

            if clicked_props.facing.rotate_clockwise() == face {
                return (ChestType::Left, clicked_props.facing);
            } else if clicked_props.facing.rotate_counter_clockwise() == face {
                return (ChestType::Right, clicked_props.facing);
            }
        }

        return (ChestType::Single, chest_facing);
    }

    if get_chest_properties_if_can_connect(
        world,
        block,
        block_pos,
        chest_facing,
        chest_facing.rotate_clockwise(),
        ChestType::Single,
    )
    .is_some()
    {
        (ChestType::Left, chest_facing)
    } else if get_chest_properties_if_can_connect(
        world,
        block,
        block_pos,
        chest_facing,
        chest_facing.rotate_counter_clockwise(),
        ChestType::Single,
    )
    .is_some()
    {
        (ChestType::Right, chest_facing)
    } else {
        (ChestType::Single, chest_facing)
    }
}

fn get_chest_properties_if_can_connect(
    world: &World,
    block: &Block,
    block_pos: &BlockPos,
    facing: HorizontalFacing,
    direction: HorizontalFacing,
    wanted_type: ChestType,
) -> Option<ChestLikeProperties> {
    let (neighbor_block, neighbor_block_state) =
        world.get_block_and_state_id(&block_pos.offset(direction.to_offset()));

    if neighbor_block != block {
        return None;
    }

    let neighbor_props = ChestLikeProperties::from_state_id(neighbor_block_state);
    if neighbor_props.facing == facing && neighbor_props.r#type == wanted_type {
        return Some(neighbor_props);
    }

    None
}

fn is_chest_blocked(world: &World, block_pos: &BlockPos) -> bool {
    // TODO: Block opening when a cat is sitting on top.
    has_block_on_top(world, block_pos)
}
fn has_block_on_top(world: &World, block_pos: &BlockPos) -> bool {
    let above_pos = block_pos.up();
    let above_state = world.get_block_state(&above_pos);
    above_state.is_solid_block()
}

trait ChestTypeExt {
    fn opposite(&self) -> ChestType;
}

impl ChestTypeExt for ChestType {
    fn opposite(&self) -> Self {
        match self {
            Self::Single => Self::Single,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, Weak};

    use arc_swap::ArcSwap;
    use pumpkin_config::lighting::LightingEngineConfig;
    use pumpkin_config::world::LevelConfig;
    use pumpkin_data::dimension::Dimension;
    use pumpkin_data::item::Item;
    use pumpkin_data::item_stack::ItemStack;
    use pumpkin_data::{Block, BlockStateId};
    use pumpkin_inventory::Inventory;
    use pumpkin_inventory::double::DoubleInventory;
    use pumpkin_util::math::position::BlockPos;
    use pumpkin_util::math::vector2::Vector2;
    use pumpkin_util::world_seed::Seed;
    use pumpkin_world::chunk::ChunkData;
    use pumpkin_world::level::Level;
    use pumpkin_world::world::BlockFlags;
    use pumpkin_world::world_info::LevelData;

    use crate::block::entities::BlockEntity;
    use crate::block::registry::default_registry;
    use crate::server::Server;
    use crate::world::World;
    use crate::world::scoreboard::Scoreboard;

    static NEXT_PROBE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    const CHEST: BlockPos = BlockPos::new(8, 80, 8);
    const CHEST_OTHER_HALF: BlockPos = BlockPos::new(9, 80, 8);
    const BARREL: BlockPos = BlockPos::new(8, 81, 8);

    struct Probe {
        runtime: tokio::runtime::Runtime,
        world: Arc<World>,
        level: Arc<Level>,
        root: PathBuf,
    }

    impl Probe {
        fn boot() -> Self {
            Self::boot_with_autosave(6000)
        }

        #[expect(
            clippy::expect_used,
            reason = "the container check needs a runtime and a temporary world folder"
        )]
        fn boot_with_autosave(autosave_ticks: u64) -> Self {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_time()
                .build()
                .expect("container persist runtime");
            let n = NEXT_PROBE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("pumpkin-a20-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&root).expect("container persist world folder");
            let folder = root.clone();
            let (world, level) =
                runtime.block_on(async move { open_world(folder, autosave_ticks) });
            let probe = Self {
                runtime,
                world,
                level,
                root,
            };
            probe
                .level
                .loaded_chunks
                .insert(Vector2::new(0, 0), ChunkData::empty_sync(0, 0));
            probe
        }

        fn place(&self, pos: BlockPos, state: BlockStateId) {
            self.world.set_block_state(
                &pos,
                state,
                BlockFlags::UPDATE_KNOWN_SHAPE | BlockFlags::SKIP_DROPS,
            );
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            self.runtime.block_on(self.level.shutdown());
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn open_world(root: PathBuf, autosave_ticks: u64) -> (Arc<World>, Arc<Level>) {
        let config = LevelConfig {
            lighting: LightingEngineConfig::Full,
            autosave_ticks,
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
            Arc::new(Mutex::new(Scoreboard::default())),
            Weak::<Server>::new(),
        ));
        (world, level)
    }

    fn put_stack(world: &Arc<World>, pos: BlockPos, slot: usize, stack: ItemStack) -> bool {
        let Some(inventory) = world
            .get_block_entity(&pos)
            .and_then(BlockEntity::get_inventory)
        else {
            return false;
        };
        inventory.on_open();
        inventory.set_stack(slot, stack);
        inventory.on_close();
        true
    }

    fn read_slot(world: &Arc<World>, pos: BlockPos, slot: usize) -> Option<(u16, u8)> {
        let inventory = world
            .get_block_entity(&pos)
            .and_then(BlockEntity::get_inventory)?;
        let stack = inventory.get_stack(slot);
        Some((stack.get_item().id, stack.item_count))
    }

    fn fill_containers(probe: &Probe) {
        probe.place(CHEST, Block::CHEST.default_state.id);
        probe.place(CHEST_OTHER_HALF, Block::CHEST.default_state.id);
        probe.place(BARREL, Block::BARREL.default_state.id);

        let right = probe
            .world
            .get_block_entity(&CHEST)
            .and_then(BlockEntity::get_inventory);
        let left = probe
            .world
            .get_block_entity(&CHEST_OTHER_HALF)
            .and_then(BlockEntity::get_inventory);
        assert!(right.is_some(), "right chest was not created");
        assert!(left.is_some(), "left chest was not created");
        let (Some(right), Some(left)) = (right, left) else {
            return;
        };
        // The chest window holds these same inventories.
        let double = DoubleInventory::new(right, left);
        double.on_open();
        double.set_stack(0, ItemStack::new(4, &Item::DIAMOND));
        double.set_stack(27, ItemStack::new(2, &Item::EMERALD));
        double.on_close();

        assert!(
            put_stack(
                &probe.world,
                BARREL,
                0,
                ItemStack::new(7, &Item::OAK_PLANKS),
            ),
            "barrel should accept a stack"
        );
    }

    fn assert_saved_stacks(world: &Arc<World>) {
        assert_eq!(
            read_slot(world, CHEST, 0),
            Some((Item::DIAMOND.id, 4)),
            "the chest stack should be in the chunk snapshot"
        );
        assert_eq!(
            read_slot(world, CHEST_OTHER_HALF, 0),
            Some((Item::EMERALD.id, 2)),
            "the other half should be in the chunk snapshot"
        );
        assert_eq!(
            read_slot(world, BARREL, 0),
            Some((Item::OAK_PLANKS.id, 7)),
            "the barrel stack should be in the chunk snapshot"
        );
    }

    /// Chunk unload saves live block entities before it drops them.
    #[test]
    fn chunk_unload_keeps_chest_and_barrel_stacks() {
        let probe = Probe::boot();
        fill_containers(&probe);
        probe.runtime.block_on(async {
            probe
                .world
                .remove_entities_in_chunks([Vector2::new(0, 0)])
                .await;
        });
        assert_saved_stacks(&probe.world);
    }

    /// `/save-all` and shutdown both run `World::save`, which copies live block
    /// entities into the chunk before the chunk saver runs.
    #[test]
    fn world_save_keeps_chest_and_barrel_stacks() {
        let probe = Probe::boot();
        fill_containers(&probe);
        probe.runtime.block_on(probe.world.save());
        probe.world.block_entities.clear();
        assert_saved_stacks(&probe.world);
    }

    /// Periodic autosave used to write the chunk without copying live inventories,
    /// so a crash afterwards kept the empty chest from placement.
    #[test]
    fn autosave_keeps_chest_and_barrel_stacks() {
        let probe = Probe::boot_with_autosave(1);
        fill_containers(&probe);
        probe.world.tick_environment();
        probe.world.block_entities.clear();
        assert_saved_stacks(&probe.world);
    }
}
