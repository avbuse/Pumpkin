# Roadmap

This fork should let a vanilla Java 26.3 client play normal survival on a world saved by the vanilla 26.3 server. After that, it should get faster without changing vanilla behaviour, grow the admin features people usually install as plugins, and add the three features called out at the end.

The evidence for what is missing is [docs/compat/GAP_ANALYSIS.md](docs/compat/GAP_ANALYSIS.md). The reason this tree is the base, and not a merge of other Rust servers, is [docs/research/landscape-survey.md](docs/research/landscape-survey.md). Performance work follows [docs/research/perf-mods.md](docs/research/perf-mods.md). The session-sized tasks and the parallel lanes are in [docs/tasks/README.md](docs/tasks/README.md).

Protocol target: Java 26.3, protocol 777. The 26.3 dedicated server writes data version **5023**. Pumpkin still treats 4903 as the newest world it will open.

## How agents should use this

- Phase A is the gate for "a vanilla client can play". Phases B, C, and D run in parallel only where a task file says they do not depend on unfinished A work and do not edit a file someone else owns.
- One task, one branch, one agent. Commit with `git commit -s`. Push to `avbuse/Pumpkin` only. Do not open a pull request. Do not push or comment on `Pumpkin-MC/Pumpkin`.
- Do not build a general DataFixer. Accept 5023, keep the current minimum of 4435, and change only the chunk fields that actually differ. [A-08](docs/tasks/A-08.md) writes that list. [A-48](docs/tasks/A-48.md) applies it.
- Performance and admin features that change behaviour ship **off**, with vanilla as the default.
- Do not commit Mojang jars, region files, or decompiled source. Do not paste Java into the repo. Class and method names are pointers. Prefer the minecraft.wiki protocol pages and the data generators. A local look at the 26.3 server jar is fine and stays on the machine that did the look.
- Reimplement performance ideas from `docs/research/perf-mods.md`. That note is already a paraphrase. Do not translate GPL or LGPL Java.

The compat harness is `tools/pumpkin-compat`. It is a headless 777 client. `pumpkin_smoke` logs into a Pumpkin process, receives the spawn chunk, and places and breaks a block. `vanilla_milestones` checks that Pumpkin and the vanilla server both produce login success, registry data, finish configuration, play login, a player position, and a chunk. It does not compare chunk bytes. Gameplay that needs a placed block layout belongs in `pumpkin-gametest`.

## Phase A. Survival on an existing 26.3 world

A vanilla 26.3 client joins a world that the vanilla server saved, plays a normal survival session, and finds the world still there after a restart. New chunks at the edge may still differ from vanilla. They must generate and save.

48 tasks. Lanes and dependencies: [docs/tasks/README.md](docs/tasks/README.md).

### A1. The world opens

Exit: a 26.3 `level.dat` with data version 5023 passes the version check, chunks in that world load, and Pumpkin writes 5023. Worlds from 4435 through 4903 still open. 4434 and 5024 do not. There is no general upgrader.

Harness: a unit test around the version check with synthetic NBT. The vanilla jar is not stored in git. [A-04](docs/tasks/A-04.md), [A-07](docs/tasks/A-07.md), [A-08](docs/tasks/A-08.md), [A-48](docs/tasks/A-48.md).

### A2. The save comes back

Exit: day time, weather, world border, gamerules, scoreboard objectives, and the player's health, inventory, spawn, and gamemode survive a stop and start. `LevelTime::load_from` is actually called.

Harness: console `time set`, stop, start, `time query`. A player NBT round-trip covers the inventory. [A-05](docs/tasks/A-05.md), [A-06](docs/tasks/A-06.md).

### A3. Configuration matches 26.3

Exit: a configuration keep-alive and a custom click do not kick. Client information reads and writes the particle-status varint, and the harness sends that field once. A client moved back into configuration receives registries again and can return to play.

Harness: `vanilla_milestones` stays green. New scenarios send keep-alive and custom click before finish configuration and still reach play. [A-09](docs/tasks/A-09.md), [A-10](docs/tasks/A-10.md), [A-11](docs/tasks/A-11.md), [A-12](docs/tasks/A-12.md).

### A4. The survival loop

Exit: a player can strip a log, make a path, till dirt, pick up and place water and lava, eat when hungry, sleep in a bed, smelt iron, craft a crafting table from planks, open a door, store an item in a chest and a barrel, move one item with a hopper, grow wheat with bone meal, and brew one awkward potion.

Harness: one scenario or gametest per task, using console `setblock` / `give` and the window helpers from [A-03](docs/tasks/A-03.md). [A-13](docs/tasks/A-13.md) through [A-23](docs/tasks/A-23.md).

### A5. Combat and the mobs a player actually meets

Exit: melee damage respects the attack cooldown, a bow fires an arrow, armor and a shield change the damage, death drops items and respawn uses the bed. A zombie hits, a skeleton shoots, a creeper breaks a known block inside its blast, one animal breeds, a villager completes one trade, a phantom sweeps, a breeze throws a wind charge, and a warden uses a sonic boom at the anger threshold.

