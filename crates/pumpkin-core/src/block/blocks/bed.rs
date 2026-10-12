use std::sync::Arc;

use crate::block::blocks::bed_spawn::{
    BedSleepOutcome, BedSleepQuery, BedSleepRefusal, bed_half_in_reach, decide_bed_sleep,
    entity_category_prevents_bed_rest, monster_rest_box,
};
use crate::block::entities::bed::BedBlockEntity;
use pumpkin_data::block_properties::BedPart;
use pumpkin_data::translation;
use pumpkin_data::{Block, BlockState, BlockStateId};
use pumpkin_macros::pumpkin_block_from_tag;
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

use crate::block::OnLandedUponArgs;
use crate::block::UpdateEntityMovementAfterFallOnArgs;
use crate::block::bounce_entity_after_fall;
use crate::block::registry::BlockActionResult;
use crate::block::{
    BlockBehaviour, BrokenArgs, CanPlaceAtArgs, NormalUseArgs, OnPlaceArgs, OnStateReplacedArgs,
    PathComputationType, PlacedArgs,
};
use crate::entity::{EntityBase, player::Player};
use crate::world::World;

type BedProperties = pumpkin_data::block_properties::WhiteBedLikeProperties;

#[pumpkin_block_from_tag("minecraft:beds")]
pub struct BedBlock;

