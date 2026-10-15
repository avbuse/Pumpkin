//! Recorded layouts.
//!
//! Add a case by writing a `build` function and a row in [`CASES`]. Leave `ignored`
//! empty only when a run already matches. A mismatch keeps the row and sets `ignored`
//! to one line saying what still differs. A-37, A-38, A-39, and A-41 clear that note
//! on the row they fix. A-40 adds sticky-piston retract and quasi-connectivity rows
//! the same way.
//!
//! The numbers below were read from a frozen 26.3 world: place the trigger, then
//! `/tick step` one game tick at a time. Repeater setting 4 powers its output on tick
//! 8. A comparator and an observer resolve on tick 2, and the observer is dark again
//! on tick 4. A dispenser acts on tick 4. A piston is still a moving block on ticks
//! 1 and 2, and the pushed block is in place on tick 3.
//!
//! Facing on a repeater or comparator is the side it reads. The piston, observer, and
//! dispenser facing values are the direction they push, watch, and shoot.

use pumpkin_data::block_properties::{Facing, HorizontalFacing, ModeComparator};

use super::states;
use super::{
    Cell, Expectation, Layout, NEIGHBOR_UPDATE_ORDER, RedstoneLane, RedstoneOrderCase, RelPos,
    SHAPE_UPDATE_ORDER, Sample, ScheduledHit, Signal,
};

const fn at(x: i32, y: i32, z: i32) -> RelPos {
    RelPos::new(x, y, z)
}

fn support(pos: RelPos) -> Cell {
    Cell {
        pos: RelPos::new(pos.x, pos.y - 1, pos.z),
        state: states::stone(),
        notify: false,
        trigger: false,
        stock: None,
    }
}

fn wire(pos: RelPos) -> Cell {
    Cell {
        pos,
        state: states::dust(),
        notify: true,
        trigger: false,
        stock: None,
    }
}

const fn placed(pos: RelPos, state: pumpkin_data::BlockStateId) -> Cell {
    Cell {
        pos,
        state,
        notify: true,
        trigger: false,
        stock: None,
    }
}

const fn trigger(pos: RelPos, state: pumpkin_data::BlockStateId) -> Cell {
    Cell {
        pos,
        state,
        notify: true,
        trigger: true,
        stock: None,
    }
}

fn dust_beside_source() -> Layout {
    let dust = at(1, 0, 0);
    let source = at(0, 0, 0);
    Layout {
        cells: vec![
            support(dust),
            support(source),
            wire(dust),
            trigger(source, states::redstone_block()),
        ],
    }
}

fn dust_line_of_fifteen() -> Layout {
    let mut cells = Vec::new();
    cells.push(support(at(0, 0, 0)));
    for x in 1..=15 {
        cells.push(support(at(x, 0, 0)));
        cells.push(wire(at(x, 0, 0)));
    }
    cells.push(trigger(at(0, 0, 0), states::redstone_block()));
    Layout { cells }
}

fn dust_branch() -> Layout {
    let spots = [
        at(1, 0, 0),
        at(2, 0, 0),
        at(3, 0, 0),
        at(2, 0, 1),
        at(2, 0, -1),
    ];
    let mut cells = vec![support(at(0, 0, 0))];
    for spot in spots {
        cells.push(support(spot));
        cells.push(wire(spot));
    }
    cells.push(trigger(at(0, 0, 0), states::redstone_block()));
    Layout { cells }
}

fn dust_signal_stops_at_zero() -> Layout {
    let mut cells = vec![support(at(0, 0, 0))];
    for x in 1..=16 {
        cells.push(support(at(x, 0, 0)));
        cells.push(wire(at(x, 0, 0)));
    }
    cells.push(trigger(at(0, 0, 0), states::redstone_block()));
    Layout { cells }
}