Harness: gametest or a harness client with a summoned mob and a known health delta. [A-24](docs/tasks/A-24.md) through [A-35](docs/tasks/A-35.md).

### A6. Redstone order

Exit: the gametest fixture's dust line, repeater delay, comparator compare and subtract, piston push, sticky retract, quasi-connectivity, observer pulse, and dispenser bucket match the expected vanilla results. The vanilla wire evaluator stays the default.

Harness: `pumpkin-gametest`, not the login smoke test. [A-36](docs/tasks/A-36.md) through [A-41](docs/tasks/A-41.md).

### A7. Advancements and predicates

Exit: predicates used by the opening story advancements load. Enter-block, location, and inventory triggers fire. Those opening advancements grant and the client receives the progress packet.

Harness: one advancement packet after `give`. [A-42](docs/tasks/A-42.md), [A-43](docs/tasks/A-43.md), [A-44](docs/tasks/A-44.md).

### A8. One new chunk past the imported edge

Exit: walking to a chunk the vanilla world did not contain produces a chunk packet and a saved chunk. Mismatch counts against vanilla are recorded next to the baseline. They do not have to be zero. An empty chunk or a crash fails the milestone. One `NoOp` feature family is replaced so the baseline can move.

Harness: [A-45](docs/tasks/A-45.md) records today's `proto_chunk_test` counts. [A-46](docs/tasks/A-46.md) implements one feature. [A-47](docs/tasks/A-47.md) loads a cached vanilla world, after [A-02](docs/tasks/A-02.md), and requests the neighbour chunk. `vanilla_milestones` still does not compare chunk bytes.

## Phase B. Performance, vanilla by default

Measure first. Then remove the linear entity scans and the unordered entity tick. Behaviour-changing knobs stay off until a phase A fixture covers the system they touch.

30 tasks. The order inside the phase matches section 9 of `docs/research/perf-mods.md`: profile, spatial index, hopper sleep, compression, chunk priorities, worldgen and light, then regionised ticking, then the opt-in knobs.

### B1. A number for every later change

Exit: `/tps` prints MSPT p50, p95, and p99, and a tick-phase split. `/pumpkin profile` writes a sample file when an operator asks. Sampling is off at boot.

Harness: console `/tps` on the smoke server. A unit test of the percentile function. [B-01](docs/tasks/B-01.md), [B-02](docs/tasks/B-02.md).

### B2. Entity lookups match the linear scan

Exit: `get_entities_at_box` and the nearby-player lookups use a section index. A fixture returns the same set as the old scan. Callers keep their current results.

Harness: a comparison fixture, then `pumpkin_smoke`. [B-03](docs/tasks/B-03.md), [B-04](docs/tasks/B-04.md), [B-28](docs/tasks/B-28.md). `B-03` waits until `A-05` is done with `world/mod.rs`.

### B3. Survival tick cost drops without changing results

Exit: an idle hopper does not scan inventories every tick and wakes when an item appears. A compressed chunk packet is reused for a second viewer until the chunk changes. Chunk reads run before writes of the same chunk, with a per-player cap that still lets view distance 2 finish smoke. Compression backend changes land only when a benchmark shows the bytes match and the CPU drops.

Harness: the A-21 hopper gametest, `pumpkin_smoke`, and a before/after line from B1. [B-05](docs/tasks/B-05.md), [B-06](docs/tasks/B-06.md), [B-07](docs/tasks/B-07.md), [B-08](docs/tasks/B-08.md), [B-09](docs/tasks/B-09.md).

### B4. Generation and light stay inside the baseline

Exit: direct palette writes, one compiled density-function node, lighting direction flags, and parallel lighting of non-overlapping chunks produce the same blocks and light values as before. `/pumpkin pregen` can fill a radius. `/locate` returns the same position from a worker thread. Jigsaw overlap uses a spatial index and picks the same pieces.

Harness: `proto_chunk_test` counts stay within [A-45](docs/tasks/A-45.md). [B-10](docs/tasks/B-10.md) through [B-15](docs/tasks/B-15.md), [B-25](docs/tasks/B-25.md), [B-26](docs/tasks/B-26.md), [B-29](docs/tasks/B-29.md).

### B5. One region ticks in vanilla order

Exit: the default config ticks every entity once per server tick, in a stable order, with no two entity ticks running in the same region at once. A multi-region map exists and is not the default. The design is [docs/compat/REGION_TICKING.md](docs/compat/REGION_TICKING.md), written by [B-17](docs/tasks/B-17.md).

Harness: an order test plus the phase A combat and redstone gametests. MSPT percentiles are recorded. [B-17](docs/tasks/B-17.md), [B-18](docs/tasks/B-18.md), [B-19](docs/tasks/B-19.md). `B-19` is the only large task.

### B6. Opt-in knobs

Exit: with defaults, view distance does not move, dust uses the vanilla evaluator, every mob in simulation distance still ticks, ores in the chunk packet are real, and experience orbs follow the vanilla merge. Turning a flag on is what enables the MSPT governor, Alternate Current, activation range, anti-xray, or aggressive orb clumping.