impl BlockBehaviour for BedBlock {
    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        if let Some(player) = args.player {
            let facing = player.get_entity().get_horizontal_facing();
            return args
                .block_accessor
                .get_block_state(args.position)
                .replaceable()
                && args
                    .block_accessor
                    .get_block_state(&args.position.offset(facing.to_offset()))
                    .replaceable();
        }
        false
    }

    fn on_landed_upon(&self, args: OnLandedUponArgs<'_>) {
        if let Some(living) = args.entity.get_living_entity() {
            living.handle_fall_damage(args.entity, args.fall_distance * 0.5, 1.0);
        }
    }

    fn update_entity_movement_after_fall_on(&self, args: UpdateEntityMovementAfterFallOnArgs<'_>) {
        bounce_entity_after_fall(args.entity, 0.66);
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut bed_props = BedProperties::default(args.block);

        bed_props.facing = args.player.get_entity().get_horizontal_facing();
        bed_props.part = BedPart::Foot;

        bed_props.to_state_id(args.block)
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        {
            let bed_entity = BedBlockEntity::new(*args.position);
            args.world.add_block_entity(Arc::new(bed_entity));

            let mut bed_head_props = BedProperties::default(args.block);
            bed_head_props.facing = BedProperties::from_state_id(args.state_id).facing;
            bed_head_props.part = BedPart::Head;

            let bed_head_pos = args.position.offset(bed_head_props.facing.to_offset());
            args.world.set_block_state(
                &bed_head_pos,
                bed_head_props.to_state_id(args.block),
                BlockFlags::NOTIFY_ALL | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK,
            );

            let bed_head_entity = BedBlockEntity::new(bed_head_pos);
            args.world.add_block_entity(Arc::new(bed_head_entity));
        }
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        let bed_props = BedProperties::from_state_id(args.state.id);
        let other_half_pos = if bed_props.part == BedPart::Head {
            args.position
                .offset(bed_props.facing.opposite().to_offset())
        } else {
            args.position.offset(bed_props.facing.to_offset())
        };
        let neighbor_state_id = args.world.get_block_state_id(&other_half_pos);
        if neighbor_state_id.to_block_id() != args.block.id {
            args.world.update_neighbors(&other_half_pos, None);
            return;
        }

        let is_creative = args.player.gamemode.load() == GameMode::Creative;
        let flags = if bed_props.part == BedPart::Foot && !is_creative {
            // Breaking foot in survival -> allow head to drop
            BlockFlags::NOTIFY_ALL
        } else {
            // Breaking head OR creative mode -> skip drops
            BlockFlags::SKIP_DROPS | BlockFlags::NOTIFY_ALL
        };

        args.world
            .break_block(&other_half_pos, Some(args.player), flags);
    }

    fn on_state_replaced(&self, args: OnStateReplacedArgs<'_>) {
        if args.moved {
            return;
        }

        let bed_props = BedProperties::from_state_id(args.old_state_id);
        let other_half_pos = if bed_props.part == BedPart::Head {
            args.position
                .offset(bed_props.facing.opposite().to_offset())
        } else {
            args.position.offset(bed_props.facing.to_offset())
        };

        let (other_block, other_state) = args.world.get_block_and_state(&other_half_pos);
        if other_block == args.block {
            let other_props = BedProperties::from_state_id(other_state.id);
            if other_props.part != bed_props.part {
                args.world.break_block(
                    &other_half_pos,
                    None,
                    BlockFlags::SKIP_DROPS | BlockFlags::NOTIFY_ALL,
                );
            }
        }
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        Self::use_bed(args.world, args.player, args.block, args.position)
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

impl BedBlock {
    fn use_bed(
        world: &Arc<World>,
        player: &Arc<Player>,
        block: &Block,
        position: &BlockPos,
    ) -> BlockActionResult {
        let state_id = world.get_block_state_id(position);
        let bed_props = BedProperties::from_state_id(state_id);

        let (bed_head_pos, bed_foot_pos) = if bed_props.part == BedPart::Head {
            (
                *position,
                position.offset(bed_props.facing.opposite().to_offset()),
            )
        } else {
            (position.offset(bed_props.facing.to_offset()), *position)
        };

        // Explode if bed rule explodes (EnvironmentAttributes.BED_RULE)
        if world.dimension.bed_rule.explodes {
            world.break_block(&bed_head_pos, None, BlockFlags::SKIP_DROPS);
            world.break_block(&bed_foot_pos, None, BlockFlags::SKIP_DROPS);

            world.explode(
                bed_head_pos.to_centered_f64(),
                5.0,
                crate::world::ExplosionInteraction::Block,
            );

            return BlockActionResult::SuccessServer;
        }

        let is_dark = world.is_dark_outside();
        let player_pos = player.position();
        let mut query = BedSleepQuery {
            can_sleep: world.dimension.bed_rule.can_sleep(is_dark),
            can_set_spawn: world.dimension.bed_rule.can_set_spawn(is_dark),
            obstructed: world.get_block_state(&bed_head_pos.up()).is_solid()
                || world.get_block_state(&bed_foot_pos.up()).is_solid(),
            occupied: bed_props.occupied,
            in_reach: bed_half_in_reach(player_pos, bed_head_pos)
                || bed_half_in_reach(player_pos, bed_foot_pos),
            monster_nearby: false,
        };
        let mut outcome = decide_bed_sleep(query);
        // Creative skips the monster box, same as the dedicated server.
        if matches!(outcome, BedSleepOutcome::Sleep { .. })
            && player.gamemode.load() != GameMode::Creative
            && monster_blocks_bed_rest(world, bed_head_pos)
        {
            query.monster_nearby = true;
            outcome = decide_bed_sleep(query);
        }

        let set_spawn = match outcome {
            BedSleepOutcome::Sleep { set_spawn } => set_spawn,
            BedSleepOutcome::Refuse(refusal) => {
                send_bed_refusal(player, refusal);
                return BlockActionResult::SuccessServer;
            }
        };

        if let Some(server) = world.server.upgrade() {
            let mut event =
                crate::plugin::api::events::player::player_bed::PlayerBedEnterEvent::new(
                    player.clone(),
                    bed_head_pos,
                );
            server.plugin_manager.fire_blocking(&server, &mut event);
            if event.cancelled {
                return BlockActionResult::SuccessServer;
            }
        }

        // The spawn moves only once sleep is actually starting.
        if set_spawn
            && player.set_respawn_point(
                world.dimension.clone(),
                bed_head_pos,
                player.get_entity().yaw.load(),
                player.get_entity().pitch.load(),
                false,
            )
        {
            player.send_system_message(&pumpkin_macros::translate_cross!(
                translation::java::BLOCK_MINECRAFT_SET_SPAWN,
                translation::bedrock::TILE_BED_RESPAWNSET
            ));
        }

        player.sleep(bed_head_pos);
        if set_spawn {
            player.trigger_advancement(
                crate::entity::player::advancement::trigger::AdvancementTrigger::SleptInBed,
            );
        }
        player.increment_stat(
            pumpkin_data::statistic::StatisticCategory::Custom,
            pumpkin_data::statistic::CustomStatistic::SleepInBed as i32,
            1,
        );
        Self::set_occupied(true, world, block, position, state_id);

        BlockActionResult::SuccessServer
    }
}

impl BedBlock {
    pub fn set_occupied(
        occupied: bool,
        world: &Arc<World>,
        block: &Block,
        block_pos: &BlockPos,
        state_id: BlockStateId,
    ) {
        let mut bed_props = BedProperties::from_state_id(state_id);
        bed_props.occupied = occupied;
        world.set_block_state(
            block_pos,
            bed_props.to_state_id(block),
            BlockFlags::NOTIFY_LISTENERS,
        );

        let other_half_pos = if bed_props.part == BedPart::Head {
            block_pos.offset(bed_props.facing.opposite().to_offset())
        } else {
            block_pos.offset(bed_props.facing.to_offset())
        };
        bed_props.part = if bed_props.part == BedPart::Head {
            BedPart::Foot
        } else {
            BedPart::Head
        };
        world.set_block_state(
            &other_half_pos,
            bed_props.to_state_id(block),
            BlockFlags::NOTIFY_LISTENERS,
        );
    }
}

fn monster_blocks_bed_rest(world: &World, bed_head: BlockPos) -> bool {
    world
        .get_entities_at_box(&monster_rest_box(bed_head))
        .iter()
        .any(|entity| entity_category_prevents_bed_rest(entity.get_entity().entity_type.category))
}

fn send_bed_refusal(player: &Player, refusal: BedSleepRefusal) {
    let message = match refusal {
        BedSleepRefusal::NotPossibleHere | BedSleepRefusal::NotPossibleNow => {
            pumpkin_macros::translate_cross!(
                translation::java::BLOCK_MINECRAFT_BED_NO_SLEEP,
                translation::bedrock::TILE_BED_NOSLEEP
            )
        }
        BedSleepRefusal::Obstructed => pumpkin_macros::translate_cross!(
            translation::java::BLOCK_MINECRAFT_BED_OBSTRUCTED,
            translation::bedrock::TILE_BED_OBSTRUCTED
        ),
        BedSleepRefusal::Occupied => {
            // TODO: Wake up villager
            pumpkin_macros::translate_cross!(
                translation::java::BLOCK_MINECRAFT_BED_OCCUPIED,
                translation::bedrock::TILE_BED_OCCUPIED
            )
        }
        BedSleepRefusal::TooFar => pumpkin_macros::translate_cross!(
            translation::java::BLOCK_MINECRAFT_BED_TOO_FAR_AWAY,
            translation::bedrock::TILE_BED_TOOFAR
        ),
        BedSleepRefusal::NotSafe => pumpkin_macros::translate_cross!(
            translation::java::BLOCK_MINECRAFT_BED_NOT_SAFE,
            translation::bedrock::TILE_BED_NOTSAFE
        ),
    };
    player.send_system_message_raw(&message, true);
}