fn repeater_four_tick_delay() -> Layout {
    // Facing east reads +X and powers -X eight game ticks later.
    let output = at(-1, 0, 0);
    let gate = at(0, 0, 0);
    let input = at(1, 0, 0);
    Layout {
        cells: vec![
            support(output),
            support(gate),
            support(input),
            wire(output),
            placed(gate, states::repeater(HorizontalFacing::East, 4)),
            trigger(input, states::redstone_block()),
        ],
    }
}

fn repeater_blocks_backward_signal() -> Layout {
    // The block sits on the output side. The dust on the input side stays dark.
    let output = at(-1, 0, 0);
    let gate = at(0, 0, 0);
    let input = at(1, 0, 0);
    Layout {
        cells: vec![
            support(output),
            support(gate),
            support(input),
            wire(input),
            placed(gate, states::repeater(HorizontalFacing::East, 1)),
            trigger(output, states::redstone_block()),
        ],
    }
}

const fn repeater_facing_toward_center(offset: (i32, i32, i32)) -> HorizontalFacing {
    match offset {
        (-1, 0, 0) => HorizontalFacing::East,
        (1, 0, 0) => HorizontalFacing::West,
        (0, 0, -1) => HorizontalFacing::South,
        // South is the remaining horizontal neighbour.
        _ => HorizontalFacing::North,
    }
}

fn repeater_neighbor_order() -> Layout {
    let mut cells = vec![support(at(0, 0, 0))];
    for offset in NEIGHBOR_UPDATE_ORDER {
        if offset.1 != 0 {
            continue;
        }
        let pos = at(offset.0, 0, offset.2);
        cells.push(support(pos));
        cells.push(placed(
            pos,
            states::repeater(repeater_facing_toward_center(offset), 1),
        ));
    }
    cells.push(trigger(at(0, 0, 0), states::redstone_block()));
    Layout { cells }
}

fn comparator_layout(mode: ModeComparator) -> Layout {
    let output = at(-1, 0, 0);
    let gate = at(0, 0, 0);
    let rear = at(1, 0, 0);
    let side = at(0, 0, -1);
    Layout {
        cells: vec![
            support(output),
            support(gate),
            support(rear),
            support(side),
            wire(output),
            placed(gate, states::comparator(HorizontalFacing::East, mode)),
            placed(side, states::redstone_block()),
            trigger(rear, states::redstone_block()),
        ],
    }
}

fn comparator_compare() -> Layout {
    comparator_layout(ModeComparator::Compare)
}

fn comparator_subtract() -> Layout {
    comparator_layout(ModeComparator::Subtract)
}

fn piston_row(front: pumpkin_data::BlockStateId, length: i32) -> Layout {
    let gate = at(0, 0, 0);
    let power = at(-1, 0, 0);
    let mut cells = vec![
        support(gate),
        support(power),
        placed(gate, states::piston(Facing::East)),
    ];
    for x in 1..=length {
        let pos = at(x, 0, 0);
        cells.push(support(pos));
        cells.push(placed(pos, front));
    }
    cells.push(trigger(power, states::redstone_block()));
    Layout { cells }
}

fn piston_pushes_one_stone() -> Layout {
    piston_row(states::stone(), 1)
}

fn piston_does_not_push_bedrock() -> Layout {
    piston_row(states::bedrock(), 1)
}

fn piston_over_push_limit() -> Layout {
    // Thirteen is one past the twelve-block push limit.
    piston_row(states::stone(), 13)
}

const fn observer_facing_toward_center(offset: (i32, i32, i32)) -> Facing {
    match offset {
        (-1, 0, 0) => Facing::East,
        (1, 0, 0) => Facing::West,
        (0, 0, -1) => Facing::South,
        (0, -1, 0) => Facing::Up,
        (0, 1, 0) => Facing::Down,
        // South is the remaining neighbour.
        _ => Facing::North,
    }
}

