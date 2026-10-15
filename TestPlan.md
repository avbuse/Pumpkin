# A-13 test plan

Automated checks live in `crates/pumpkin-data/src/block_transformer_tests.rs`. They call the generated axe, shovel, and hoe transformers. They do not boot a world.

| Check | Expected |
| --- | --- |
| Oak log with axis `x` | Stripped oak log, axis `x`, strip sound, 1 durability |
| Exposed copper | Copper block, scrape sound and particles |
| Oxidized copper | Weathered copper, one stage |
| Waxed oxidized copper | Oxidized copper, wax-off sound and particles |
| Exposed copper door, upper half, facing west, open | Copper door with those properties kept |
| Grass with air or cave air above, clicked from above | Dirt path, flatten sound, 1 durability |
| Grass with stone or water above, or clicked from below | No path |
| Dirt or grass with air above, not from below | Farmland, moisture 0, till sound, 1 durability |
| Dirt with stone above, or clicked from below | No farmland |
| Coarse dirt with air above | Dirt, not farmland. Stone above blocks it |
| Rooted dirt from below with stone above | Dirt, loot `minecraft:till/rooted_dirt`, drop from the clicked face |
| Stick item id | Not in the axe, shovel, or hoe tags. Stone does not transform |

`cargo test -p pumpkin-data --lib block_transformer_tests` runs that table.

Not covered by those tests, and listed in `docs/playtest/A-13.md`:

- Adventure mode refuses strip, path, and till because `mayBuild` is false. A shovel still puts out a lit campfire.
- Creative pays no durability.
- A copper door's other half changes with the clicked half.
- A double copper chest changes both halves and keeps its items.
- The rooted-dirt loot table actually spawns a hanging-roots item.
- A real client hears the sound and shows the particles.
