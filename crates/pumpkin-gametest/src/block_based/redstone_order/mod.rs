//! Redstone order fixture.
//!
//! Cases live in [`cases`]. A later task adds a row there, or clears `ignored` once
//! the layout matches. [`run_case`] places the layout and compares it with the recorded
//! result. It does not change how redstone is evaluated.
//!
//! Tick 0 is the trigger placement, after neighbour updates and before any game tick.
//! Each later tick runs due block ticks, then synced block events, then block-entity
//! ticks. That is the order a frozen 26.3 tick uses for these layouts.

mod cases;
mod states;

pub use cases::{case, cases};
pub use states::{
    bedrock, comparator, dispenser, dust, observer, piston, redstone_block, repeater, stone,
};

use pumpkin_data::block_properties::{
    ComparatorLikeProperties, ObserverLikeProperties, RedstoneWireLikeProperties,
    RepeaterLikeProperties, StickyPistonLikeProperties,
};
use pumpkin_data::{Block, BlockStateId};
use std::collections::HashSet;

/// Which later task owns the layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedstoneLane {
    Dust,
    Repeater,
    Comparator,
    Piston,
    Observer,
    Dispenser,
}

/// A block position relative to the layout origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl RelPos {
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

/// One observable fact about a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// Redstone dust power, from 0 to 15.
    Dust(u8),
    /// Repeater, comparator, or observer `powered` property.
    Powered(bool),
    /// Piston `extended` property.
    Extended(bool),
    /// Block name without a namespace, such as `stone` or `water`.
    Block(&'static str),
}

/// A block state read after `tick` game ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub tick: u32,
    pub pos: RelPos,
    pub signal: Signal,
}

/// A block tick waiting after the trigger placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledHit {
    pub delay: u8,
    pub pos: RelPos,
    pub block: &'static str,
}

/// What a passing run records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expectation {
    pub samples: &'static [Sample],
    pub schedule: Option<&'static [ScheduledHit]>,
    /// `(tick, entity count)` pairs. Empty means the count is not checked.
    pub entities: &'static [(u32, u32)],
}

impl Expectation {
    #[must_use]
    pub const fn samples(samples: &'static [Sample]) -> Self {
        Self {
            samples,
            schedule: None,
            entities: &[],
        }
    }

    #[must_use]
    pub const fn with_schedule(mut self, schedule: &'static [ScheduledHit]) -> Self {
        self.schedule = Some(schedule);
        self
    }

    #[must_use]
    pub const fn with_entities(mut self, entities: &'static [(u32, u32)]) -> Self {
        self.entities = entities;
        self
    }
}

/// One block in a layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub pos: RelPos,
    pub state: BlockStateId,
    /// When false, the block is written without neighbour or shape updates.
    pub notify: bool,
    /// Triggers are placed after every other cell.
    pub trigger: bool,
    /// Item registry name placed in slot 0 after the block exists. `water_bucket` is one.
    pub stock: Option<&'static str>,
}

/// Blocks that make up one layout, in placement order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub cells: Vec<Cell>,
}

/// One recorded vanilla layout.
#[derive(Clone, Copy, Debug)]
pub struct RedstoneOrderCase {
    pub id: &'static str,
    pub lane: RedstoneLane,
    /// Called by the runner. Add a function next to the others in `cases`.
    pub build: fn() -> Layout,
    pub expect: Expectation,
    /// Set when this build does not match vanilla yet. One line, no newline.
    pub ignored: Option<&'static str>,
}

/// Neighbour-update offsets: west, east, down, up, north, south.
pub const NEIGHBOR_UPDATE_ORDER: [(i32, i32, i32); 6] = [
    (-1, 0, 0),
    (1, 0, 0),
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
];

/// Shape-update offsets: west, east, north, south, down, up.
pub const SHAPE_UPDATE_ORDER: [(i32, i32, i32); 6] = [
    (-1, 0, 0),
    (1, 0, 0),
    (0, 0, -1),
    (0, 0, 1),
    (0, -1, 0),
    (0, 1, 0),
];

