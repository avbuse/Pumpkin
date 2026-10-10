# Server-side performance projects for Paper and Fabric: an implementer's reference for Pumpkin

Prepared for Jonathan. Research date: Saturday 10 October 2026, about 21:30 BST. The target is his fork **avbuse/Pumpkin**. The GitHub compare API says the fork is `identical` to `Pumpkin-MC/Pumpkin@master` (0 ahead, 0 behind). The Pumpkin checks below come from the local shallow clone at `987b979` (committed 2026-10-10 03:40 BST), stored at `/workspace/rust-mc-servers/src/Pumpkin`.

**Rules used in this document**
* **Numbers:** only figures stated by the project or another cited source. "No published figure" means I didn't find one.
* **Licences:** Paper and Folia patches are GPL-3.0. Lithium, Krypton, ScalableLux, Starlight, ModernFix and Noisium are LGPL-3.0. C2ME, VMP, FerriteCore, Alternate Current, Clumps and most of ServerCore are MIT. **Everything below describes techniques in my own words. Do not paste Java from these projects into Pumpkin.** Reimplement from the description and the vanilla behaviour. Pumpkin is GPL-3.0, so GPL/LGPL/MIT *ideas* are all fine to use.
* **Parity labels:** none = byte-for-byte or tick-for-tick vanilla. Low = vanilla in all practical cases. **Configurable** = changes behaviour but only when an admin enables it. **High** = observable gameplay change.
* **Pumpkin relevance tags:**
  * **MOOT:** Rust and Pumpkin's design already avoid the problem.
  * **PORT-IDEA:** worth reimplementing.
  * **HAVE:** Pumpkin already has an equivalent.
  * **PARTIAL:** Pumpkin has part of it.
  * **N/A:** doesn't apply to Pumpkin.

---

## 0. Project index (licence, popularity, status)

Modrinth downloads come from `api.modrinth.com/v2/project/<slug>` on 2026-10-10. GitHub data comes from `api.github.com/repos/...` on the same date.

