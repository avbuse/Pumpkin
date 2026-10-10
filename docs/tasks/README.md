# Task index

Work breakdown for `ROADMAP.md`. One file per task. Counts: **48** in phase A, **30** in phase B, **10** in phase C, **13** in phase D, **101** total. **66** target `core-functionality` and **35** target `enhanced-functionality`. The rule is in `ROADMAP.md`. Branch mechanics are in `docs/BRANCHING.md`.

## How to pick up a task

1. Read the task file and `ROADMAP.md`.
2. Cut the task branch named in the task file from that task's primary branch (`core-functionality` or `enhanced-functionality`). One task per branch. Merge it back into that same primary branch. See `docs/BRANCHING.md`.
3. Stay inside the files the task names. If you need a file another running task owns, stop and pick a different task.
4. Commit with a conventional subject and `git commit -s`. Push to `avbuse/Pumpkin` only.
5. Do not open a pull request. Do not push or comment on `Pumpkin-MC/Pumpkin`.
6. Do not commit Mojang jars, assets, or decompiled source. Do not paste Java into the repo.
7. Behaviour-changing config defaults to vanilla or off.
8. Do not build a general DataFixer. Phase A accepts data version 5023 and the chunk fields `A-08` lists.

Size is one agent session: **S** is a few files, **M** is one focused behaviour and its test, **L** is the region tick switch only (`B-19`).

Parity risk is what a player can observe if the change is wrong. **None** is a test, a doc, or a measurement. **High** means combat, explosions, redstone order, or a feature that must stay off by default.

## Hot files

Only one task edits these at a time.

| File | Order |
| --- | --- |
| `crates/pumpkin-core/src/world/mod.rs` | On core: `A-05` (load), then `B-03` (entity queries), then `B-17` (sequential tick). On enhanced, after those are merged in: `B-19` (region tick). Other tasks stay out. |
| `crates/pumpkin-world/src/chunk/format/anvil.rs` | `A-04`, then `A-07` and `A-48` one after another. |
| `crates/pumpkin-core/src/net/java/pending.rs` | `A-09`, then `A-12`. |
| `crates/pumpkin-core/src/entity/player.rs` attack path | `A-24`, then `A-26` and `A-27`. |
| `crates/pumpkin-core/src/command/commands/mod.rs` | Phase C tasks each add one register line. Rebase that line. The new command file is otherwise independent. |
| `crates/pumpkin-core/src/block/blocks/redstone/redstone_wire.rs` | `A-37`, then `B-21`. |

## First wave

These ten have no unfinished dependency and do not share a hot file. Hand them out together.

| ID | Target | Why it is in the first cut |
| --- | --- | --- |
| [A-04](A-04.md) | `core-functionality` | 26.3 worlds currently fail the version check. |
| [A-05](A-05.md) | `core-functionality` | Reloaded worlds forget time, weather, border, and scoreboard. |
| [A-09](A-09.md) | `core-functionality` | Configuration keep-alive kicks the client. |
| [A-10](A-10.md) | `core-functionality` | Client information is missing particle status. |
| [A-01](A-01.md) | `core-functionality` | Later harness scenarios need a registry. |
| [A-08](A-08.md) | `core-functionality` | Chunk field notes, docs only, unblocks `A-48`. |
| [A-36](A-36.md) | `core-functionality` | Redstone fixture, test only, unblocks the dust and piston lanes. |
| [A-24](A-24.md) | `core-functionality` | Melee damage, unblocks bow, armor, zombie. |
| [B-01](B-01.md) | `core-functionality` | MSPT percentiles, so later perf work has a number. |
| [A-13](A-13.md) | `core-functionality` | Strip, path, and till. |

Do not add `B-03` to this wave. It edits `world/mod.rs` beside `A-05`. Do not add `A-07` or `A-48` beside `A-04`. Do not add `A-15` beside `A-24` if both would edit `living.rs`.

## Second wave

Start these once the first wave is actually running on disjoint files. They also have no hard dependency, or they only need a first-wave task that is not their file neighbour.

