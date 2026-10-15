# Test plan: A-45 worldgen baseline

Branch `core/A-45-worldgen-baseline`. Confirm this branch before it is merged into `core-functionality` `70a0b846e`.

Documentation only. No generator, chunk format, or packet change. Terrain matches `core-functionality` at `70a0b846e` and does not match vanilla. The fixture counts are the measurement. The numbers live in `docs/compat/WORLDGEN_BASELINE.md`.

A higher count is a regression even when the test still passes under its allowance (noise 6000, surface 7500, carvers 8000, per chunk).

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

  This is a long quiet run. The recorded run finished in 289.81 seconds. Review saw about 8 minutes on 2 threads. A silent few minutes is the test working.
  Expect `43 passed; 0 failed`. Expect suite totals noise 2, surface 2846, carvers 9122, and the same per-chunk cells as the baseline note. A missing `Total mismatches` line on a passing test means 0.
  Feature totals in the same run (not a pass/fail gate): overworld 47135 (worst chunk 1995), nether 353478 (worst 11567), end 0.
  **Fail:** any failed test, or any suite total above the note (noise above 2, surface above 2846, carvers above 9122).
  **Evidence:** the final `passed` line and the three `Total mismatches` suite lines.

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

Suggested merge order into `core-functionality` `70a0b846e`:

1. `core/A-45-worldgen-baseline` (docs only)
2. `core/A-42-predicates` (datapacks, isolated from world save)
3. `core/A-48-chunk-5023` (chunk codec, before saves vanilla must read)
4. `core/A-20-container-persist` (autosave flush into the chunk A-48 writes)
5. `core/A-16-bed-spawn` (in-memory bed spawn)
6. `core/A-06-player-data` (writes that spawn, and the rest of the player file)
7. `core/A-24-melee` last, after Sweeping Edge deals more than 1, rebased onto A-06

Only A-06 and A-24 both change `crates/pumpkin-core/src/entity/player.rs`. A-48's merge base is `50a0a2e6`, and its chunk format file does not overlap the later core commits.

This branch is step 1. None of the other pending branches change `proto_chunk_test` inputs. Re-run test 2 after a later merge only if that merge touches `crates/pumpkin-world/src/generation/`.