| Project | Source | Licence | Modrinth downloads | Status |
|---|---|---|---|---|
| Paper | https://github.com/PaperMC/Paper | GPL-3.0 (contributors may opt into MIT; see LICENSE.md) | n/a | Active, 12.7k★ |
| Folia | https://github.com/PaperMC/Folia | GPL-3.0 | n/a | Active, 4.4k★ |
| Moonrise (Paper's chunk/entity core as a mod) | https://github.com/Tuinity/Moonrise | GPL-3.0-only | 239,570 | Active |
| Pufferfish | https://github.com/pufferfish-gg/Pufferfish | Patches GPL-3.0, other files MIT | n/a | Last push 2025-12-21 |
| Purpur | https://github.com/PurpurMC/Purpur | MIT (repo LICENSE) | n/a | Active. Mostly gameplay configurability |
| Leaf (Paper fork; async pathfinding/spawning/tracker) | https://github.com/Winds-Studio/Leaf | Mixed MIT/GPL-3.0 per patch; binaries GPL-3.0-only | n/a | Active |
| Lithium | https://github.com/CaffeineMC/lithium | LGPL-3.0-only | 135,139,114 | Active |
| C2ME | https://github.com/RelativityMC/C2ME-fabric | MIT (Modrinth) | 39,604,523 | Active |
| ScalableLux | https://github.com/RelativityMC/ScalableLux | LGPL-3.0-only | 15,089,594 | Active |
| Starlight (Fabric) | https://github.com/PaperMC/Starlight | LGPL-3.0-only | 16,116,147 | **Archived**. Lives on inside Paper/Moonrise |
| FerriteCore | https://github.com/malte0811/FerriteCore | MIT | 158,913,643 | Active |
| Krypton | https://github.com/astei/krypton | LGPL-3.0-only | 43,419,931 | Maintained "as-needed" |
| ModernFix | https://github.com/embeddedt/ModernFix | LGPL-3.0-only | 80,845,228 | Active |
| Chunky | https://github.com/pop4959/Chunky | GPL-3.0-only | 19,085,574 | Active |
| spark | https://github.com/lucko/spark | GPL-3.0-only | 23,371,210 | Active |
| Noisium | https://github.com/Steveplays28/noisium | LGPL-3.0-only | 24,703,932 | **Archived** |
| VMP (Very Many Players) | https://github.com/RelativityMC/VMP-fabric | MIT | 17,282,250 | Active, "early development" |
| Alternate Current | https://github.com/SpaceWalkerRS/alternate-current | MIT | 11,714,978 | Active. Also built into Paper as an option |
| Clumps | https://github.com/jaredlll08/Clumps | MIT | 40,732,344 | Active |
| ServerCore | https://github.com/Wesley1808/ServerCore | MIT, plus GPL-3.0 files marked in their headers | 17,884,205 | Active |
| *Other widely used server-side mods (Modrinth search, category `optimization`, server-side, sorted by downloads):* | | | | |
| Let Me Despawn | https://github.com/frikinjay/let-me-despawn | LGPL-3.0-only | 24,426,196 | Changes despawn rules for picked-up-item mobs (gameplay) |
| Structure Layout Optimizer | https://github.com/TelepathicGrunt/StructureLayoutOptimizer | MIT | 8,007,544 | Jigsaw structure generation |
| Async Locator | https://github.com/thebrightspark/AsyncLocator | MIT | 6,668,993 | `/locate`, treasure maps and eyes of ender off-thread |
| Entity View Distance | https://github.com/Patbox/EntityViewDistance | LGPL-3.0-only | 4,662,085 | Per-entity-type tracking distance |
| Memory Leak Fix / Debugify / Packet Fixer / LazyDFU / ThreadTweak / Ksyxis | (Modrinth) | various | 7–38 M each | JVM- or startup-specific. Mostly **N/A** for Rust (see §8) |

---

## 1. Chunks, world generation and I/O

### 1.1 Paper chunk system rewrite and async chunk I/O (now "Moonrise")
* **Sources:** Paper PR #8177 "Rewrite chunk system" (https://github.com/PaperMC/Paper/pull/8177), the older "Asynchronous chunk IO and loading" patch, the global config reference (https://docs.papermc.io/paper/reference/global-configuration/) and Moonrise (https://github.com/Tuinity/Moonrise). GPL-3.0.
* **Problem:** in vanilla, chunk scheduling and the chunk-holder state machine are tied to the main thread. Loads finish through a queue that only the main thread drains, and NBT reading, data fixing and deserialisation happen synchronously. Low TPS slows chunk loading and generation, and loading stalls TPS.
* **Mechanism (from PR #8177):**
  * **Any-thread scheduling.** The chunk system can be driven from any thread. Generation only returns to the tick thread for work that truly must run there (attaching entity/POI data).
  * **Two separate prioritised pools.** An **I/O pool** handles disk reads and writes. A **worker pool** handles deserialise, generate and light. Thread counts are set by `chunk-system.io-threads` and `worker-threads`. The worker default is half the *physical* cores, or 1 thread on machines with 3 or fewer cores.
  * **Coordinate-keyed priorities.** I/O tasks are keyed by world and chunk coordinate. Reads run before writes. A load already queued can be raised in priority, e.g. to URGENT when the tick thread is blocked waiting on it.
  * **Off-thread serialisation on unload.** Chunks being unloaded are serialised on workers. Live saves that aren't unloads stay synchronous for safety.
  * **Per-player rate limits.** Load, generate and send rates are capped per player (`player-max-chunk-load-rate: 100`, `player-max-chunk-send-rate: 75`, concurrent-load and concurrent-generate caps).
  * **Unload tuning.** Unloads are delayed (`delay-chunk-unloads-by: 10s`). Each tick unloads at least `max(50, 5% of eligible)` chunks. Autosave is capped at 24 chunks per tick.
* **Measured impact:** C2ME's own benchmark (§1.2) puts Paper at about 170 chunks/s at most, plateauing near 20 threads. That's a third-party figure.
* **Parity:** none intended. Delayed unloads and rate limits change timing only.
* **Pumpkin:** **HAVE (mostly).** Pumpkin already has a scheduler and chunk-state DAG (`pumpkin-world/src/chunk_system/{schedule,dag,chunk_state,generation}.rs`), a dedicated rayon generation pool, and blocking chunk I/O moved to `spawn_blocking`. **PORT-IDEA:**
  * explicit **priorities** (player-distance-based, and urgent when tick-blocking) on both I/O and generation queues;
  * **reads before writes** in I/O;
  * a **delayed unload** with hysteresis;
  * **per-player load and generate caps**, which bound how much one fast-flying player can monopolise workers.

### 1.2 C2ME (Concurrent Chunk Management Engine)
* **Source:** https://github.com/RelativityMC/C2ME-fabric. MIT.
* **Problem:** vanilla chunk generation, I/O and loading don't scale with cores.
* **Mechanism:** the module names in the repo give the techniques:
  * `c2me-rewrites-chunk-system` and `c2me-opts-scheduling`: a new scheduler.
  * `c2me-rewrites-chunkio` and `c2me-opts-chunkio`: async region-file I/O.
  * `c2me-rewrites-chunk-serializer`: a faster NBT chunk serialiser.
  * `c2me-opts-worldgen-general`, `-vanilla` and `-biome-cache`: worldgen hot-path optimisations.
  * `c2me-opts-dfc`: density-function compilation.
  * `c2me-opts-math` and `-natives-math`: vectorised and native maths for noise. The build requires Clang and CMake, so native code is involved.
  * `c2me-opts-allocs`: fewer allocations.
  * `c2me-threading-lighting`: parallel lighting.
  * `c2me-fixes-*-threading-issues`: fixes for vanilla's thread-unsafe spots.
  * `c2me-notickvd`: a view distance that doesn't tick.
  * `c2me-opts-accel-opencl`: experimental GPU worldgen.

  It also includes a `CheckedThreadLocalRandom` that detects cross-thread misuse of world random.
* **Measured impact (Modrinth page, benchmark dated 14 Aug 2026; MC 26.2 with the Terratonic + Terralith worldgen datapacks; Intel Xeon Platinum 8558P at 3.2 GHz):**
  * about 1,400 chunks/s at 96 threads, "up to ~34x" vanilla and "up to ~8.2x" Paper on a 48-core system;
  * scales linearly to 458 cps at 16 threads, against 142 for Paper and 41 for vanilla;
  * single thread: 29.1 vs 13.2 cps for vanilla.

  Source: https://modrinth.com/mod/c2me-fabric (chart alt-text). These are the project's own numbers.
* **Parity:** none by default (README). Vanilla worldgen itself isn't deterministic run-to-run (MC-55596), so seeds can't be compared bit-for-bit anyway.
* **Pumpkin:** **PORT-IDEA (high value).** The most transferable ideas:
  1. **Compile the density-function graph** into a flattened, specialised evaluator instead of walking a tree of virtual calls. In Rust that means an enum-based IR plus constant folding, or closures/monomorphised generics. Evaluate noise in **SIMD batches** across a cell column. Pumpkin's worldgen is still being completed, so build this in early.
  2. Cache biome lookups per chunk and per generation thread.
  3. Lighting that runs in parallel with generation (§2).
  4. The "UB sanitiser" lesson: give each chunk-generation task its own deterministic RNG stream, and assert that no shared world RNG is used off-thread. Rust can enforce this with types: no `Sync` world RNG.
* **Relevant comparison:** SteelMC, also Rust (see report.md), published 2,581.8 chunks/s on a 9950X, 18.8× vanilla-Fabric. So a Rust server can match C2ME-class throughput.

### 1.3 Noisium
* **Source:** https://github.com/Steveplays28/noisium. LGPL-3.0. Archived.
* **Mechanism:** while filling noise into a new chunk, it writes block states **directly into the section's palette storage**, skipping the general `setBlockState` path (heightmap, lighting and neighbour bookkeeping that is pointless during generation). Three smaller changes speed up biome population, block-state sampling and chunk unlocking.
* **Measured:** "a few seconds to a few dozen seconds faster depending on the amount of chunks". Spark profiles are in the repo. No normalised number.
* **Parity:** none ("full 1:1 parity", README).
* **Pumpkin:** **PORT-IDEA (cheap).** Provide a `ProtoChunk` bulk-fill API that writes palette indices directly and recomputes heightmaps once per section at the end. Check that Pumpkin's noise-fill stage doesn't go through the per-block update path.

### 1.4 Structure Layout Optimizer and Async Locator
* **Structure Layout Optimizer** (MIT) speeds up jigsaw structure assembly: villages, bastions, trial chambers. It targets the piece-placement collision checks.
* **Async Locator** (MIT) moves `/locate`, explorer and treasure maps, eyes of ender and dolphin treasure searches off the tick thread, with the result delivered later.
* No published figures found for either.
* **Parity:**
  * Structure Layout Optimizer: none intended.
  * Async Locator: low. The result arrives some ticks later, so a map may briefly show as "loading".
* **Pumpkin:** **PORT-IDEA.**
  * Jigsaw: use a spatial index (an AABB tree, or a grid of piece bounding boxes) instead of linear overlap tests.
  * Locate: Pumpkin already has `/locate`. Run structure searches as async tasks on the worker pool and complete them on the tick. Tokio makes this natural.

### 1.5 Chunky (pregeneration)
* **Source:** https://github.com/pop4959/Chunky. GPL-3.0.
* **What it does:** a task-based pregenerator with shapes (square, circle and others) and patterns (spiral and others). Tasks can be paused and resumed (progress persisted), run concurrently across worlds, report ETA and rate, and `trim` chunks outside a selection. It works by **requesting chunk loads at a bounded concurrency** through the server's own chunk system, then letting the chunks unload.
* **Measured:** Chunky is a tool, not an optimiser. SteelMC uses Chunky on Fabric as its vanilla baseline (137.2 chunks/s; https://steelmc.dev/reference/benchmarks/). Folia's README recommends pregenerating so fewer chunk-system worker threads are needed.
* **Parity:** none.
* **Pumpkin:** **PORT-IDEA (cheap, high utility).** I found no pregen command in `crates/pumpkin-core/src/command/commands/`. A built-in `/pumpkin pregen` (or a plugin) gives admins a feature they expect. It also gives you a **repeatable benchmark harness**: the same test SteelMC publishes.

### 1.6 Region-file formats (context)
* Leaf ships the "Linear" region format to save disk space. Pumpkin already supports **Vanilla Anvil, Linear and its own "Pump"** formats (`pumpkin-world/src/chunk/format/`). **HAVE.**

---

## 2. Lighting

### 2.1 Starlight → Moonrise/Paper, and ScalableLux
* **Sources:**
  * Starlight: https://github.com/PaperMC/Starlight (archived).
  * Spottedleaf's "future of Starlight" gist: https://gist.github.com/Spottedleaf/6cc1acdd03a9b7ac34699bf5e8f1b85c.
  * ScalableLux: https://github.com/RelativityMC/ScalableLux.
  * All LGPL-3.0.
* **Problem:** the old vanilla light engine propagated *updates* through one queue backed by a linked hash set. It also kept light-section state separate from chunks, which caused missing-section bugs and complicated chunk unloads.
* **Mechanism (from the gist):**
  1. **Propagate levels, not updates.** Use two plain deques, an *increase* queue and a *decrease* queue.
  2. Each queued entry stores **whether its source is a special-shape block**, which halves block reads, and a **bitset of directions still worth propagating**, which skips the direction it came from.
  3. **Dedicated sky-light initialisation** from heightmaps, rather than going through generic section init.
  4. **Stateless design:** light data lives on the chunk object. There is no separate light-engine state, so the chunk system can unload chunks without coordinating with the light engine. Lighting can run **in parallel** for chunks whose neighbourhoods don't overlap. Spottedleaf says this is what lets the chunk system scale past about 10 worker threads.

  ScalableLux adds optional **parallel light updates** and bug fixes on top of Starlight.
* **Measured:**
  * The gist: Mojang adopted ideas 1 and 2 in 1.20, but "Vanilla is still **2x** slower" than Starlight.
  * ScalableLux README: Starlight is "100% faster than vanilla" (linking the same gist) and allows the chunk system "to scale beyond 24 threads".
  * Starlight's README graphs (LightBench, 1.20-rc1, Ryzen 9 7950X) are images only, with no numeric table.
* **Parity:** none. The results are vanilla light values.
* **Pumpkin:** **PARTIAL.** `pumpkin-world/src/lighting/engine.rs` already uses `VecDeque` increase and decrease queues, i.e. the post-1.20 vanilla/Starlight structure. **PORT-IDEA:**
  * add the per-entry **direction bitset** and the **"source is special-shape" flag**;
  * keep light arrays **owned by the chunk** with no global engine state;
  * schedule light work so that non-overlapping 3×3 chunk neighbourhoods light **in parallel** on the worker pool (ScalableLux-style).
  * Verify sky-light initialisation is heightmap-driven.

---

## 3. Entities and AI

### 3.1 Entity storage and spatial lookups (Moonrise/Paper "entity sections", Lithium `entity_class_groups` / `fast_retrieval` / `collisions.*`, VMP "area maps")
* **Sources:**
  * Moonrise README: "Entity movement/collisions/physics/tracking", "Block/Entity retrieval (which systems like pathfinding and entity AI use frequently)".
  * Lithium config: https://github.com/CaffeineMC/lithium/blob/HEAD/lithium-fabric-mixin-config.md.
  * VMP README.
* **Mechanism:**
  * Store entities in **per-chunk-section buckets**. Keep **per-type and per-class sub-lists** (e.g. "pushable", "item", "player") so a query like "all pushable entities in this AABB" touches only the relevant sections and lists.
  * Lithium:
    * `collections.entity_by_type`: hash maps per type.
    * `entity.collisions.unpushable_cramming`: a separate pushable list for cramming tests.
    * `entity.collisions.movement`: optimised block access and delayed entity access; for entities moving only downward, check the supporting block first.
    * `util.entity_movement_tracking`: subscribers are notified when entities of a given type move within certain sections. Hoppers use this to watch for items.
  * VMP: "area maps", i.e. a precomputed map from chunk to the players whose view or track range covers it, used for packet broadcast and player lookup.
* **Measured:** no isolated published figures. Moonrise says "measurably lower tick times".
* **Parity:** none.
* **Pumpkin:** **PORT-IDEA (top priority).** In the clone, `World::get_entities_at_box`, `get_nearby_entities`, `get_players_at_box` and `get_nearby_players` (`pumpkin-core/src/world/mod.rs`, around line 4381) **linearly scan every entity, or every player, in the world**. `get_entities_at_box` alone has 52 call sites across the crates (rg count). That makes collision, AI targeting and item pickup O(N) per query, so O(N²) per tick as entity counts grow. Implement:
  * an **entity section index** (chunk-section key → small vec of entity ids, updated on section crossing), with **per-category sub-lists**;
  * a **player area map** for broadcast and nearby-player queries.

  This is pure data structure, has no parity risk, and its benefit grows with load.

### 3.2 Entity Activation Range (Spigot → Paper "EAR 2.0"; ServerCore port)
* **Sources:** Paper's `spigot.yml` reference (https://docs.papermc.io/paper/reference/spigot-configuration/) and patch `0005-Entity-Activation-Range-2.0.patch`. GPL-3.0.
* **Mechanism:** each entity type belongs to an activation category. Defaults: animals 32, monsters 32, raiders 64, misc 16, water 16, villagers 32, flying-monsters 32 blocks. Each tick, entities inside any player's activation box are marked active. Inactive entities skip most of their tick, with exceptions:
  * **Immunity checks** run every 20 ticks and keep entities active if, for example, they're burning, in water, have a target, are newly spawned (under 10 s old), are in a portal, or are working or panicking villagers.
  * **Periodic wake-ups**, e.g. monsters every 400 ticks for 100 ticks, at most 8 per tick; animals every 1200 for 100, at most 4.
  * Some entities always tick, such as fireworks.
* **Measured:** no official figure. Community guides call it one of the biggest entity-lag levers.
* **Parity:** **HIGH, but configurable.** Distant mobs freeze, and farms depending on far-away mobs (iron/mob farms, villager breeders) can break (https://dylancode.dev/minecraft/spigot/entity-activation-range/).
* **Pumpkin:** **PORT-IDEA as opt-in only.** Default it to off ("vanilla mode"). Pumpkin's goal is vanilla compatibility, so ship it as a server-owner toggle with Paper-compatible config keys. The only match in the clone is a creaking-specific use. Pumpkin already skips entities in non-active chunks (simulation distance), which is vanilla behaviour.

### 3.3 Dynamic Activation of Brain (Pufferfish; also in Leaf)
* **Source:** https://docs.pufferfish.host/setup/pufferfish-fork-configuration/. GPL-3.0.
* **Mechanism:** throttle brain/goal-selector ticks by distance to the nearest player. Entities within `start-distance` (default 12 blocks) tick every tick. Beyond that, they tick every `freq = dist² / 2^activation-dist-mod` ticks (default mod 8), capped at `max-tick-freq` (default 20). There's a per-type blacklist.
* **Measured:** no published number.
* **Parity:** **HIGH, but configurable.** It changes AI reaction timing.
* **Pumpkin:** **PORT-IDEA as opt-in only.** Same treatment as EAR.

### 3.4 AI micro-optimisations (Lithium `ai.*`, Paper world config)
* **Lithium:**
  * `ai.pathing`: a faster lookup of which pathfinding node type a block is, plus a chunk cache for block reads during path evaluation.
  * `ai.poi`: faster POI search.
  * `ai.sensor.secondary_poi`: skip secondary POI search for non-farmer villagers.
  * `entity.inactive_navigations`: block updates skip notifying mobs that wouldn't react.
* **Paper options:**
  * `update-pathfinding-on-block-update` (on by default; disabling is a behaviour change);
  * `stuck-entity-poi-retry-delay: 200`;
  * `tick-rates.sensor.villager.secondarypoisensor: 40` (a behaviour change);
  * `zombies-target-turtle-eggs`.
* **Leaf** offers *async* pathfinding (README). Parity risk is high: paths complete a tick or more later.
* **Parity:**
  * Lithium's items: none.
  * Paper tick-rate knobs: configurable.
  * Async pathfinding: high.
* **Pumpkin:** **PORT-IDEA.**
  * **Precompute a per-blockstate `PathNodeType` table at build time.** Pumpkin already generates block data, so this is close to free and is the Rust equivalent of Lithium's lookup.
  * A per-path-search **local chunk cache**: borrow section references once and avoid going through `World` per block.
  * Index POI by section and type.
  * Only notify mobs whose current path crosses the updated block (a per-section listener set).

### 3.5 Clumps (XP orbs) and item merging
* **Source:** https://github.com/jaredlll08/Clumps. MIT.
* **Mechanism:** merges nearby XP orbs into one entity that carries a value map, and the player collects it immediately on touch.
* **Measured:** no published number.
* **Parity:** **medium.** Orb counts, visuals and pickup timing differ.
  * Vanilla already merges orbs of the same value since 1.17 (orb "count").
  * Paper has `experience-merge-max-value`.
* **Pumpkin:** **PORT-IDEA as opt-in.** First make sure vanilla's own orb-count merging is implemented exactly.

### 3.6 Entity tracking distance (Paper `tracking-range-y`; Entity View Distance mod)
* **Mechanism:** per-entity-category tracking ranges, optionally with a separate vertical range, so fewer entities are sent and updated per player.
* **Parity:** configurable. Defaults equal vanilla.
* **Pumpkin:** `world/entity_tracker.rs` exists. **PORT-IDEA:** make per-category horizontal and vertical ranges configurable, and use the §3.1 area map to find candidate viewers.

### 3.7 Explosions (Paper `optimize-explosions`, Lithium `world.explosions`)
* **Mechanism:**
  * Cache the per-entity exposure calculation (ray samples) and block-resistance lookups during one explosion.
  * In Paper, reuse entity lookups across chained TNT.
* **Parity:**
  * Lithium: none.
  * Paper: off by default, and documented as a cache, not a behaviour change.
* **Pumpkin:** **PORT-IDEA (cheap).** Matters for TNT-heavy servers.

---

## 4. Redstone and block ticking

### 4.1 Alternate Current (also Paper `redstone-implementation: ALTERNATE_CURRENT`)
* **Source:** https://github.com/SpaceWalkerRS/alternate-current. MIT. Paper's world config lists `VANILLA` (default), `EIGENCRAFT` and `ALTERNATE_CURRENT`, plus `alternate-current-update-order` (4 orders).
* **Problem (README):**
  * Vanilla wire updates recursively. Each wire computes its power alone, so in a grid it may change power "over half a dozen times" before settling.
  * Each power change emits **42 block updates and up to 22 shape updates**. 6 of those block updates hit itself, and 12 more are duplicates. Only 6 of the 22 shape updates are needed.
* **Mechanism:**
  1. Build the **connected wire network**.
  2. Find wires powered from *outside* it and spread power from those (BFS-style). Each wire checks non-wire power at most once and neighbouring wires at most twice, and **sets its block state once**.
  3. Emit block and shape updates **only at final power**, and **drop updates between wires inside the network**: up to 20% fewer.
  4. Order neighbour updates by **direction of power flow**, which is deterministic and non-locational. This fixes MC-11193.
* **Measured:** "MSPT contributions of redstone dust are up to 20 times lower" (README).
* **Parity:** **CHANGES BEHAVIOUR.** Update order differs from vanilla's locational order. Paper's docs say "Both the Eigencraft and Alternate Current implementations change the behavior of redstone dust". Some contraptions that depend on locational order behave differently.
* **Pumpkin:** **PORT-IDEA as a config option, default VANILLA.** Pumpkin's `redstone_wire.rs` (678 lines) has an "Evaluator Logic (matching DefaultRedstoneWireEvaluator)" section: a vanilla-style evaluator. Keep vanilla as the reference. Add `redstone.implementation = vanilla | alternate_current` behind a trait, as Paper does. Also consider Mojang's own **experimental redstone** (the `redstone_experiments` feature flag). Matching it exactly is a parity requirement in its own right.

### 4.2 Lithium redstone and block micro-optimisations
* `block.redstone_wire`: wire power calculation avoids duplicate block reads. **Parity: none.** This is a safe first step before AC.
* `world.tick_scheduler`: faster scheduled-tick collections, with scheduled ticks packed into integers for cheap comparisons.
* `block.flatten_states`: store "is empty" directly on fluid states.
* `alloc.chunk_random`: fewer block-position allocations in random ticking.
* `world.chunk_ticking`, `world.combined_heightmap_update`, `world.inline_block_access`, `world.inline_height`.
* **Pumpkin:** mostly **MOOT** in Rust: positions are `Copy` structs, so there's no allocation, and generated per-state property tables already flatten state data. Still **PORT-IDEA:**
  * a scheduled-tick queue keyed by `(tick, priority, sub-tick order)` packed into a `u64` in a radix or binary heap per chunk;
  * random ticks that skip sections whose palette contains no randomly-ticking state. Keep a per-section count of ticking states, as vanilla does with `tickingBlockCount`. Verify Pumpkin does this.

### 4.3 Hoppers and block entities
* **Lithium `block.hopper`:** caches adjacent inventories, has a notification system (inventory-change listeners, entity-movement subscriptions for item entities) and **block-entity sleeping**. `world.block_entity_ticking.sleeping` stops ticking a block entity until something wakes it: an inventory change or a block-state change.
* **Paper hopper options:**
  * `cooldown-when-full: true`;
  * `disable-move-event` (a Bukkit-event concern);
  * `ignore-occluding-blocks` (a behaviour change).
* **Pufferfish:** "30% faster hoppers over Paper (Airplane)" (Pufferfish README; no methodology given).
* **Parity:**
  * Lithium sleeping: none, because it's designed to wake on every state change that matters.
  * Paper `ignore-occluding-blocks`: configurable behaviour change.
* **Pumpkin:** **PORT-IDEA (high value for survival servers).**
  * Pumpkin keeps `block_entities: DashMap<ChunkPos, FxHashMap<BlockPos, Arc<dyn BlockEntity>>>` and ticks them each tick. The hopper has a vanilla-style `cooldown_time`.
  * Add a **sleep/wake** mechanism: a block entity returns `Sleep` from `tick()` and is moved out of the ticking set. It's woken by inventory mutation, by neighbour block or state change, or by an item entity entering its pickup AABB (via the §3.1 section index).
  * Also cache "inventory at the hopper's target" until a neighbour update invalidates it.

### 4.4 Lag-spike and abuse bounds (Paper)
* `max-block-ticks: 65536`, `max-fluid-ticks: 65536`, `max-auto-save-chunks-per-tick: 24`, `max-entity-collisions: 8`, and per-chunk entity save limits.
* **Parity:** the collision cap differs from vanilla's full processing. The tick caps only engage in pathological cases.
* **Pumpkin:** **PORT-IDEA.** Use safety caps with vanilla-equivalent defaults. Note that vanilla's `maxEntityCramming` is a gamerule, separate from Paper's collision cap.

---

## 5. Networking

### 5.1 Krypton (and Paper and Velocity's natives)
* **Source:** https://github.com/astei/krypton and the Modrinth page https://modrinth.com/mod/krypton. LGPL-3.0.
* **Mechanism (Modrinth text):**
  * "Highly optimized Netty handlers derived from the Velocity proxy", using native code "where it makes the most sense". That's Velocity's native **libdeflate**-based compressor. The LilyPad issue #22 confirms libdeflate does whole-buffer, non-streaming compression.
  * **Flush consolidation**: buffer writes and flush once per tick instead of once per packet, which saves syscalls.
  * Micro-optimisations in packet serialisation: a faster VarInt frame decoder that reads 4 bytes little-endian and decodes with bit operations, and a frame splitter that also blocks "nullping" attacks by skipping leading zero bytes.
* **Measured:** no published figures.
* **Parity:** none. It's wire-compatible.
* **Pumpkin:**
  * **Flush consolidation: HAVE.** `Server::tick()` calls `suspend_player_flushes()` and `resume_player_flushes()` around the tick, with the comment "Do not flush mid-tick".
  * **Compression: PORT-IDEA.** Packet compression uses `flate2` with `rust_backend` (miniz_oxide). Two candidates:
    1. **libdeflate** through the `libdeflater` crate, for one-shot whole-packet compression. That's the Velocity/Krypton approach, and it suits Minecraft because each packet is compressed independently.
    2. **zlib-rs**, via the flate2 `zlib-rs` feature.

    Benchmark both on chunk-data packets, the largest bytes on the wire. Also **compress shared payloads once**: Pumpkin already caches encoded chunk payloads (`EncodedChunk` in `net/chunk_sender.rs`). Make sure the *compressed* frame is cached too, so N players viewing a chunk cost one compression.
  * **VarInt decode:** Rust codegen is already tight, so this is low priority. Do check the frame decoder for the nullping-style zero-byte flood.

### 5.2 VMP (Very Many Players)
* **Source:** https://github.com/RelativityMC/VMP-fabric. MIT.
* **Mechanism (README):**
  * **Area maps** for nearby packet sending and player lookup.
  * **Caches** for entity trackers, fluid-state lookups, ingredient matching and biome lookup.
  * Natural-spawning optimisations.
  * Entity-iteration optimisations for collisions.
  * An optimised ticket propagator (based on Paper's MCUtil).
  * **Async initial chunk loading on login.**
  * **Its own chunk-sending mechanism**, with optional packet-level rate limiting and **per-player render distance at the packet level**.
  * **Packet priority** borrowed from raknetify, to make TCP more responsive.
  * Separate ("split") event loops to resist bot attacks.
  * Async logging.
* **Measured:** no published figures.
* **Parity:** none claimed. Several commands become async *only when issued by players*.
* **Pumpkin:**
  * Chunk sending: **HAVE.** `chunk_sender.rs` implements adaptive chunks-per-tick (min 0.1, initial 9, max 500 per tick; up to 10 concurrent batches), using vanilla's Chunk Batch Start/End and acknowledgement protocol and parallel encoding with rayon.
  * Per-player view distance: **verify**, via client settings → `min(client, server)`.
  * **PORT-IDEA:** **packet priority classes**. Chunk data is bulk, while keep-alive, movement corrections, block changes and chat are urgent. Use separate outbound queues per connection and drain urgent ones first. Also **isolate the login/handshake path** (rate limit per IP, cap concurrent pending logins) from established connections. Tokio makes the latter straightforward.

### 5.3 Anti-Xray (Paper; per-world)
* **Source:** https://docs.papermc.io/paper/anti-xray. GPL-3.0.
* **Mechanism:** chunk packets are **obfuscated before sending**. Real data is revealed in an `update-radius` (0–2) around blocks the player interacts with. There are three engine modes:
  * mode 1 replaces fully enclosed `hidden-blocks` with stone/deepslate, netherrack or end stone;
  * mode 2 replaces `hidden-blocks` and `replacement-blocks` with random hidden blocks;
  * mode 3 randomises per chunk layer.

  `max-block-height` defaults to 64. `lava-obscures` is optional, and a bypass permission is optional.
* **Measured:** "engine-mode: 3 can reduce network load when joining by a factor of ~2 and helps with chunk packet compression" (Paper docs).
* **Parity:** server state is unchanged. The *client view* is altered, so client-side x-ray and some client mods (minimaps of caves) see fake ores. Off by default in Paper.
* **Pumpkin:** **PORT-IDEA (opt-in).** I found no anti-xray in the clone (rg). It fits neatly in the chunk-encode step:
  * obfuscate a *copy* of the section palette during network encode;
  * keep the per-chunk "exposed to air" check in a precomputed bitmask;
  * make the cached encoded chunk per-world and per-mode, not per-player.

### 5.4 Packet Fixer (context)
* About 30 M downloads, MIT. It raises packet size limits and timeouts rather than improving performance. **N/A**, beyond making sure Pumpkin's limits match vanilla's.

---

## 6. Memory

### 6.1 FerriteCore
* **Source:** https://github.com/malte0811/FerriteCore, with details in `summary.md`. MIT.
* **Problem:** JVM object overhead in block-state tables, models and shapes.
* **Server-relevant items, with the project's measured savings** (measured on large modpacks; client-side items omitted):
  * **(2) BlockState neighbour tables:**
    * vanilla stores a `Table<Property, Value, State>` per state, costing about 600 MB;
    * FerriteCore replaces this with one "FastMap" per block, a multi-dimensional array indexed by mixed-radix arithmetic or bitmasks, costing about 7 MB total.
  * **(3) Property storage** via the same FastMap: about 170 MB.
  * **(7) Block-state shape-cache dedup:** identical collision shapes share one instance. About 200 MB.
  * **(9) Removing per-PalettedContainer `ThreadingDetector` objects:** about 10–15 MB, scaling with loaded chunks.
* **Parity:** none.
* **Pumpkin:** **largely MOOT.** Pumpkin generates block-state data as static tables at build time (`pumpkin-data`), so there are no per-state heap maps. Still **PORT-IDEA/verify:**
  * a state's "with property = value" lookup should be **arithmetic on a state-id offset** (mixed-radix), not a hash lookup;
  * collision and outline shapes should be **deduplicated into a shared static table**, with each state holding a `u16` index;
  * per-section palette storage should hold no per-section heap overhead beyond the data itself.

    Pumpkin's `PalettedContainer` / `HeterogeneousPaletteData` exist (`chunk/palette.rs`). Check that a homogeneous section (all air/stone) is a single value with **no array**, and that palette compaction happens on save (Lithium's `chunk.serialization`).

### 6.2 ModernFix
* **Source:** https://github.com/embeddedt/ModernFix. LGPL-3.0.
* **What it does:** launch time, world load time and memory: lazy model loading, faster datapack and registry reload, dynamic resources. These are mostly client- and JVM-startup fixes.
* **Parity:** none.
* **Pumpkin:** **mostly N/A or MOOT.** Native startup is already about 5 ms per pumpkinmc.org. The one transferable idea: **load datapack registries lazily and in parallel**, and **intern identifiers** (`Identifier` → u32). This matters once Pumpkin supports large datapacks.

### 6.3 Lithium `alloc.*`, Memory Leak Fix, and GC-oriented work (Pufferfish "Reduced GC times & frequency")
* These target allocation rate and GC pressure in the JVM.
* **Pumpkin:** **MOOT** as a category. There's no GC, and positions are values.
* The Rust-equivalent concern is **allocation churn in hot paths**: `Vec` returned from every query (e.g. `get_entities_at_box` returns a fresh `Vec<Arc<…>>`) and `Arc` clone/drop atomics. **PORT-IDEA:** callback or iterator query APIs, `SmallVec`, per-thread scratch buffers, and borrowing instead of `Arc::clone` in tight loops.
* Also consider **mimalloc or jemalloc** as the global allocator. I found none configured in the root Cargo.toml (rg). Benchmark before adopting either.

---

## 7. Threading and scheduling

### 7.1 Folia (regionised multithreading)
* **Sources:** https://github.com/PaperMC/Folia and https://docs.papermc.io/folia/reference/region-logic. GPL-3.0.
* **Mechanism:**
  * The world is grouped into **independent regions** of nearby loaded chunks. Each region runs its **whole tick loop** at 20 TPS on a thread pool. There is no main thread.
  * The regionizer works on **region sections**: N×N chunks, with N a power of two.
  * It maintains invariants:
    * every chunk holder belongs to exactly one region;
    * everything within a **merge radius** of a region's chunks is owned by that region;
    * a **ticking region cannot expand**, so nearby newly created sections become *transient* regions that merge in after the tick;
    * regions are in one of the states transient, ready, ticking or dead.
  * Regions **merge** when they come close and **split** after a tick, when dead-section thresholds allow.
  * Cross-region work is scheduled onto the owning region with Region, Entity, Global and Async schedulers. Bad cross-thread access is made to **fail hard**.
* **Measured:** no throughput figures in the README. Deployment guidance is based on a test server with about 330 players at peak. Example: netty I/O about 4 threads and chunk I/O about 3 threads per 200–300 players. Folia recommends at least 16 cores.
* **Parity:** **within a region, vanilla order holds; across regions, ordering is undefined.** The README says plugin compatibility should be expected to be zero.
* **Pumpkin:** **PORT-IDEA (strategic), and it doubles as a parity fix.**
  * Today Pumpkin ticks players with `par_iter` and **entities with `par_chunks(ENTITY_TICK_BATCH_SIZE)`** (`world/mod.rs` around lines 1576–1618). Entities therefore tick in parallel in arbitrary order inside one world.
  * That's fast, but it **diverges from vanilla's deterministic sequential entity order**, which matters for RNG consumption, collision resolution, item pickup races and redstone/entity contraptions.
  * Folia's model gives the same parallelism **with** deterministic, vanilla-like ordering inside each region.
  * Recommendation: move to **"regions tick in parallel, everything inside a region ticks sequentially in vanilla order"**. Use region sections as ownership units. Hand off cross-region effects as messages applied at the next tick. Rust's ownership model, with `&mut Region` given to one thread at a time, enforces Folia's "fail hard" rule at compile time.
  * Have a single-region mode that behaves exactly like vanilla, for parity testing.

### 7.2 Moonrise / Paper "responsiveness" items
* Handle packets that arrive while the server waits for the next tick. This lowers perceived latency.
* Reduce TPS catch-up by default.
* Use fewer worker threads on low-core machines.
* **Parity:** catch-up reduction changes vanilla's "run faster after lag" behaviour, so treat it as configurable.
* **Pumpkin:** **PORT-IDEA.** Process inbound packets during idle time between ticks. Tokio tasks already decode concurrently, so check *when* decoded actions are applied. Make catch-up configurable with vanilla as the default.

### 7.3 ServerCore (dynamic performance management)
* **Source:** https://github.com/Wesley1808/ServerCore. MIT with some GPL files.
* **Mechanism:**
  * **Dynamic performance checks** automatically lower or raise view distance, simulation distance, chunk-tick distance and mob caps based on MSPT.
  * A configurable EAR port.
  * **Villager "lobotomisation"**: villagers stuck in 1×1 spaces tick less often.
  * Breeding caps.
  * Per-group mob-cap and spawn-rate overrides.
  * Lag-spike fixes for chunk ticking, spawning, item frames and logins.
  * Defaults keep vanilla behaviour, and behaviour-changing features are opt-in (README).
* **Parity:** none by default. The opt-in features are high-impact.
* **Pumpkin:** **PORT-IDEA.** Expose an **MSPT-driven governor** with hysteresis, plus admin bounds on view and simulation distance. It's cheap to build, very effective on small VPSes, and off by default. `/tps`, `/tick` and per-tick timing already exist (`server/ticker.rs` tracks tick durations).

### 7.4 Leaf async work (context)
* Leaf advertises **async pathfinding, async mob spawning and an async entity tracker** (README).
* **Parity:** high for pathfinding and spawning, because results land later. The async tracker is lower risk.
* **Pumpkin:** in Rust, the safer analogue of the *tracker* idea is to compute tracking deltas in parallel after the entity tick and before network flush. That's a read-only phase, so it doesn't change vanilla order.

---

## 8. Profiling and tooling

### 8.1 spark
* **Source:** https://github.com/lucko/spark. GPL-3.0.
* **What it does:**
  * a sampling CPU profiler (async-profiler `AsyncGetCallTrace` + `perf_events` on Linux x86_64, or a ThreadMXBean sampler);
  * a call-tree web viewer;
  * "only record laggy ticks" mode;
  * heap summary and dump;
  * GC monitoring;
  * health metrics: TPS, min/avg/max tick durations, CPU, memory, disk;
  * per-tick threshold alerts.
* **Pumpkin:** **PORT-IDEA (high value for every other item here).** The Rust equivalents:
  * `tracing` spans are already a dependency. Add a `tracing-tracy` or `tracing-chrome` feature for dev builds.
  * Add a production **`/pumpkin profile`** command that samples with `pprof-rs`, or exports via `perf` and `samply`, for N seconds or only during ticks over X ms. Output a flamegraph or pprof file.
  * Add a **tick-phase breakdown**: entities, block entities, chunk tick, lighting, network flush, plugin time. `world/mod.rs` already takes `Instant` timings around the entity phase.
  * Expose MSPT percentiles (p50/p95/p99) via `/tps` and a Prometheus endpoint.
  * A heap-profile option via `dhat` or jemalloc stats.

### 8.2 Benchmark harnesses
* Pumpkin's **BotMark** (https://github.com/Pumpkin-MC/BotMark, MIT) simulates players.
* SteelMC's worldgen benchmark methodology (https://steelmc.dev/reference/benchmarks/) is reproducible: fixed seed, tmpfs, Chunky on Fabric as the baseline, CPU and RSS sampled every 250 ms.
* **Pumpkin:** adopt both as CI benchmarks so every optimisation in this document gets measured, not assumed.

### 8.3 JVM-only items (N/A for Pumpkin)
These have no Rust equivalent worth porting, except where noted:
* LazyDFU and Paper DataConverter. DataFixerUpper laziness does matter for *loading old worlds*: Pumpkin will need its own chunk-NBT upgrader for older Anvil worlds, and should make it a fast single-pass converter.
* ThreadTweak / Smooth Boot (thread priorities and pool sizing for JVM startup).
* Memory Leak Fix.
* Debugify.
* Faster Random (now discouraged by its own page).
* Ksyxis (skips spawn-chunk loading, a gameplay change).
* Pufferfish's Sentry integration: it's error reporting, which Rust can do with `sentry-rust` if wanted.

---

## 9. Prioritised recommendations for Pumpkin

The order weighs expected value against parity risk and cost. "Value" is qualitative. I've attached no numbers I couldn't source. Every behaviour-changing item must default to **vanilla**.

| # | Improvement | Inspired by | Expected value | Parity risk | Effort | Pumpkin today |
|---|---|---|---|---|---|---|
| 1 | **Profiling and benchmark baseline:** tick-phase timings, MSPT percentiles, `/pumpkin profile` (pprof/tracy), BotMark + pregen benchmarks in CI | spark, SteelMC/C2ME methods | Very high: tells you which of the items below actually matters | None | S–M | Partial (`/tps`, tick durations, `tracing`) |
| 2 | **Entity section spatial index + per-category lists + player area map.** Replace linear world scans in `get_entities_at_box` (52 call sites), `get_nearby_*` and `get_players_at_box` | Moonrise/Paper, Lithium, VMP | Very high under load: removes O(N²) entity behaviour | None | M | Missing (linear scans) |
| 3 | **Regionised ticking with sequential in-region order** (Folia model). Replace free-for-all `par_chunks` entity ticking | Folia | High for scale; **also fixes a current parity gap** (non-deterministic entity tick order) | Reduces risk vs today; single-region mode = vanilla | L | Divergent (parallel unordered entity ticks) |
| 4 | **Block-entity sleep/wake** (hoppers first), cached target inventories, item-entity subscriptions | Lithium `block.hopper`, `block_entity_ticking.sleeping` | High on survival servers with hopper chains | None if wake conditions are complete | M | Missing (all BEs ticked) |
| 5 | **Packet compression backend** (libdeflate one-shot or zlib-rs) + **cache compressed chunk frames** across viewers | Krypton / Velocity natives | Medium–high CPU saving on join and teleport bursts | None | S | Partial (flate2/miniz; encoded-chunk cache exists) |
| 6 | **Chunk system priorities:** distance and urgent priorities on I/O and gen queues, reads before writes, delayed unload with hysteresis, per-player load/gen caps | Paper chunk system / Moonrise | Medium–high (smoother exploration, fewer stalls) | None | M | Partial (scheduler + DAG + rayon pool exist) |
| 7 | **Worldgen hot path:** density-function compilation, SIMD noise, direct palette writes during noise fill, per-thread biome cache | C2ME, Noisium | High for generation-heavy servers | None, provided a parity suite (SteelMC-style chunk hashes) guards it | M–L | Unknown/partial (worldgen still maturing) |
| 8 | **Lighting:** direction bitsets and special-shape flags in queue entries, chunk-owned stateless light data, parallel lighting of non-overlapping neighbourhoods | Starlight / ScalableLux | Medium–high (removes a chunk-pipeline bottleneck) | None | M | Partial (increase/decrease deques present) |
| 9 | **Built-in pregenerator** (`/pumpkin pregen`, shapes, pause/resume, trim) | Chunky | Medium (admin utility + benchmark) | None | S | Missing |
| 10 | **AI micro-opts:** precomputed PathNodeType per state, local chunk cache for path search, POI index by section and type, targeted navigation invalidation | Lithium `ai.*` | Medium | None | S–M | Unknown |
| 11 | **Network QoS:** priority outbound queues (urgent vs bulk), login-path isolation and rate limits, nullping guard | VMP, Krypton | Medium (responsiveness, bot resilience) | None | S–M | Partial (batching, flush consolidation) |
| 12 | **MSPT governor** (dynamic view/sim distance within admin bounds) | ServerCore | Medium on small hosts | Configurable (off by default) | S | Missing |
| 13 | **Anti-Xray** modes 1–3 in the chunk encode path | Paper | Medium (popular admin feature; mode 3 ~2× smaller join payload per Paper docs) | Client view only; opt-in | M | Missing |
| 14 | **Redstone:** keep vanilla evaluator as default; add Lithium-style duplicate-read removal (no parity change); add **Alternate Current** as an opt-in implementation | Lithium, Alternate Current, Paper | Medium–high for redstone-heavy servers ("up to 20×" lower dust MSPT per AC README) | Duplicate-read removal: none. AC: **changes update order** (opt-in) | M | Vanilla-style evaluator |
| 15 | **Opt-in entity throttles:** EAR (Paper keys), DAB, villager lobotomisation, XP clumping | Spigot/Paper, Pufferfish, ServerCore, Clumps | High on big servers when enabled | **High** (farms and AI timing); opt-in only | M | Missing |
| 16 | **Allocation hygiene:** iterator/callback query APIs, SmallVec, fewer `Arc` clones in hot loops; trial mimalloc/jemalloc | Lithium `alloc.*`, Pufferfish GC work (Rust analogue) | Low–medium, cumulative | None | S | Unknown |
| 17 | **Explosion caching, scheduled-tick packing, empty-section random-tick skip, safety caps** | Lithium, Paper | Low–medium | None (caps only in pathological cases) | S | Unknown |
| 18 | **Async locate / structure search; jigsaw spatial index** | Async Locator, Structure Layout Optimizer | Low–medium (removes spikes) | Low (result latency) | S | Unknown |

**Suggested sequencing:**
1. Do #1 first.
2. Then #2, #4 and #5, which are cheap, safe and high-value.
3. Then #6–#8 in the chunk pipeline.
4. Do #3 as a planned architecture change, together with a vanilla-order test suite.
5. Leave the opt-in, behaviour-changing knobs (#12–#15) until the vanilla-default paths are solid.

**Coding-agent guardrails:**
* Reimplement from the descriptions here, not by translating GPL/LGPL Java.
* Every change needs a before/after measurement from #1.
* Every behaviour-changing change needs a config flag with a vanilla default and a parity test: Pumpkin `gametest`, redstone test worlds, or worldgen chunk hashes.

---

## 10. Sources
Paper: https://github.com/PaperMC/Paper · https://github.com/PaperMC/Paper/pull/8177 · https://docs.papermc.io/paper/reference/world-configuration · https://docs.papermc.io/paper/reference/global-configuration/ · https://docs.papermc.io/paper/reference/spigot-configuration/ · https://docs.papermc.io/paper/anti-xray
Folia: https://github.com/PaperMC/Folia · https://docs.papermc.io/folia/reference/region-logic
Moonrise: https://github.com/Tuinity/Moonrise · Starlight: https://github.com/PaperMC/Starlight · https://gist.github.com/Spottedleaf/6cc1acdd03a9b7ac34699bf5e8f1b85c · ScalableLux: https://github.com/RelativityMC/ScalableLux
Pufferfish: https://github.com/pufferfish-gg/Pufferfish · https://docs.pufferfish.host/setup/pufferfish-fork-configuration/ · Purpur: https://github.com/PurpurMC/Purpur · Leaf: https://github.com/Winds-Studio/Leaf
Lithium: https://github.com/CaffeineMC/lithium · https://github.com/CaffeineMC/lithium/blob/HEAD/lithium-fabric-mixin-config.md
C2ME: https://github.com/RelativityMC/C2ME-fabric · https://modrinth.com/mod/c2me-fabric · Noisium: https://github.com/Steveplays28/noisium
FerriteCore: https://github.com/malte0811/FerriteCore/blob/HEAD/summary.md · ModernFix: https://github.com/embeddedt/ModernFix
Krypton: https://github.com/astei/krypton · https://modrinth.com/mod/krypton · https://github.com/astei/krypton/issues/22
VMP: https://github.com/RelativityMC/VMP-fabric · Alternate Current: https://github.com/SpaceWalkerRS/alternate-current · Clumps: https://github.com/jaredlll08/Clumps · ServerCore: https://github.com/Wesley1808/ServerCore
Chunky: https://github.com/pop4959/Chunky · spark: https://github.com/lucko/spark
Others: https://modrinth.com/mod/lmd · https://modrinth.com/mod/structure-layout-optimizer · https://modrinth.com/mod/async-locator · https://modrinth.com/mod/entity-view-distance
Benchmarks: https://steelmc.dev/reference/benchmarks/ · https://github.com/Pumpkin-MC/BotMark
EAR explainer: https://dylancode.dev/minecraft/spigot/entity-activation-range/
Raw downloads/READMEs: `/workspace/rust-mc-servers/perf/`