[A-06](A-06.md), [A-11](A-11.md), [A-14](A-14.md), [A-16](A-16.md), [A-17](A-17.md), [A-19](A-19.md), [A-20](A-20.md), [A-22](A-22.md), [A-23](A-23.md), [A-30](A-30.md), [A-31](A-31.md), [A-33](A-33.md), [A-34](A-34.md), [A-35](A-35.md), [A-42](A-42.md), [A-45](A-45.md), [B-07](B-07.md), [B-08](B-08.md), [B-12](B-12.md), [B-15](B-15.md), [B-16](B-16.md), [B-24](B-24.md), [B-25](B-25.md), [B-26](B-26.md), [C-01](C-01.md), [D-01](D-01.md), [D-05](D-05.md), [D-09](D-09.md)

`C-01` is the pattern for `C-02` through `C-09`: they can run together if each person rebases the register line. `C-10` waits on `A-27`. In this list, `B-16`, `B-25`, `C-01`, `D-01`, `D-05`, and `D-09` are cut from `enhanced-functionality`. The other ids are cut from `core-functionality`.

## Lanes

Tasks on `enhanced-functionality` that depend on a core task start after that core task is merged into enhanced. Core tasks do not depend on enhanced tasks.

### Phase A

- **A-harness:** `A-01` then `A-02` and `A-03`.
- **A-world:** `A-04` and `A-08` first. `A-48` after both. `A-07` after `A-04` leaves `anvil.rs`.
- **A-persist:** `A-05` and `A-06` in parallel. `A-27` after `A-06` and `A-16`.
- **A-protocol:** `A-09` then `A-12`. `A-10` and `A-11` beside `A-09`.
- **A-items:** `A-13`, `A-14`, `A-15`, `A-16`, `A-17`, `A-18`, `A-19`, `A-20`, `A-22`, `A-23` in parallel. `A-21` after `A-20`.
- **A-combat:** `A-24` then `A-25`, `A-26`, `A-28`. `A-29` after `A-25`. `A-27` after `A-06` and `A-16`.
- **A-mobs:** `A-30`, `A-31`, `A-33`, `A-34`, `A-35` in parallel. `A-32` after `A-20`.
- **A-redstone:** `A-36` then `A-37`, `A-38`, `A-39`, `A-41` in parallel. `A-40` after `A-39`.
- **A-packs:** `A-42` then `A-43` then `A-44`.
- **A-gen:** `A-45` then `A-46`. `A-47` after `A-04` and `A-45`.

### Phase B

- **B-profile:** `B-01` then `B-02` and `B-20`.
- **B-entities:** `B-03` after `A-05` leaves `world/mod.rs`, then `B-04` and `B-28`. `B-22` after `B-03`. `B-30` any time.
- **B-blocks:** `B-05` after `A-21`. `B-21` after `A-37`. `B-23` after `A-30`. `B-24` any time.
- **B-net:** `B-06` after `B-01`. `B-07`, `B-16`, `B-27` any time.
- **B-chunks:** `B-08` then `B-09` and `B-14`.
- **B-gen:** `B-10` after `A-45`, then `B-11`. `B-25` and `B-26` any time.
- **B-light:** `B-12` then `B-13`.
- **B-ai:** `B-15` then `B-29`.
- **B-regions:** `B-17` on core after `B-01` and `B-03`. `B-18` and `B-19` are on enhanced, after `B-17` has been merged into `enhanced-functionality`.

### Phase C

`C-01` through `C-09` are independent aside from the register line in `commands/mod.rs`. `C-10` depends on `A-27`. Every feature is unregistered or inert when its flag is off.

### Phase D

- **D-voice:** `D-01` then `D-02` then `D-03` then `D-04`.
- **D-map:** `D-05` then `D-06`. `D-07` and `D-08` after `D-06`.
- **D-audit:** `D-09` first. `D-10` then `D-11`. `D-12` after `D-09` and `D-10`. `D-13` after `D-12`.

## Phase A