fn observer_shape_update_order() -> Layout {
    let mut cells = Vec::new();
    for offset in SHAPE_UPDATE_ORDER {
        cells.push(placed(
            at(offset.0, offset.1, offset.2),
            states::observer(observer_facing_toward_center(offset)),
        ));
    }
    cells.push(trigger(at(0, 0, 0), states::stone()));
    Layout { cells }
}

fn observer_pulse_lasts_one_redstone_tick() -> Layout {
    // The observer sits east of the trigger and watches west.
    let watched = at(0, 0, 0);
    let eye = at(1, 0, 0);
    Layout {
        cells: vec![
            support(watched),
            support(eye),
            placed(eye, states::observer(Facing::West)),
            trigger(watched, states::stone()),
        ],
    }
}

fn dispenser_layout(stock: Option<&'static str>) -> Layout {
    let gate = at(0, 0, 0);
    let power = at(-1, 0, 0);
    let mut dispenser = placed(gate, states::dispenser(Facing::East));
    dispenser.stock = stock;
    Layout {
        cells: vec![
            support(gate),
            support(power),
            support(at(1, 0, 0)),
            dispenser,
            trigger(power, states::redstone_block()),
        ],
    }
}

fn dispenser_empty_places_nothing() -> Layout {
    dispenser_layout(None)
}

fn dispenser_water_bucket_places_source() -> Layout {
    dispenser_layout(Some("water_bucket"))
}

const DUST_BESIDE: &[Sample] = &[Sample {
    tick: 0,
    pos: at(1, 0, 0),
    signal: Signal::Dust(15),
}];

const DUST_LINE: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(1, 0, 0),
        signal: Signal::Dust(15),
    },
    Sample {
        tick: 0,
        pos: at(2, 0, 0),
        signal: Signal::Dust(14),
    },
    Sample {
        tick: 0,
        pos: at(3, 0, 0),
        signal: Signal::Dust(13),
    },
    Sample {
        tick: 0,
        pos: at(4, 0, 0),
        signal: Signal::Dust(12),
    },
    Sample {
        tick: 0,
        pos: at(5, 0, 0),
        signal: Signal::Dust(11),
    },
    Sample {
        tick: 0,
        pos: at(6, 0, 0),
        signal: Signal::Dust(10),
    },
    Sample {
        tick: 0,
        pos: at(7, 0, 0),
        signal: Signal::Dust(9),
    },
    Sample {
        tick: 0,
        pos: at(8, 0, 0),
        signal: Signal::Dust(8),
    },
    Sample {
        tick: 0,
        pos: at(9, 0, 0),
        signal: Signal::Dust(7),
    },
    Sample {
        tick: 0,
        pos: at(10, 0, 0),
        signal: Signal::Dust(6),
    },
    Sample {
        tick: 0,
        pos: at(11, 0, 0),
        signal: Signal::Dust(5),
    },
    Sample {
        tick: 0,
        pos: at(12, 0, 0),
        signal: Signal::Dust(4),
    },
    Sample {
        tick: 0,
        pos: at(13, 0, 0),
        signal: Signal::Dust(3),
    },
    Sample {
        tick: 0,
        pos: at(14, 0, 0),
        signal: Signal::Dust(2),
    },
    Sample {
        tick: 0,
        pos: at(15, 0, 0),
        signal: Signal::Dust(1),
    },
];

const DUST_BRANCH: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(1, 0, 0),
        signal: Signal::Dust(15),
    },
    Sample {
        tick: 0,
        pos: at(2, 0, 0),
        signal: Signal::Dust(14),
    },
    Sample {
        tick: 0,
        pos: at(3, 0, 0),
        signal: Signal::Dust(13),
    },
    Sample {
        tick: 0,
        pos: at(2, 0, 1),
        signal: Signal::Dust(13),
    },
    Sample {
        tick: 0,
        pos: at(2, 0, -1),
        signal: Signal::Dust(13),
    },
];

