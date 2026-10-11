//! Block states the fixture places. Property values are the ones a layout asks for,
//! not a copy of a generated table.

use pumpkin_data::block_properties::{
    ComparatorLikeProperties, DispenserLikeProperties, Facing, HorizontalFacing, ModeComparator,
    ObserverLikeProperties, RedstoneWireLikeProperties, RepeaterLikeProperties,
    StickyPistonLikeProperties,
};
use pumpkin_data::{Block, BlockStateId};

#[must_use]
pub fn stone() -> BlockStateId {
    Block::STONE.default_state.id
}

#[must_use]
pub fn bedrock() -> BlockStateId {
    Block::BEDROCK.default_state.id
}

#[must_use]
pub fn redstone_block() -> BlockStateId {
    Block::REDSTONE_BLOCK.default_state.id
}

#[must_use]
pub fn dust() -> BlockStateId {
    RedstoneWireLikeProperties::default(&Block::REDSTONE_WIRE).to_state_id(&Block::REDSTONE_WIRE)
}

/// `facing` is the side the repeater reads. Power leaves from the opposite side.
/// `delay` is the repeater's 1 to 4 setting.
#[must_use]
pub fn repeater(facing: HorizontalFacing, delay: u8) -> BlockStateId {
    let mut props = RepeaterLikeProperties::default(&Block::REPEATER);
    props.facing = facing;
    props.delay = delay;
    props.locked = false;
    props.powered = false;
    props.to_state_id(&Block::REPEATER)
}

/// `facing` is the side the comparator reads. Power leaves from the opposite side.
#[must_use]
pub fn comparator(facing: HorizontalFacing, mode: ModeComparator) -> BlockStateId {
    let mut props = ComparatorLikeProperties::default(&Block::COMPARATOR);
    props.facing = facing;
    props.mode = mode;
    props.powered = false;
    props.to_state_id(&Block::COMPARATOR)
}

/// `facing` is the direction the piston pushes.
#[must_use]
pub fn piston(facing: Facing) -> BlockStateId {
    let mut props = StickyPistonLikeProperties::default(&Block::PISTON);
    props.facing = facing;
    props.extended = false;
    props.to_state_id(&Block::PISTON)
}

/// `facing` is the side the observer watches.
#[must_use]
pub fn observer(facing: Facing) -> BlockStateId {
    let mut props = ObserverLikeProperties::default(&Block::OBSERVER);
    props.facing = facing;
    props.powered = false;
    props.to_state_id(&Block::OBSERVER)
}

/// `facing` is the direction the dispenser shoots.
#[must_use]
pub fn dispenser(facing: Facing) -> BlockStateId {
    let mut props = DispenserLikeProperties::default(&Block::DISPENSER);
    props.facing = facing;
    props.triggered = false;
    props.to_state_id(&Block::DISPENSER)
}
