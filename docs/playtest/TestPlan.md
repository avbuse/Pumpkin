# A-37 test plan

Dust power only. The default wire evaluator stays the one already in `redstone_wire.rs`. Alternate Current and the experimental redstone datapack stay off.

## Automated

From the repo root, after `cargo build --workspace`:

```bash
cargo test -p pumpkin-gametest --lib block_based::redstone_order::tests -- --test-threads=1
cargo clippy -p pumpkin-gametest --all-targets --all-features
cargo fmt --check
```

`recorded_layouts_match_or_stay_ignored` places each layout, compares it with the recorded result, and fails if a live row mismatches or an ignored row suddenly matches. Tick 0 is the trigger placement, after neighbour updates and before any game tick.

Dust rows that must pass, all on tick 0:

| Id | Expect |
| --- | --- |
| `dust_beside_source` | Dust next to a redstone block is power 15. |
| `dust_line_of_fifteen` | Powers 15, 14, …, 1 along a straight line. |
| `dust_branch` | Junction 14, and the straight continuation and both arms are 13. |
| `dust_signal_stops_at_zero` | Step 15 is power 1. Step 16 is power 0. |
| `dust_climbs_onto_block` | Dust on the block beside a power-15 dust is 14. |
| `dust_climb_blocked_by_solid` | Solid block above the lower dust: the upper dust stays 0. |
| `dust_drops_off_block` | Dust one block lower, beside air, is 14. |
| `dust_drop_blocked_by_solid` | Solid beside and above the lower dust: it stays 0. |
| `dust_staircase_loses_one_per_step` | Three rising steps are 15, 14, 13. |
| `dust_weak_power_stops_at_solid` | Redstone block, stone, dust: the dust stays 0. |
| `dust_reads_strong_power_through_solid` | A powered repeater strongly powers stone. Dust on the far side is 15. |
| `dust_line_goes_dark_when_source_breaks` | Replacing the block with air leaves four dust at 0. |
| `dust_takes_the_stronger_end` | Blocks on both ends: dust is 15, 14, 15. |
| `dust_powered_from_above` | A redstone block above dust sets power 15. |
| `dust_powered_from_below` | A redstone block under dust sets power 15. |
| `dust_break_in_the_middle_cuts_the_line` | The near dust stays 15. The far side of the gap is 0. |

The four original dust rows are the frozen 26.3 measurements already in the fixture. The rows added for A-37 use that same default evaluator: neighbouring dust contributes one less than its power, a solid block blocks the vertical link, weak power does not cross a solid block, and strong power into the block is taken in full. They were checked by running the fixture, not by a second live `/tick step` session.

The in-game checklist is `docs/playtest/A-37.md`.

## One tick late (not this task)

Four ignored rows are each one game tick behind the 26.3 recording:

| Id | Recorded | Pumpkin | Owner |
| --- | --- | --- | --- |
| `repeater_four_tick_delay` | Powers on tick 8 | Powers on tick 9 | A-38 |
| `comparator_compare` | Powers on tick 2 | Powers on tick 3 | A-38 |
| `observer_pulse_lasts_one_redstone_tick` | Powers on tick 2 | Powers on tick 3 | A-41 |
| `dispenser_water_bucket_places_source` | Water on tick 4 | Water on tick 5 | A-41 |

The delays those blocks schedule are 8, 2, 2, and 4. `ChunkTickScheduler` in `crates/pumpkin-world/src/tick/scheduler.rs` stores a delay of N at slot `offset + N` and `step_tick` consumes slot `offset`, so the hit runs on step N + 1. A 26.3 tick with delay N runs on step N. Dust never schedules a block tick, so this miss is outside `redstone_wire.rs`.

A-39's piston rows already match. They move by block events, not by those delayed block ticks. Fixing the scheduler will change when repeater, comparator, observer, and dispenser rows clear their `ignored` notes. Do that in A-38 and A-41, not here.

## Out of scope

- Alternate Current (`B-21`).
- Comparator measure and repeater diode delay (`A-38`), aside from using an already-powered repeater as a strong source.
- Piston push (`A-39`).
