use std::sync::Arc;

use crate::block::entities::block_entity_from_nbt;
use crate::block::registry::BlockActionResult;
use crate::entity::player::Player;
use crate::item::{ItemBehaviour, ItemMetadata};
use crate::server::Server;
use crate::world::World;
use pumpkin_data::block_properties::{ChestLikeProperties, ChestType, DoubleBlockHalf};
use pumpkin_data::block_transformer::{AXE, TransformType};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::SoundCategory;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, BlockStateId, tag};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;

pub struct AxeItem;

impl ItemMetadata for AxeItem {
    fn ids() -> Box<[u16]> {
        tag::Item::MINECRAFT_AXES.1.into()
    }
}

impl ItemBehaviour for AxeItem {
    fn use_on_block(
        &self,
        item: &mut ItemStack,
        player: &Player,
        location: BlockPos,
        face: BlockDirection,
        _cursor_pos: Vector3<f32>,
        block: &Block,
        _server: &Server,
    ) -> BlockActionResult {
        // Adventure mode clears mayBuild. Vanilla refuses the item use unless the
        // stack has a can_place_on predicate, which Pumpkin does not evaluate yet.
        if !player.may_build() {
            return BlockActionResult::Pass;
        }

        let world = player.world();
        let get_block = |dx: i8, dy: i8, dz: i8| {
            let check_pos = BlockPos(location.0 + Vector3::new(dx as i32, dy as i32, dz as i32));
            world.get_block(&check_pos)
        };

        let current_state_id = world.get_block_state_id(&location);
        let Some(result) = AXE.transform(block, current_state_id, face, &get_block) else {
            return BlockActionResult::Pass;
        };

        if block.has_tag(&tag::Block::MINECRAFT_DOORS) {
            // Door placement writes a second half above the clicked block. Skip that
            // callback and write both halves before neighbour updates, or the old half
            // breaks the pair while the two ids differ.
            apply_door(
                &world,
                location,
                block,
                current_state_id,
                result.new_state_id,
                result.target_block,
            );
        } else {
            set_transformed_state(&world, location, current_state_id, result.new_state_id);
            if result.entry.transform_type == Some(TransformType::CopperChest)
                && let Some(neighbor_pos) = copper_chest_neighbor(location, current_state_id)
            {
                let (neighbor_block, neighbor_state_id) =
                    world.get_block_and_state_id(&neighbor_pos);
                if neighbor_block.has_tag(&tag::Block::MINECRAFT_COPPER_CHESTS) {
                    let neighbor_new = copy_block_properties(
                        neighbor_block,
                        neighbor_state_id,
                        result.target_block,
                    );
                    set_transformed_state(&world, neighbor_pos, neighbor_state_id, neighbor_new);
                    if let Some(particle) = result.entry.particle {
                        world.sync_world_event(particle, neighbor_pos, 0);
                    }
                }
            }
        }

        if let Some(sound) = result.entry.sound {
            world.play_sound(sound, SoundCategory::Blocks, &location.to_f64());
        }
        if let Some(particle) = result.entry.particle {
            world.sync_world_event(particle, location, 0);
        }

        if player.gamemode.load() != GameMode::Creative {
            let _ = item.damage_item(i32::from(result.entry.item_damage_per_use));
        }
        BlockActionResult::Success
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn apply_door(
    world: &Arc<World>,
    location: BlockPos,
    block: &Block,
    current_state_id: BlockStateId,
    new_state_id: BlockStateId,
    target_block: &Block,
) {
    use pumpkin_data::block_properties::OakDoorLikeProperties;

    let door_props = OakDoorLikeProperties::from_state_id(current_state_id);
    let other_half_pos = match door_props.half {
        DoubleBlockHalf::Lower => location.up(),
        DoubleBlockHalf::Upper => location.down(),
    };
    let (other_block, other_state_id) = world.get_block_and_state_id(&other_half_pos);
    if other_block == block {
        let other_new_state_id = copy_block_properties(other_block, other_state_id, target_block);
        let quiet = BlockFlags::NOTIFY_LISTENERS
            | BlockFlags::UPDATE_KNOWN_SHAPE
            | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK;
        world.set_block_state(&other_half_pos, other_new_state_id, quiet);
    }

    let notified = BlockFlags::NOTIFY_ALL
        | BlockFlags::UPDATE_KNOWN_SHAPE
        | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK;
    world.set_block_state(&location, new_state_id, notified);
}

fn set_transformed_state(
    world: &Arc<World>,
    position: BlockPos,
    old_state_id: BlockStateId,
    new_state_id: BlockStateId,
) {
    let saved = if keeps_block_entity(old_state_id, new_state_id) {
        world.get_block_entity(&position).map(|entity| {
            let mut nbt = NbtCompound::new();
            entity.write_internal(&mut nbt);
            nbt
        })
    } else {
        None
    };

    let mut flags = BlockFlags::NOTIFY_ALL;
    if saved.is_some() {
        // Same block-entity type (copper chest, copper golem statue). Vanilla keeps
        // the entity; a plain replacement drops its contents and spawns an empty one.
        flags |= BlockFlags::UPDATE_KNOWN_SHAPE
            | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK
            | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK;
    }
    world.set_block_state(&position, new_state_id, flags);

    if let Some(nbt) = saved
        && let Some(entity) = block_entity_from_nbt(&nbt)
    {
        world.add_block_entity(entity);
    }
}

const fn keeps_block_entity(old_state_id: BlockStateId, new_state_id: BlockStateId) -> bool {
    let old_type = old_state_id.to_state().block_entity_type;
    old_type != u16::MAX && old_type == new_state_id.to_state().block_entity_type
}

fn copy_block_properties(
    source_block: &Block,
    source_state_id: BlockStateId,
    target_block: &Block,
) -> BlockStateId {
    if target_block.states.len() <= 1 {
        return target_block.default_state.id;
    }
    let Some(source_props) = source_block.properties(source_state_id) else {
        return target_block.default_state.id;
    };
    let props = source_props.to_props();
    target_block
        .from_properties(&props)
        .to_state_id(target_block)
}

fn copper_chest_neighbor(location: BlockPos, state_id: BlockStateId) -> Option<BlockPos> {
    let props = ChestLikeProperties::from_state_id(state_id);
    let toward = match props.r#type {
        ChestType::Single => return None,
        ChestType::Left => props.r#facing.rotate_clockwise(),
        ChestType::Right => props.r#facing.rotate_counter_clockwise(),
    };
    Some(location.offset(toward.to_offset()))
}