const DUST_STOPS: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(15, 0, 0),
        signal: Signal::Dust(1),
    },
    Sample {
        tick: 0,
        pos: at(16, 0, 0),
        signal: Signal::Dust(0),
    },
];

const REPEATER_DELAY: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(0, 0, 0),
        signal: Signal::Powered(false),
    },
    Sample {
        tick: 0,
        pos: at(-1, 0, 0),
        signal: Signal::Dust(0),
    },
    Sample {
        tick: 7,
        pos: at(0, 0, 0),
        signal: Signal::Powered(false),
    },
    Sample {
        tick: 7,
        pos: at(-1, 0, 0),
        signal: Signal::Dust(0),
    },
    Sample {
        tick: 8,
        pos: at(0, 0, 0),
        signal: Signal::Powered(true),
    },
    Sample {
        tick: 8,
        pos: at(-1, 0, 0),
        signal: Signal::Dust(15),
    },
];

const REPEATER_DIODE: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(0, 0, 0),
        signal: Signal::Powered(false),
    },
    Sample {
        tick: 0,
        pos: at(1, 0, 0),
        signal: Signal::Dust(0),
    },
    Sample {
        tick: 4,
        pos: at(0, 0, 0),
        signal: Signal::Powered(false),
    },
    Sample {
        tick: 4,
        pos: at(1, 0, 0),
        signal: Signal::Dust(0),
    },
];

const REPEATER_ORDER: &[ScheduledHit] = &[
    ScheduledHit {
        delay: 2,
        pos: at(-1, 0, 0),
        block: "repeater",
    },
    ScheduledHit {
        delay: 2,
        pos: at(1, 0, 0),
        block: "repeater",
    },
    ScheduledHit {
        delay: 2,
        pos: at(0, 0, -1),
        block: "repeater",
    },
    ScheduledHit {
        delay: 2,
        pos: at(0, 0, 1),
        block: "repeater",
    },
];

const COMPARATOR_ON: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(-1, 0, 0),
        signal: Signal::Dust(0),
    },
    Sample {
        tick: 1,
        pos: at(-1, 0, 0),
        signal: Signal::Dust(0),
    },
    Sample {
        tick: 2,
        pos: at(0, 0, 0),
        signal: Signal::Powered(true),
    },
    Sample {
        tick: 2,
        pos: at(-1, 0, 0),
        signal: Signal::Dust(15),
    },
];

const COMPARATOR_SUB: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(-1, 0, 0),
        signal: Signal::Dust(0),
    },
    Sample {
        tick: 2,
        pos: at(0, 0, 0),
        signal: Signal::Powered(false),
    },
    Sample {
        tick: 2,
        pos: at(-1, 0, 0),
        signal: Signal::Dust(0),
    },
    Sample {
        tick: 4,
        pos: at(-1, 0, 0),
        signal: Signal::Dust(0),
    },
];

const PISTON_PUSH: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(0, 0, 0),
        signal: Signal::Extended(false),
    },
    Sample {
        tick: 0,
        pos: at(1, 0, 0),
        signal: Signal::Block("stone"),
    },
    Sample {
        tick: 1,
        pos: at(0, 0, 0),
        signal: Signal::Extended(true),
    },
    Sample {
        tick: 1,
        pos: at(1, 0, 0),
        signal: Signal::Block("moving_piston"),
    },
    Sample {
        tick: 1,
        pos: at(2, 0, 0),
        signal: Signal::Block("moving_piston"),
    },
    Sample {
        tick: 3,
        pos: at(0, 0, 0),
        signal: Signal::Extended(true),
    },
    Sample {
        tick: 3,
        pos: at(1, 0, 0),
        signal: Signal::Block("piston_head"),
    },
    Sample {
        tick: 3,
        pos: at(2, 0, 0),
        signal: Signal::Block("stone"),
    },
];