/// Drives one world. The pumpkin driver lives in this module's tests.
pub trait RedstoneProbe {
    fn reset(&self);
    fn place(&self, pos: RelPos, state: BlockStateId, notify: bool);
    /// Puts one item in slot 0. Returns false when the block has no inventory.
    fn stock(&self, pos: RelPos, item: &str) -> bool;
    fn state_id(&self, pos: RelPos) -> BlockStateId;
    fn schedule(&self) -> Vec<ScheduledHit>;
    /// One game tick: due block ticks, then synced block events, then block entities.
    fn step(&self);
    fn entity_count(&self) -> u32;
}

/// Outcome of comparing one case with a probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseReport {
    pub matched: bool,
    pub detail: String,
}

/// Places `case` and compares the probe with the recorded result.
#[must_use]
pub fn run_case(case: &RedstoneOrderCase, probe: &dyn RedstoneProbe) -> CaseReport {
    let layout = (case.build)();
    probe.reset();
    for cell in layout.cells.iter().filter(|cell| !cell.trigger) {
        probe.place(cell.pos, cell.state, cell.notify);
    }
    for cell in &layout.cells {
        if let Some(item) = cell.stock
            && !probe.stock(cell.pos, item)
        {
            return CaseReport {
                matched: false,
                detail: format!("harness: could not stock {item} at {:?}", cell.pos),
            };
        }
    }
    for cell in layout.cells.iter().filter(|cell| cell.trigger) {
        probe.place(cell.pos, cell.state, cell.notify);
    }

    if let Some(expected) = case.expect.schedule {
        let actual = scheduled_hits(&probe.schedule(), expected);
        if actual != expected {
            return CaseReport {
                matched: false,
                detail: format!("schedule {actual:?} != {expected:?}"),
            };
        }
    }

    let last_tick = case
        .expect
        .samples
        .iter()
        .map(|sample| sample.tick)
        .chain(case.expect.entities.iter().map(|(tick, _)| *tick))
        .max()
        .unwrap_or(0);

    for tick in 0..=last_tick {
        if tick > 0 {
            probe.step();
        }
        for sample in case
            .expect
            .samples
            .iter()
            .filter(|sample| sample.tick == tick)
        {
            let state_id = probe.state_id(sample.pos);
            if !signal_matches(state_id, sample.signal) {
                let block = Block::from_state_id(state_id);
                return CaseReport {
                    matched: false,
                    detail: format!(
                        "tick {tick} at {:?} expected {:?} but found {} ({state_id})",
                        sample.pos, sample.signal, block.name
                    ),
                };
            }
        }
        for (entity_tick, expected_count) in case.expect.entities {
            if *entity_tick == tick && probe.entity_count() != *expected_count {
                return CaseReport {
                    matched: false,
                    detail: format!(
                        "tick {tick} entity count {} != {expected_count}",
                        probe.entity_count()
                    ),
                };
            }
        }
    }

    CaseReport {
        matched: true,
        detail: String::new(),
    }
}

/// True when `state_id` shows `signal`.
#[must_use]
pub fn signal_matches(state_id: BlockStateId, signal: Signal) -> bool {
    let block = Block::from_state_id(state_id);
    match signal {
        Signal::Dust(power) => {
            block == &Block::REDSTONE_WIRE
                && RedstoneWireLikeProperties::from_state_id(state_id).power == power
        }
        Signal::Powered(powered) => {
            if block == &Block::REPEATER {
                RepeaterLikeProperties::from_state_id(state_id).powered == powered
            } else if block == &Block::COMPARATOR {
                ComparatorLikeProperties::from_state_id(state_id).powered == powered
            } else if block == &Block::OBSERVER {
                ObserverLikeProperties::from_state_id(state_id).powered == powered
            } else {
                false
            }
        }
        Signal::Extended(extended) => {
            (block == &Block::PISTON || block == &Block::STICKY_PISTON)
                && StickyPistonLikeProperties::from_state_id(state_id).extended == extended
        }
        Signal::Block(name) => block.name == name,
    }
}

fn scheduled_hits(actual: &[ScheduledHit], expected: &[ScheduledHit]) -> Vec<ScheduledHit> {
    let wanted: HashSet<RelPos> = expected.iter().map(|hit| hit.pos).collect();
    actual
        .iter()
        .filter(|hit| wanted.contains(&hit.pos))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests;
