# Test plan: A-48 chunk format 5023

Branch `core/A-48-chunk-5023`. Confirm this branch before it is merged into `core-functionality` `f453f24a`.

This branch reads three block-state palette shapes and writes 5023:

- a 4903 compound with `Name` and optional `Properties`
- a 5023 list of block-id strings, when every entry is the block's default state
- a 5023 list of compounds, where a default entry is an empty-named string and any other entry is `id` plus `properties`

A default furnace (facing north, unlit) is a default state and is written as the block id. Oak stairs facing east are not. Biomes stay a string palette. Block-entity compounds are copied with their original tag types: furnace cook times, brewing numbers, and pot sherds are not rewritten. An unknown block id becomes air, with no log line. Lighting is copied, not recomputed.

This branch now includes `core-functionality` `f453f24a`, including A-05 time, weather, border, and scoreboard restore. A reset of those after `stop` is a fail. The chunk format file does not overlap the core commits after `50a0a2e6`. The 5023 writer on this branch is still this branch's change.

Worlds outside data version 4435..5023 are rejected with `Unsupported world version`. 26.2 is 4903. 26.3 is 5023. Both are in range. Copy a world. Do not test against the only copy.

## Setup

Run this branch from a scratch directory so the server writes `pumpkin.toml` and `world/` there:

```bash
mkdir -p /tmp/pumpkin-A-48 && cd /tmp/pumpkin-A-48
cargo run --manifest-path <repo>/Cargo.toml -p pumpkin
```

Debug build. Wait for `Started server; took`, then `Server is now running. Connect using port: Java Edition: 0.0.0.0:25565`. Stop with `stop`. The same text is in `latest.log`.

A fresh boot enables `vanilla`, `pumpkin-unit-test`, and `pumpkin-unit-test-example`. That list is normal.

Defaults the steps rely on:

- `default_gamemode = "Survival"`, `force_gamemode = false`. Leave `force_gamemode` false.
- `default_level_name = "world"`. `level.dat` sits directly inside `world/`.
- Java port `25565`, `online_mode = true`, `encryption = true`. A licensed 26.3 client joins with those defaults.
- `spawn_protection = 16`. From the console, `op <playername>` before breaking or placing blocks.
- `allow_nether = true`, `allow_end = true`.
- `[world] autosave_ticks = 6000` (5 minutes). This branch does not change it. A-20 is the branch that lowers it.
- Player files are `world/players/data/<uuid>.dat` (hyphenated UUID, gzip NBT). That is not the vanilla `playerdata/` folder.

Each step has a checkbox. Record pass, fail, or not run, plus the evidence named on the step. A crash, a panic, or a log error that names the thing you just did is also a fail.

**Client** means a licensed Java Edition 26.3 client. **Console** means the server console only.


## Tests

- [ ] **1. Vanilla 26.3 world opens** (Client)

  Create a world in vanilla 26.3. Remember the seed. Place, somewhere you can find again: oak stairs facing east, a furnace, a stone block, a chest with one diamond, a sign with text, and a decorated pot if you have sherds. Stand where you can see deepslate or dig to it. Stop vanilla so the region files flush.
  Copy that save in as Pumpkin's `world/`. Start this branch. Join with a 26.3 client.
  Expect the terrain you left, not a hollow of air. In the overworld the bottom (`y = -64`) is bedrock. The deepslate band is deepslate. Stone, dirt, grass, sand, and logs are the blocks you remember. The stairs still face east. The furnace is a furnace. The chest, sign, and pot still have their contents and text. Water and lava still sit where you left them.
  **Fail:** stone or deepslate replaced by air, stairs lost their facing, or the chest is empty.
  **Evidence:** the same spot in vanilla before the copy, then in Pumpkin. A short walk from a cave floor to the surface is enough. Deepslate or stone underfoot must be visible.

- [ ] **2. Edit, save, reopen in vanilla 26.3** (Client)

  In Pumpkin, break one block and place oak stairs facing east, a furnace, and a stone block in a chunk you can find again. Note the coordinates. `stop` and wait until the save finishes.
  Open the same world in vanilla 26.3.
  Expect vanilla to open with no `Recoverable errors when loading section` in its log. Terrain you did not touch is unchanged. The stairs, furnace, and stone are those blocks, with the facing you set. A furnace inventory and sign text you did not edit are still there.
  **Fail:** vanilla reports recoverable section errors, or the placed stairs or furnace are air.
  **Evidence:** the vanilla log around the load, and the same coordinates in vanilla.

- [ ] **3. A 26.2 world still loads** (Client)

  Create a small world in vanilla 26.2 (data version 4903). Stop it. Copy it in as `world/`. Start this branch and join.
  Expect terrain, bedrock at the bottom, and a placed furnace or stone block to still be that block. Tag upgrades are not required. The block has to be the block.
  **Fail:** the 26.2 world opens as air, or the server rejects it as an unsupported version.
  **Evidence:** one screenshot of solid ground and the furnace or stone you placed in 26.2.

- [ ] **4. A world saved by current core opens here, then in vanilla** (Client)

  On `core-functionality` `f453f24a`, new world, place oak stairs facing east, a furnace, and stone. `stop`.
  Copy that `world/` onto this branch. Start this branch and join.
  Expect those blocks, not air. Current core writes `Name` / `Properties` palettes labelled 5023, which this branch still reads.
  `stop` on this branch. Open that same world in vanilla 26.3.
  Expect no `Recoverable errors when loading section`. The stairs, furnace, and stone survived. Vanilla reading the world before this branch rewrites it is the bug: core's palette is not a 5023 palette. You do not need to demonstrate that failure again unless the rewrite step still produces the recoverable-error line.
  **Fail:** Pumpkin shows air for those blocks, or vanilla logs recoverable section errors after this branch has saved.
  **Evidence:** the blocks in Pumpkin, then the vanilla log and the same blocks.

- [ ] **5. Boot of a brand new world** (Console)

  Empty scratch directory on this branch. Start, wait for `Started server; took`, `stop`.
  Expect no panic and no `Unsupported world version`.
  **Fail:** a panic while creating or saving the level.
  **Evidence:** the log.

- [ ] **6. Time survives stop** (Console)

  A-05 is on this branch. `time set 1000`, `stop`, start, `time query daytime`.
  Expect about 1000, plus any ticks that passed before `stop`.
  **Fail:** daytime back at the new-world value. Weather, the world border, and the scoreboard follow the same rule. A reset is a fail. The full A-05 checklist is not required.
  **Evidence:** the two query lines.

## Where this branch sits

Suggested merge order into `core-functionality` `f453f24a`. A-16 is already on that core (`6b0d37545`), so it is not a remaining step:

1. `core/A-45-worldgen-baseline` (docs only)
2. `core/A-42-predicates` (datapacks, isolated from world save)
3. `core/A-48-chunk-5023` (chunk codec, before saves vanilla must read)
4. `core/A-20-container-persist` (autosave flush into the chunk A-48 writes)
5. `core/A-06-player-data` (writes the bed spawn, and the rest of the player file)
6. `core/A-24-melee` last, after Sweeping Edge deals more than 1, rebased onto A-06

Only A-06 and A-24 both change `crates/pumpkin-core/src/entity/player.rs`. A-48's chunk format file does not overlap the core commits after `50a0a2e6`.

This branch is step 3, before A-20. After both are merged, place cobblestone in a chest and coal in a furnace in the same chunk, set `autosave_ticks = 200`, stay in the chunk, wait for `Saving <N> chunks...`, `kill -9`, and start. Both items must remain. Then `stop` and open that world in vanilla 26.3: the chest exists and the cobblestone is still in it.