const PISTON_BEDROCK: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(0, 0, 0),
        signal: Signal::Extended(false),
    },
    Sample {
        tick: 0,
        pos: at(1, 0, 0),
        signal: Signal::Block("bedrock"),
    },
    Sample {
        tick: 1,
        pos: at(0, 0, 0),
        signal: Signal::Extended(false),
    },
    Sample {
        tick: 1,
        pos: at(1, 0, 0),
        signal: Signal::Block("bedrock"),
    },
    Sample {
        tick: 3,
        pos: at(0, 0, 0),
        signal: Signal::Extended(false),
    },
    Sample {
        tick: 3,
        pos: at(2, 0, 0),
        signal: Signal::Block("air"),
    },
];

const PISTON_LIMIT: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(0, 0, 0),
        signal: Signal::Extended(false),
    },
    Sample {
        tick: 1,
        pos: at(0, 0, 0),
        signal: Signal::Extended(false),
    },
    Sample {
        tick: 1,
        pos: at(1, 0, 0),
        signal: Signal::Block("stone"),
    },
    Sample {
        tick: 1,
        pos: at(13, 0, 0),
        signal: Signal::Block("stone"),
    },
    Sample {
        tick: 1,
        pos: at(14, 0, 0),
        signal: Signal::Block("air"),
    },
    Sample {
        tick: 3,
        pos: at(0, 0, 0),
        signal: Signal::Extended(false),
    },
    Sample {
        tick: 3,
        pos: at(13, 0, 0),
        signal: Signal::Block("stone"),
    },
];

const OBSERVER_ORDER: &[ScheduledHit] = &[
    ScheduledHit {
        delay: 2,
        pos: at(-1, 0, 0),
        block: "observer",
    },
    ScheduledHit {
        delay: 2,
        pos: at(1, 0, 0),
        block: "observer",
    },
    ScheduledHit {
        delay: 2,
        pos: at(0, 0, -1),
        block: "observer",
    },
    ScheduledHit {
        delay: 2,
        pos: at(0, 0, 1),
        block: "observer",
    },
    ScheduledHit {
        delay: 2,
        pos: at(0, -1, 0),
        block: "observer",
    },
    ScheduledHit {
        delay: 2,
        pos: at(0, 1, 0),
        block: "observer",
    },
];

const OBSERVER_PULSE: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(1, 0, 0),
        signal: Signal::Powered(false),
    },
    Sample {
        tick: 1,
        pos: at(1, 0, 0),
        signal: Signal::Powered(false),
    },
    Sample {
        tick: 2,
        pos: at(1, 0, 0),
        signal: Signal::Powered(true),
    },
    Sample {
        tick: 3,
        pos: at(1, 0, 0),
        signal: Signal::Powered(true),
    },
    Sample {
        tick: 4,
        pos: at(1, 0, 0),
        signal: Signal::Powered(false),
    },
];

const DISPENSER_EMPTY: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(0, 0, 0),
        signal: Signal::Block("dispenser"),
    },
    Sample {
        tick: 0,
        pos: at(1, 0, 0),
        signal: Signal::Block("air"),
    },
    Sample {
        tick: 4,
        pos: at(1, 0, 0),
        signal: Signal::Block("air"),
    },
];

const DISPENSER_EMPTY_ENTITIES: &[(u32, u32)] = &[(0, 0), (4, 0)];

const DISPENSER_WATER: &[Sample] = &[
    Sample {
        tick: 0,
        pos: at(1, 0, 0),
        signal: Signal::Block("air"),
    },
    Sample {
        tick: 3,
        pos: at(1, 0, 0),
        signal: Signal::Block("air"),
    },
    Sample {
        tick: 4,
        pos: at(1, 0, 0),
        signal: Signal::Block("water"),
    },
];

const DISPENSER_WATER_ENTITIES: &[(u32, u32)] = &[(4, 0)];