| ID | Title | Size | Parity | Dependencies | Lane | Target |
| --- | --- | --- | --- | --- | --- | --- |
| [A-01](A-01.md) | Scenario registry for the compat harness | S | None | none | A-harness | `core-functionality` |
| [A-02](A-02.md) | Download a vanilla world fixture at test time | M | None | A-01 | A-harness | `core-functionality` |
| [A-03](A-03.md) | Harness helpers for console commands and windows | S | None | A-01 | A-harness | `core-functionality` |
| [A-04](A-04.md) | Accept world data version 5023 | M | Low | none | A-world | `core-functionality` |
| [A-05](A-05.md) | Restore time, weather, border, gamerules, and scoreboard | M | Low | none | A-persist | `core-functionality` |
| [A-06](A-06.md) | Round-trip player health, inventory, spawn, and gamemode | M | Low | none | A-persist | `core-functionality` |
| [A-07](A-07.md) | Decide Anvil compression id 127 | M | Low | none | A-world | `core-functionality` |
| [A-08](A-08.md) | Write the 4903 to 5023 chunk field notes | S | None | none | A-world | `core-functionality` |
| [A-09](A-09.md) | Answer configuration keep-alive | S | Low | none | A-protocol | `core-functionality` |
| [A-10](A-10.md) | Read and write particle status on client information | S | Low | none | A-protocol | `core-functionality` |
| [A-11](A-11.md) | Resend registries on reconfiguration | M | Medium | none | A-protocol | `core-functionality` |
| [A-12](A-12.md) | Accept configuration custom click | S | Low | A-09 | A-protocol | `core-functionality` |
| [A-13](A-13.md) | Axe strip, shovel path, and hoe till | S | Low | none | A-items | `core-functionality` |
| [A-14](A-14.md) | Water and lava buckets | M | Medium | none | A-items | `core-functionality` |
| [A-15](A-15.md) | Eating restores hunger | M | Medium | none | A-items | `core-functionality` |
| [A-16](A-16.md) | Sleep in a bed and set the spawn | M | Medium | none | A-items | `core-functionality` |
| [A-17](A-17.md) | Furnace smelts one recipe | M | Medium | none | A-items | `core-functionality` |
| [A-18](A-18.md) | Crafting grid uses datapack recipes | M | Medium | none | A-items | `core-functionality` |
| [A-19](A-19.md) | Doors, trapdoors, and fence gates open | S | Low | none | A-items | `core-functionality` |
| [A-20](A-20.md) | Chest and barrel inventories persist | S | Low | none | A-items | `core-functionality` |
| [A-21](A-21.md) | Hopper moves one item | M | Medium | A-20 | A-items | `core-functionality` |
| [A-22](A-22.md) | Crop growth and bone meal | M | Medium | none | A-items | `core-functionality` |
| [A-23](A-23.md) | One brewing recipe | M | Medium | none | A-items | `core-functionality` |
| [A-24](A-24.md) | Melee damage and attack cooldown | M | High | none | A-combat | `core-functionality` |
| [A-25](A-25.md) | Bow fires an arrow | M | Medium | A-24 | A-combat | `core-functionality` |
| [A-26](A-26.md) | Armor points and shield block | M | High | A-24 | A-combat | `core-functionality` |
| [A-27](A-27.md) | Death drops and respawn | M | High | A-06, A-16 | A-combat | `core-functionality` |
| [A-28](A-28.md) | Zombie melee attack | M | Medium | A-24 | A-combat | `core-functionality` |
| [A-29](A-29.md) | Skeleton shoots | M | Medium | A-25 | A-combat | `core-functionality` |
| [A-30](A-30.md) | Creeper explosion | M | High | none | A-mobs | `core-functionality` |
| [A-31](A-31.md) | Animal breeding and one drop | M | Medium | none | A-mobs | `core-functionality` |
| [A-32](A-32.md) | Villager trade screen | M | Medium | A-20 | A-mobs | `core-functionality` |
| [A-33](A-33.md) | Phantom sweep attack | M | Medium | none | A-mobs | `core-functionality` |
| [A-34](A-34.md) | Breeze wind charge | M | Medium | none | A-mobs | `core-functionality` |
| [A-35](A-35.md) | Warden sonic boom | M | High | none | A-mobs | `core-functionality` |
| [A-36](A-36.md) | Redstone order fixture | M | None | none | A-redstone | `core-functionality` |
| [A-37](A-37.md) | Dust power cases | M | High | A-36 | A-redstone | `core-functionality` |
| [A-38](A-38.md) | Repeater delay and comparator measure | M | High | A-36 | A-redstone | `core-functionality` |
| [A-39](A-39.md) | Piston push and pull | M | High | A-36 | A-redstone | `core-functionality` |
| [A-40](A-40.md) | Sticky pistons and quasi-connectivity | M | High | A-39 | A-redstone | `core-functionality` |
| [A-41](A-41.md) | Observer and dispenser | M | Medium | A-36 | A-redstone | `core-functionality` |
| [A-42](A-42.md) | Predicate subset for loot and advancements | M | Medium | none | A-packs | `core-functionality` |
| [A-43](A-43.md) | Enter-block, location, and inventory advancement triggers | M | Medium | A-42 | A-packs | `core-functionality` |
| [A-44](A-44.md) | Story tab grants the opening advancements | M | Medium | A-43 | A-packs | `core-functionality` |
| [A-45](A-45.md) | Record the worldgen mismatch baseline | S | None | none | A-gen | `core-functionality` |
| [A-46](A-46.md) | Replace one NoOp configured-feature family | M | High | A-45 | A-gen | `core-functionality` |
| [A-47](A-47.md) | Generate the chunk beside an imported world | M | High | A-04, A-45 | A-gen | `core-functionality` |
| [A-48](A-48.md) | Apply the 5023 chunk field changes | M | High | A-04, A-08 | A-world | `core-functionality` |

