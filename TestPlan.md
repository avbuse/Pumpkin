# Test plan: A-45 worldgen baseline

Branch `core/A-45-worldgen-baseline`. Confirm this branch before it is merged into `core-functionality` `f453f24a`.

Documentation only. No generator, chunk format, or packet change. Terrain matches `core-functionality` at `f453f24a`. `docs/compat/WORLDGEN_BASELINE.md` is the measurement taken on `70a0b846e`, before `b0c48b2d7` (`fix: many generation mismatches`) replaced the suite. Do not compare a new run to those old suite totals.

A higher count than test 2 is a regression even when the test still passes under its allowance (noise 6000, surface 7500, carvers 8000, per chunk).

## Setup

Run this branch from a scratch directory so the server writes `pumpkin.toml` and `world/` there:

```bash
mkdir -p /tmp/pumpkin-A-45 && cd /tmp/pumpkin-A-45
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
- `[world] autosave_ticks = 6000` (5 minutes). This branch does not change it.
- Player files are `world/players/data/<uuid>.dat` (hyphenated UUID, gzip NBT). That is not the vanilla `playerdata/` folder.

Each step has a checkbox. Record pass, fail, or not run, plus the evidence named on the step. A crash, a panic, or a log error that names the thing you just did is also a fail.

**Client** means a licensed Java Edition 26.3 client. **Console** means the server console only.


## Tests

- [ ] **1. No Rust diff** (Console)

  `git diff --stat core-functionality...HEAD -- crates/`
  Expect no output.
  **Fail:** any file under `crates/` differs.
  **Evidence:** the empty diff command.

- [ ] **2. Proto chunk counts** (Console)

  From the repository root, debug profile:

  ```bash
  cargo test -j 2 -p pumpkin-world --lib generation::proto_chunk_test -- --nocapture --test-threads=2
  ```

  This is a long quiet run. A run on this tree finished in 196.46 seconds on 2 threads. A silent few minutes is the test working.
  Expect `46 passed; 0 failed`. The old `43 passed` line, the 8x8 grid suite totals (noise 2, surface 2846, carvers 9122), and the feature totals (overworld 47135, nether 353478, end 0) belong to `70a0b846e`. Those grid tests are gone. `b0c48b2d7` replaced them with stage hashes, biome hashes, and structure-start checks.
  `count_dump_mismatches` still prints `[<name>] Total mismatches: N` only when `N > 0`. A passing test with no such line is 0. This run printed only:

  - `no_blend_no_beard_carvers_7_4`: 9
  - `no_blend_no_beard_surface_7_4`: 9
  - `no_blend_no_beard_surface_13579_-6_11`: 56
  - `no_blend_no_beard_surface_13579_-7_9`: 101

  **Fail:** any failed test, any of those four counts going up, or a new `Total mismatches` line.
  **Evidence:** the final `passed` line and every `Total mismatches` line.

- [ ] **3. Boot** (Console)

  Scratch directory, start, wait for `Started server; took`, `stop`.
  Expect no panic.
  **Fail:** a panic or an error this branch added. There should be no new error, because there is no code change.
  **Evidence:** the log from start through `stop`.

- [ ] **4. Walk around spawn** (Client)

  New world, seed 0 (`seed` in `pumpkin.toml` before the first boot, or delete `world/` and set it). Walk a few chunks around spawn.
  Expect the world to generate and the player to stand on solid ground. Expect no visible difference from `core-functionality`. This walk does not show whether terrain matches vanilla.
  **Fail:** a hole of air where the ground should be, or a crash while chunks generate.
  **Evidence:** optional. A clip of the player on generated ground is enough if you record one. No capture is required to merge a docs-only change.

## Where this branch sits

Suggested merge order into `core-functionality` `f453f24a`. A-16 is already on that core (`6b0d37545`), so it is not a remaining step:

1. `core/A-45-worldgen-baseline` (docs only)
2. `core/A-42-predicates` (datapacks, isolated from world save)
3. `core/A-48-chunk-5023` (chunk codec, before saves vanilla must read)
4. `core/A-20-container-persist` (autosave flush into the chunk A-48 writes)
5. `core/A-06-player-data` (writes the bed spawn, and the rest of the player file)
6. `core/A-24-melee` last, after Sweeping Edge deals more than 1, rebased onto A-06

Only A-06 and A-24 both change `crates/pumpkin-core/src/entity/player.rs`. A-48's chunk format file does not overlap the core commits after `50a0a2e6`.

This branch is step 1. `b0c48b2d7` already changed `proto_chunk_test`, and that change is on this branch. None of the other pending branches change those inputs. Re-run test 2 after a later merge only if that merge touches `crates/pumpkin-world/src/generation/`.