Harness: the phase A fixture for that system passes with the flag off. A second test forces the flag on. [B-20](docs/tasks/B-20.md), [B-21](docs/tasks/B-21.md), [B-22](docs/tasks/B-22.md), [B-27](docs/tasks/B-27.md), [B-30](docs/tasks/B-30.md). Packet priority, explosion-ray caching, and scheduled-tick packing are behaviour-neutral and sit in [B-16](docs/tasks/B-16.md), [B-23](docs/tasks/B-23.md), and [B-24](docs/tasks/B-24.md).

## Phase C. Features server owners usually add

Native commands for homes, teleport requests, warps, mute, a starter kit, tab header, nicknames, a single claim, vanish, and `/back`. Each one is unregistered or inert when its flag is off, so a default config still looks like vanilla. Search `crates/pumpkin-core/src/command/commands/` before adding a second command for the same job.

10 tasks, [C-01](docs/tasks/C-01.md) through [C-10](docs/tasks/C-10.md). They can run in parallel. Each new file is independent. The one-line register in `commands/mod.rs` is rebased.

### C1. Defaults unchanged

Exit: `vanilla_milestones` and `pumpkin_smoke` pass with every phase C flag off. No new command appears in `/help`.

### C2. Each flag does one job

Exit: with the flag on, the task's acceptance criteria hold across a restart where the feature stores data. Claims reject a second player's break inside the cuboid and allow it outside. Vanish hides the operator from the other client's spawn packets.

Harness: a compat scenario per command. Two clients for teleport requests and vanish.

## Phase D. Voice, web map, audit, and replay

These are new. They are off unless configured. A vanilla client that never installs a mod and never opens a browser plays the phase A game.

13 tasks.

### D1. Proximity voice

Simple Voice Chat (henkelmax) requires a client mod. Setup goes through a plugin message that carries a secret and a UDP host and port. Audio is a separate UDP socket, port 24454 by default, Opus frames, AES as the project documents it. Plasmo Voice also requires a client mod and a UDP socket. Its default is UDP on the same port number as the Minecraft TCP port, which does not conflict with TCP. Players without the mod still join and play. The server must not decode Opus on the game tick.

Exit: voice disabled changes nothing a vanilla client sees, and `pumpkin_smoke` passes. Voice enabled completes the setup channel, accepts a UDP packet only when the secret matches, and forwards audio to players inside range and not to players outside it. Audio is not written to disk. The first implementation follows the Simple Voice Chat split so an existing client mod is the target, and [D-01](docs/tasks/D-01.md) records whatever of that layout is not public enough to implement yet.

Harness: a custom-payload scenario for the handshake, and a unit test with fake UDP peers for range and mute. No microphone. [D-01](docs/tasks/D-01.md) through [D-04](docs/tasks/D-04.md).

### D2. A map in a browser

Dynmap, squaremap, and BlueMap all render tiles from chunks and show player markers on a web page. BlueMap's integrated server is a separate port (8100 in its docs) and can sit behind a reverse proxy. Its live markers are a push or a proxied live endpoint. squaremap is the closer first slice: a top-down tile, not a 3D mesh.

Exit: a loaded chunk becomes a tile image rendered off the game tick. An HTTP server bound to localhost, off by default, serves the tile and a page. A poll lists online players' dimension and block position and omits inventory and chat. A request without the configured token is rejected.

Harness: render a synthetic chunk, GET the tile, GET the player JSON after a harness login. [D-05](docs/tasks/D-05.md) through [D-08](docs/tasks/D-08.md).

### D3. Audit log and session replay

CoreProtect-style logging records block place and break, then lookup and rollback replay those rows. It is not a world backup. A rollback cannot repair a corrupt region file. Server-side replay projects such as ServerReplay record a session on the server and export something a ReplayMod (`.mcpr`) or Flashback client can open later. Their careful defaults omit voice, can omit chat, and do not offer the file for download unless asked.

Exit: defaults create no log and no recording. With the block log on, a place and a break are rows, and a rollback puts the previous block back and logs itself. A session recording stores positions and those block changes, omits chat, containers, and voice, and stops at a size cap. Export and a test playback rewrite the recorded block. The map server does not serve the file.

Harness: harness place, dig, rollback, and a unit test of the recording folder. [D-09](docs/tasks/D-09.md) through [D-13](docs/tasks/D-13.md). Privacy defaults land before any writer.

## What this roadmap leaves out

- Fabric, Forge, and NeoForge server mods. The landscape survey explains why a Rust server cannot load them. Client mods that only talk to the vanilla protocol already work. Voice is the deliberate exception, and it needs its own mod.
- Merging SteelMC, Valence, FerrumC, or Temper into this tree.
- A DataFixer that upgrades every historical version. Worlds older than data version 4435 stay rejected.
- Bedrock parity, and protocol translation for Java clients older than 26.3.
- Bukkit. PatchBukkit is a separate repository.

## Task counts

| Phase | Tasks | Role |
| --- | --- | --- |
| A | 48 | Vanilla survival on an existing 26.3 world |
| B | 30 | Speed, measured, vanilla by default |
| C | 10 | Opt-in admin commands |
| D | 13 | Voice, web map, audit, replay |
| Total | 101 | |