## Phase B

| ID | Title | Size | Parity | Dependencies | Lane | Target |
| --- | --- | --- | --- | --- | --- | --- |
| [B-01](B-01.md) | Tick-phase timings and MSPT percentiles | S | None | none | B-profile | `core-functionality` |
| [B-02](B-02.md) | Profile command | M | None | B-01 | B-profile | `core-functionality` |
| [B-03](B-03.md) | Entity section spatial index | M | Low | none | B-entities | `core-functionality` |
| [B-04](B-04.md) | Player area map | M | None | B-03 | B-entities | `core-functionality` |
| [B-05](B-05.md) | Sleep idle hoppers | M | Low | A-21 | B-blocks | `enhanced-functionality` |
| [B-06](B-06.md) | Compare packet compression backends | S | None | B-01 | B-net | `core-functionality` |
| [B-07](B-07.md) | Reuse compressed chunk packets | S | None | none | B-net | `core-functionality` |
| [B-08](B-08.md) | Chunk I/O priorities and reads before writes | M | None | none | B-chunks | `core-functionality` |
| [B-09](B-09.md) | Per-player load caps and delayed unload | M | Low | B-08 | B-chunks | `enhanced-functionality` |
| [B-10](B-10.md) | Direct palette writes during noise fill | S | Low | A-45 | B-gen | `core-functionality` |
| [B-11](B-11.md) | Compile one density-function node | M | Low | B-10 | B-gen | `core-functionality` |
| [B-12](B-12.md) | Lighting queue direction flags | M | Low | none | B-light | `core-functionality` |
| [B-13](B-13.md) | Light non-overlapping chunks in parallel | M | Low | B-12 | B-light | `core-functionality` |
| [B-14](B-14.md) | Pregenerate command | M | None | B-08 | B-chunks | `enhanced-functionality` |
| [B-15](B-15.md) | Precomputed path node types | S | None | none | B-ai | `core-functionality` |
| [B-16](B-16.md) | Outbound packet priority | M | Low | none | B-net | `enhanced-functionality` |
| [B-17](B-17.md) | Sequential entity tick | M | Low | B-01, B-03 | B-regions | `core-functionality` |
| [B-18](B-18.md) | Region section ownership | M | Low | B-17 | B-regions | `enhanced-functionality` |
| [B-19](B-19.md) | Tick entities sequentially inside a region | L | Medium | B-18 | B-regions | `enhanced-functionality` |
| [B-20](B-20.md) | MSPT governor, off by default | S | High | B-01 | B-profile | `enhanced-functionality` |
| [B-21](B-21.md) | Alternate Current as an opt-in redstone engine | M | High | A-37 | B-blocks | `enhanced-functionality` |
| [B-22](B-22.md) | Entity activation range, opt-in | M | High | B-03 | B-entities | `enhanced-functionality` |
| [B-23](B-23.md) | Cache explosion ray checks | S | Low | A-30 | B-blocks | `core-functionality` |
| [B-24](B-24.md) | Pack scheduled ticks | S | None | none | B-blocks | `core-functionality` |
| [B-25](B-25.md) | Async locate | S | Low | none | B-gen | `enhanced-functionality` |
| [B-26](B-26.md) | Jigsaw piece spatial index | M | Low | none | B-gen | `core-functionality` |
| [B-27](B-27.md) | Anti-xray, opt-in | M | Medium | none | B-net | `enhanced-functionality` |
| [B-28](B-28.md) | Entity queries without a fresh Vec | S | None | B-03 | B-entities | `core-functionality` |
| [B-29](B-29.md) | POI index by section and type | S | None | B-15 | B-ai | `core-functionality` |
| [B-30](B-30.md) | Vanilla experience-orb merge, then an opt-in clump | S | High | none | B-entities | `enhanced-functionality` |