const CASES: &[RedstoneOrderCase] = &[
    RedstoneOrderCase {
        id: "dust_beside_source",
        lane: RedstoneLane::Dust,
        build: dust_beside_source,
        expect: Expectation::samples(DUST_BESIDE),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "dust_line_of_fifteen",
        lane: RedstoneLane::Dust,
        build: dust_line_of_fifteen,
        expect: Expectation::samples(DUST_LINE),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "dust_branch",
        lane: RedstoneLane::Dust,
        build: dust_branch,
        expect: Expectation::samples(DUST_BRANCH),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "dust_signal_stops_at_zero",
        lane: RedstoneLane::Dust,
        build: dust_signal_stops_at_zero,
        expect: Expectation::samples(DUST_STOPS),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "repeater_four_tick_delay",
        lane: RedstoneLane::Repeater,
        build: repeater_four_tick_delay,
        expect: Expectation::samples(REPEATER_DELAY),
        ignored: Some("repeater still unpowered at tick 8; it powers on tick 9"),
    },
    RedstoneOrderCase {
        id: "repeater_blocks_backward_signal",
        lane: RedstoneLane::Repeater,
        build: repeater_blocks_backward_signal,
        expect: Expectation::samples(REPEATER_DIODE),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "repeater_neighbor_order",
        lane: RedstoneLane::Repeater,
        build: repeater_neighbor_order,
        expect: Expectation::samples(&[]).with_schedule(REPEATER_ORDER),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "comparator_compare",
        lane: RedstoneLane::Comparator,
        build: comparator_compare,
        expect: Expectation::samples(COMPARATOR_ON),
        ignored: Some("comparator still unpowered at tick 2; it powers on tick 3"),
    },
    RedstoneOrderCase {
        id: "comparator_subtract",
        lane: RedstoneLane::Comparator,
        build: comparator_subtract,
        expect: Expectation::samples(COMPARATOR_SUB),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "piston_pushes_one_stone",
        lane: RedstoneLane::Piston,
        build: piston_pushes_one_stone,
        expect: Expectation::samples(PISTON_PUSH),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "piston_does_not_push_bedrock",
        lane: RedstoneLane::Piston,
        build: piston_does_not_push_bedrock,
        expect: Expectation::samples(PISTON_BEDROCK),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "piston_over_push_limit",
        lane: RedstoneLane::Piston,
        build: piston_over_push_limit,
        expect: Expectation::samples(PISTON_LIMIT),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "observer_shape_update_order",
        lane: RedstoneLane::Observer,
        build: observer_shape_update_order,
        expect: Expectation::samples(&[]).with_schedule(OBSERVER_ORDER),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "observer_pulse_lasts_one_redstone_tick",
        lane: RedstoneLane::Observer,
        build: observer_pulse_lasts_one_redstone_tick,
        expect: Expectation::samples(OBSERVER_PULSE),
        ignored: Some("observer still dark at tick 2; it powers on tick 3"),
    },
    RedstoneOrderCase {
        id: "dispenser_empty_places_nothing",
        lane: RedstoneLane::Dispenser,
        build: dispenser_empty_places_nothing,
        expect: Expectation::samples(DISPENSER_EMPTY).with_entities(DISPENSER_EMPTY_ENTITIES),
        ignored: None,
    },
    RedstoneOrderCase {
        id: "dispenser_water_bucket_places_source",
        lane: RedstoneLane::Dispenser,
        build: dispenser_water_bucket_places_source,
        expect: Expectation::samples(DISPENSER_WATER).with_entities(DISPENSER_WATER_ENTITIES),
        ignored: Some("dispenser front still air at tick 4; water appears on tick 5"),
    },
];

/// Every layout the fixture knows about.
#[must_use]
pub const fn cases() -> &'static [RedstoneOrderCase] {
    CASES
}

/// One layout by its id.
#[must_use]
pub fn case(id: &str) -> Option<&'static RedstoneOrderCase> {
    CASES.iter().find(|case| case.id == id)
}