## Phase C

| ID | Title | Size | Parity | Dependencies | Lane | Target |
| --- | --- | --- | --- | --- | --- | --- |
| [C-01](C-01.md) | Sethome and home | S | High | none | C-commands | `enhanced-functionality` |
| [C-02](C-02.md) | Teleport request | S | High | none | C-commands | `enhanced-functionality` |
| [C-03](C-03.md) | Warps | S | High | none | C-commands | `enhanced-functionality` |
| [C-04](C-04.md) | Mute and slowmode | S | High | none | C-commands | `enhanced-functionality` |
| [C-05](C-05.md) | Join message and starter kit | S | High | none | C-commands | `enhanced-functionality` |
| [C-06](C-06.md) | Tab header and footer | S | Low | none | C-commands | `enhanced-functionality` |
| [C-07](C-07.md) | Nicknames | S | Medium | none | C-commands | `enhanced-functionality` |
| [C-08](C-08.md) | One claim per player | M | High | none | C-commands | `enhanced-functionality` |
| [C-09](C-09.md) | Operator vanish | M | High | none | C-commands | `enhanced-functionality` |
| [C-10](C-10.md) | Back to the last teleport or death | S | High | A-27 | C-commands | `enhanced-functionality` |

## Phase D

| ID | Title | Size | Parity | Dependencies | Lane | Target |
| --- | --- | --- | --- | --- | --- | --- |
| [D-01](D-01.md) | Voice protocol note | S | None | none | D-voice | `enhanced-functionality` |
| [D-02](D-02.md) | Voice plugin-channel handshake | M | Low | D-01 | D-voice | `enhanced-functionality` |
| [D-03](D-03.md) | Voice UDP socket | M | Low | D-02 | D-voice | `enhanced-functionality` |
| [D-04](D-04.md) | Proximity fanout and mute | M | Medium | D-03 | D-voice | `enhanced-functionality` |
| [D-05](D-05.md) | Top-down map tiles from loaded chunks | M | None | none | D-map | `enhanced-functionality` |
| [D-06](D-06.md) | Embedded map HTTP server | M | Low | D-05 | D-map | `enhanced-functionality` |
| [D-07](D-07.md) | Live player markers | S | Low | D-06 | D-map | `enhanced-functionality` |
| [D-08](D-08.md) | Map access control | S | Medium | D-06 | D-map | `enhanced-functionality` |
| [D-09](D-09.md) | Audit and replay privacy defaults | S | None | none | D-audit | `enhanced-functionality` |
| [D-10](D-10.md) | Block change log | M | Low | D-09 | D-audit | `enhanced-functionality` |
| [D-11](D-11.md) | Lookup and rollback command | M | High | D-10 | D-audit | `enhanced-functionality` |
| [D-12](D-12.md) | Session recording format | M | Medium | D-09, D-10 | D-audit | `enhanced-functionality` |
| [D-13](D-13.md) | Replay export | M | Medium | D-12 | D-audit | `enhanced-functionality` |
