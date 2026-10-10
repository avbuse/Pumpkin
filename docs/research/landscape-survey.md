# Rust Minecraft Java Edition servers: survey, merge feasibility and a plan

Prepared for Jonathan. Research date: Saturday 10 October 2026. Repo stats come from the GitHub REST API (`api.github.com/repos/...`) and shallow clones made the same evening. Star counts and dates are as of then. Dates are calendar dates, UTC.
Every number below comes from a cited source or the repo itself. Where I couldn't verify something, I say so.

---

## 0. TL;DR

* The projects don't all try the same thing. **Pumpkin**, **SteelMC**, **FerrumC/Temper** and **Oxide** want to be vanilla-like servers. **Valence** is a framework or engine with no vanilla gameplay by default. **MCHPRS** is a redstone-only creative server. **PicoLimbo** is a limbo/AFK server. **Azalea** is client and bot libraries. **Feather** is inactive and points people to Valence.
* **The licences rule out a "merge them all" repo, or force it to be GPL/AGPL.** Pumpkin is GPL-3.0, Temper and Oxide are GPL-3.0, and SteelMC is AGPL-3.0-or-later. Valence, FerrumC, Azalea, MCHPRS and PicoLimbo are MIT, and Feather is Apache-2.0. Permissive code can flow into a GPL project. GPL code cannot flow into a permissive one.
* **A code merge is also a poor fit technically.** The cores are built in incompatible ways. Pumpkin is Arc/OOP on tokio plus rayon, with no ECS. Valence, FerrumC and Temper use Bevy ECS, on different Bevy versions (0.14, 0.18 and 0.19). SteelMC is a custom design with a synchronous tick. Feather uses a hecs fork. They also target different protocol versions: 1.16.5, 1.20.1, 1.20.4, 1.21.8, 26.2 and 26.3.
* **Recommendation: don't merge.** Pick one base and contribute to it, or fork it. Take only specific permissive crates (NBT, protocol codegen, redstone compiler ideas) and only where their licence allows. The best base for "fully compatible vanilla server" is **Pumpkin**. It has by far the largest community (12,266★), commits daily, targets the latest version (26.3), has the broadest feature coverage, and already has a WASM plus native plugin API and a JVM bridge for Bukkit plugins (PatchBukkit). The catch is that you take on GPL-3.0. **SteelMC** is the parity-first alternative: block-for-block worldgen parity tests and published benchmarks, but AGPL-3.0-or-later and much less gameplay. If you need MIT, start from **Valence's or Azalea's crates** and accept a much longer road.
* **There is no official spec.** The de-facto spec is minecraft.wiki's protocol docs (the old wiki.vg), the server jar's data generator, and the vanilla server code itself. Since 26.1, Mojang ships that code **unobfuscated**, so the old mappings step is gone, but the EULA still applies.
* **"Works with all mods" isn't achievable in a Rust server.** Client-only mods mostly work already. Fabric, Forge and NeoForge server-side mods are Java code linked against Mojang's server internals. Getting them to run means reimplementing a large part of the vanilla server's Java API surface inside an embedded JVM. Realistic goals: (a) negotiate protocols correctly so modded *clients* can join, (b) a Paper/Bukkit-style plugin API (Pumpkin's PatchBukkit already does this through an embedded JVM), (c) support for specific popular mods' network channels, and (d) a proxy setup with real Fabric/NeoForge backends for modded worlds.

---

## 1. Comparison table

| Project | Licence (SPDX) | ★ | Last commit (default branch) | Target MC (protocol) | Core architecture | Status |
|---|---|---|---|---|---|---|
| [Pumpkin-MC/Pumpkin](https://github.com/Pumpkin-MC/Pumpkin) | GPL-3.0 (bare id in Cargo.toml; -only vs -or-later not stated). Plugin API crates MIT OR Apache-2.0 | 12,266 | 2026-10-10 | Java **26.3** (`CURRENT_MC_VERSION` = `LOWEST_SUPPORTED` = 26.3) + Bedrock 1.26.51 | No ECS. `Arc<World>`/`Arc<Player>` object model in the style of vanilla classes. tokio + rayon (players ticked with `par_iter`, chunk gen on a rayon pool). Plugins: wasmtime component model (WIT) + native `libloading` | Very active. Prerelease builds only (`nightly`, `canary`). 1.0 tracking issue #449 open |
| [Steel-Foundation/SteelMC](https://github.com/Steel-Foundation/SteelMC) | AGPL-3.0-or-later | 757 | 2026-10-10 | **26.2** | Custom, no ECS. "Gameplay updates remain synchronous, while chunk generation, lighting, packet processing, and chunk sending can run outside the main tick". tokio + rayon | Very active. v0.15.4+mc26.2 (2026-10-03). Pre-alpha |
| [ferrumc-rs/ferrumc](https://github.com/ferrumc-rs/ferrumc) | MIT | 2,421 | 2026-08-14 (master). `rework/v2-skeleton`-line branch 2026-07-18 | **1.21.8** (772) | Bevy ECS 0.18 (multi_threaded) + tokio + rayon. LMDB (`heed`) world store | Active but in transition: a "fresh rewrite" from scratch on `rework/v2-skeleton`. Release v0.1.0-rc2 (2026-02-16) |
| [temper-mc/temper](https://github.com/temper-mc/temper) | GPL-3.0 | 95 | pushed 2026-10-10 | **26.2** (776) | Fork of FerrumC: Bevy ECS 0.19 + tokio + rayon + LMDB | Active. v0.2.0 (2026-10-02) |
| [valence-rs/valence](https://github.com/valence-rs/valence) | MIT (logo CC BY-NC-ND 4.0) | 3,281 | 2026-06-15 | **1.20.1** (763) | Bevy ECS/App 0.14 + tokio + rayon. A *framework*, not a server | Slow: commits Jan and Jun 2026, only release v0.1.0 (2022). Not archived |
| [feather-rs/feather](https://github.com/feather-rs/feather) | Apache-2.0 | 2,702 | 2024-02-20 | **1.16.5** (754) | hecs (fork) ECS + tokio. WASM plugins ("Quill") | **Inactive** by its own README ("Consider contributing to valence instead"). Not archived |
| [azalea-rs/azalea](https://github.com/azalea-rs/azalea) | MIT | 788 | 2026-10-06 | **26.2** (776) | Bevy ECS/App 0.19 + tokio. Client/bot libraries | Active (single main maintainer) |
| [MCHPR/MCHPRS](https://github.com/MCHPR/MCHPRS) | MIT | 2,326 | pushed 2026-09-28 | **1.20.4** (765) | One thread per 512×512 plot. tokio + rayon. Redpiler redstone compiler | Active. v0.5.2 (2026-08-14). Redstone creative only |
| [T-x-T/oxide](https://github.com/T-x-T/oxide) | GPL-3.0 | 78 | pushed 2026-10-09 | "latest version" (not pinned in README) | Minimal deps, custom everything (dashmap, flate2) | Active one-person learning project. GitHub is a mirror of git.thetxt.io |
| [Quozul/PicoLimbo](https://github.com/Quozul/PicoLimbo) | MIT | 544 | pushed 2026-10-10 | **1.7.2–26.3** (multi-version) | Minimal limbo server | Active. Limbo/AFK only |
| [Moulberry/Graphite](https://github.com/Moulberry/Graphite) | **none detected** (no licence = all rights reserved) | 180 | pushed 2025-08-05 | 1.19.1 (repo description) | n/a | Dormant. Can't legally reuse |

Not servers, so excluded: lazymc, lodestone, mcman, Shulker, matscan, Nitrolaunch, chorus (Bedrock), mc173 (Beta 1.7.3), spacetimedb-minecraft, hematite_server (dead since 2021), Quartz (dead since 2023).

Sources: GitHub API metadata, each repo's `Cargo.toml`, `LICENSE` and README, and version constants found in source: `Pumpkin/crates/pumpkin-data/src/generated/packet.rs`, `crates/pumpkin-world/src/lib.rs`, `valence_protocol/src/lib.rs`, `ferrumc/src/lib/net/src/conn_init/mod.rs`, `temper/src/net/runtime/src/conn_init/mod.rs`, `azalea-protocol/src/packets/mod.rs`, `MCHPRS/crates/world/src/lib.rs`, `feather/server/src/initial_handler.rs`.

---

## 2. Project details

### 2.1 Pumpkin: https://github.com/Pumpkin-MC/Pumpkin
* **Licence:** server is GPL-3.0 (`LICENSE`; Cargo.toml `license = "GPL-3.0"`, a deprecated SPDX id that doesn't say whether it's -only or -or-later). `pumpkin-plugin-api` and `pumpkin-plugin-wit` are MIT OR Apache-2.0 (README "License & Attribution"). `assets/NOTICE.md` covers Bedrock mappings and Minecraft assets.
* **Stats:** 12,266★, 907 forks. Last commit 2026-10-10 ("fix(entity): add attack animation for iron golem (#3967)"). The three newest commits all landed within about 4 hours.
* **Version:** Java 26.3 only (`LOWEST_SUPPORTED_MC_VERSION == CURRENT_MC_VERSION == V_26_3`). Bedrock 1.26.51 (protocol 2193) WIP. Crate version `0.2.0+26.3-26.51`, Rust 1.96, edition 2024.
* **Architecture:** no ECS. Server, World and Player are `Arc`-shared structs with `tick()` methods that mirror vanilla's class structure (`server/mod.rs::tick → tick_worlds`, `world/mod.rs::tick`). Players are ticked in parallel with rayon `par_iter` inside a tokio runtime. Chunk generation runs on a dedicated rayon `ThreadPool` (`pumpkin-world/src/chunk_system/schedule.rs`), and chunk IO uses `spawn_blocking`.
* **Implemented (README checklist + source tree):** status/ping, encryption, compression, Java + Bedrock (WIP), world load/save in Vanilla Anvil plus "Linear" and "Pump" formats, lighting, liquids, world border/time, scoreboard, bossbar, inventories, hunger, XP, off-hand, advancements (WIP), entity saving, mobs/animals/villagers/bosses (WIP), entity AI (`entity/ai/{goal,brain,pathfinder,…}`; tracking issue #1406 closed), chunk generation (tracking #36 closed), redstone (tracking #1402 **open**; `block/blocks/redstone/` has wire, repeater, comparator, observer, piston, crafter, sculk sensor and others), combat (#1404 open), commands, permissions, translations, query, MSMP, BungeeCord/BungeeGuard/Velocity forwarding.
* **Plugins:** WASM component-model plugins (wasmtime, WIT) and native dynamic libraries. **PatchBukkit** (https://github.com/Pumpkin-MC/PatchBukkit, GPL-3.0) embeds a JVM (Java 25+) through JNI/FFM and reimplements the Bukkit API surface to run Bukkit/Spigot/Paper jars. Its own README says it's in heavy development.
* **Performance claims:** pumpkinmc.org says about 100 MB at idle with a world loaded vs "near 1.8 GB" for vanilla, and 0.005 s startup vs 15 s (https://pumpkinmc.org/). The project's benchmarks page says outright "**This comparison is unfair.** Pumpkin currently has far fewer features…" (https://docs.pumpkinmc.org/about/benchmarks). The tables there didn't render through my fetch, so I'm not quoting them. A 2024 Reddit post by the author reported 300 moving bots at about 70 MB RAM and 4–5% CPU, at render distance 3 (https://www.reddit.com/r/rust/comments/1evhurt/).
* **Maintained:** yes, very actively.

### 2.2 SteelMC: https://github.com/Steel-Foundation/SteelMC
* **Licence:** AGPL-3.0-or-later (workspace Cargo.toml). 757★. Last push 2026-10-10. Release v0.15.4+mc26.2 on 2026-10-03.
* **Version:** 26.2.
* **Architecture:** custom crates (`steel-protocol`, `steel-registry`, `steel-worldgen`, `steel-core`, …), tokio, rayon, crossbeam, parking_lot, simdnbt (the NBT crate by Azalea's author). No ECS. Synchronous gameplay tick, with chunk gen, lighting, packets and chunk sending off the main tick (README).
* **Implemented (README):** networking, auth, encryption, compression, persistent chunk generation/loading/saving, lighting, movement/collision, block interaction, inventories, commands, permissions, chat. Early entity and block-entity behaviour. **No plugins.** README: "Paper, Bukkit, Fabric, Forge, and NeoForge extensions are not compatible."
* **Worldgen parity:** "compares 7,500 randomly selected chunks with a reproducible vanilla reference … All tested chunks match block for block" (README).
* **Performance claims (https://steelmc.dev/reference/benchmarks/, updated 2026-10-08):** a 101×101 (10,201-chunk) Overworld pregeneration. Median **3.95 s** for Steel vs **74.42 s** for Fabric (vanilla worldgen via Chunky), i.e. 18.8× faster. Steel used about 30.2 cores on average vs 4.4. Per CPU-second Steel was 2.75× more efficient. Median peak RSS was 1.79 GiB vs 2.47 GiB. Run on a Ryzen 9 9950X with tmpfs. Their caveats: entity-spawn generation is disabled in Steel, and the test is worldgen only.
* **Method note:** contributors are told to "Generate the targeted vanilla source with `./update-minecraft-src.sh` and verify behavior against it". The script uses GitCraft to produce unobfuscated vanilla source locally. This matters for the legal analysis in §5.

### 2.3 FerrumC: https://github.com/ferrumc-rs/ferrumc
* **Licence:** MIT. 2,421★. Last master commit 2026-08-14 (README fix). The rewrite branch's head commit is dated 2026-07-18 ("Merge … into integration/v2-full").
* **Version:** 1.21.8 (protocol 772).
* **Architecture:** Bevy ECS 0.18 (`multi_threaded`), tokio, rayon, crossbeam. Custom NBT and Anvil code ("some raw assembly"). World storage in a K/V database (LMDB through `heed`). Anvil worlds are *imported* rather than used as the live format. Requires nightly Rust.
* **Implemented:** README features: vanilla client join, world import, multithreaded chunk loading. "Upcoming" list: PvE, entities, plugins (Rust FFI). The v2 banner describes a flat creative world, block placement orientation, signs and chests, `/fill`, saving, import, Rust plugins and a dashboard, and calls itself "early, offline-mode alpha … not survival, not vanilla-accurate".
* **Performance claims:** marketing only ("Extremely fast", a memory-use screenshot, and "Scales linearly with available cores" at https://www.ferrumc.com). I found no reproducible benchmark.
* **Maintained:** yes, but mid-rewrite, so the current master is a moving target.

### 2.4 Temper: https://github.com/temper-mc/temper
* GPL-3.0. 95★. Created 2026-02-13. "Originally a fork of the FerrumC project". Targets 26.2 (protocol 776). Bevy ECS 0.19 + tokio + rayon + LMDB. v0.2.0 on 2026-10-02. Its build needs a JDK to extract assets from the vanilla server jar.
* Licence note: relicensing an MIT fork as GPL is allowed, but the original MIT notices must be kept.

### 2.5 Valence: https://github.com/valence-rs/valence
* **Licence:** MIT for code. The logo is CC BY-NC-ND 4.0, so don't reuse it. 3,281★. Last commit 2026-06-15. The commit before that is 2026-01-15. The only GitHub release is v0.1.0 (2022). Workspace version `0.2.0-alpha.1+mc.1.20.1`.
* **Version:** 1.20.1 (763). Single version by design. Use a ViaVersion/ViaBackwards proxy for other versions.
* **Architecture:** a Bevy ECS 0.14 *framework*: "doesn't do much by default". Vanilla mechanics are expected to come from plugins. tokio, rayon, flume. A bounding volume hierarchy for spatial queries.
* **Implemented (README):** auth, encryption, compression, block states, chunks, entities + metadata, player list/skins, dimensions/biomes, text, inventories, items, particles, **Anvil read-only**, Velocity/Bungee/Waterfall, commands, advancements, scoreboard, boss bar (crates list), and a Fabric mod that extracts game data as JSON for codegen. No vanilla worldgen, redstone, mob AI or crafting rules.
* **Performance claim:** supports "thousands of players at the same time without lag (assuming you have the bandwidth)", linked to a screenshot (`assets/many-players.png`). Not a reproducible benchmark.
* **Maintained:** low activity. Not archived.

### 2.6 Feather: https://github.com/feather-rs/feather
* Apache-2.0. 2,702★. Last commit 2024-02-20 (last push 2024-04-12). README: "This project is currently inactive. Consider contributing to valence instead." Not archived.
* 1.16.5 (protocol 754). ECS = fork of `hecs`. tokio. Plugins ("Quill") compiled to WASM and sandboxed. `libcraft` crates for blocks, items, chunks and region files.
* **Performance claims (README, with its own caveat that comparisons "will be extremely misleading"):** 1,000,000 plugin-spawned entities "before it starts to max out the CPU". 500 concurrent walking players. About 40 MiB RAM in the 500-player test. 400 MiB with 1 M entities.
* Worth reading for ideas only. Old protocol, dead code.

### 2.7 Azalea: https://github.com/azalea-rs/azalea
* MIT. 788★. Last commit 2026-10-06. Currently supports 26.2 (protocol 776). Crate version `0.16.0+mc26.2`.
* Bevy ECS/App 0.19 + tokio. Crates for protocol, NBT (via simdnbt), registries, physics ("accurate physics", some gaps), pathfinding, inventories, chat, auth.
* **Why it matters:** it's MIT, current, and has a complete client-side protocol codec and accurate physics. That makes it useful as a test harness: Azalea-driven bots can drive conformance and load tests against your server. Pumpkin also publishes **BotMark** (MIT, https://github.com/Pumpkin-MC/BotMark) for stress testing. You can also borrow Azalea's protocol and registry crates directly.

### 2.8 Others
* **MCHPRS** (MIT, 2,326★, 1.20.4): a redstone-only creative server. Each 512×512 plot runs on its own thread. **Redpiler** compiles redstone circuits into a graph for very fast simulation. Its README notes that compilation changes semantics (e.g. `--optimize` "can, and will, break the state of your build"). A great reference for *fast* redstone, not *vanilla-exact* redstone.
* **PicoLimbo** (MIT): handles every version from 1.7.2 to 26.3. Claims "hundreds of players under 10 MB RAM" with 0% idle CPU (https://picolimbo.quozul.dev/about/benchmarks.html, linked from its README). A good reference for multi-version packet tables.
* **Oxide** (GPL-3.0): a learning project with a YouTube series. Has crafting, smelting and basic AI, but no encryption or compression yet.

---

## 3. Licence compatibility for merging

| From → into | MIT | Apache-2.0 | GPL-3.0 | AGPL-3.0-or-later |
|---|---|---|---|---|
| MIT (Valence, FerrumC, Azalea, MCHPRS, PicoLimbo) | ✓ | ✓ | ✓ | ✓ |
| Apache-2.0 (Feather) | ✓ only if the combined work is distributed under Apache terms (keep NOTICE, patent clause) | ✓ | ✓ (Apache-2.0 is compatible with GPLv3, but not GPLv2) | ✓ |
| GPL-3.0 (Pumpkin, Temper, Oxide) | ✗ | ✗ | ✓ | ✓ (GPLv3 §13 allows combining with AGPLv3) |
| AGPL-3.0-or-later (SteelMC) | ✗ | ✗ | ✗ (the combined work must be AGPL) | ✓ |

What follows:
* A repo containing Pumpkin code must be **GPL-3.0** (or AGPL). A repo containing SteelMC code must be **AGPL-3.0**, which means anyone you let connect over a network can demand the source of your modified server.
* If you want MIT or Apache for your own work, you can only take code from Valence, FerrumC, Azalea, MCHPRS, PicoLimbo and Feather.
* Pumpkin's bare `GPL-3.0` id leaves "only" vs "or later" unclear. If you plan to mix GPL and AGPL code, check with the Pumpkin maintainers. Mixing still works under GPLv3 §13 either way.
* Contributing upstream sidesteps all of this.
* Data and assets: every project derives generated data (block states, registries, packet IDs) from Mojang's server jar or data generator. That isn't open-source-licensed data (see §5). Generate it at build time instead of committing big Mojang-derived blobs where you can. Pumpkin's `assets/NOTICE.md` is an example of careful attribution.

---

## 4. Technical feasibility of merging

The incompatibilities are deep:

1. **Core state model.** Pumpkin uses Arc'd objects with interior mutability and async methods. Valence, FerrumC, Temper and Azalea use Bevy ECS with systems and components. SteelMC is custom with a synchronous tick. Feather uses hecs. Entity, world and player code can't move between these without rewriting it. Even among the Bevy projects, Bevy 0.14, 0.18 and 0.19 have breaking API changes between them.
2. **Protocol version.** 1.16.5, 1.20.1, 1.20.4, 1.21.8, 26.2 and 26.3 have different packet IDs, registries (data-driven registries, the configuration phase added in 1.20.2, item components added in 1.20.5) and chunk formats. Older code needs real porting, not just a merge.
3. **Generated data pipelines differ.** Valence uses a Fabric extractor mod. Pumpkin uses `pumpkin-codegen` plus extractor data. Oxide and Temper run the vanilla data generator. SteelMC uses GitCraft sources plus `update-minecraft-assets.sh`. Each project's block-state and item IDs are its own Rust types.
4. **Storage.** Pumpkin uses Anvil, Linear and Pump. FerrumC and Temper use LMDB with Anvil import. Valence's Anvil support is read-only.

**Conclusion:** a multi-project code merge is a rewrite, not a merge. The workable approach is **one base plus targeted ports**. Some leaf libraries are easy to share because they're self-contained and permissive: NBT (`valence_nbt`, `simdnbt`), text components, `azalea-protocol`/`azalea-registry`, MCHPRS's Redpiler *ideas*, and PicoLimbo's multi-version packet tables.

---

## 5. What "the original Minecraft spec" actually is

There is **no official protocol or game-logic specification** from Mojang. These are the sources in practice:

1. **Community protocol docs:** minecraft.wiki's *Java Edition protocol* pages (https://minecraft.wiki/w/Java_Edition_protocol). These cover packets, data types, encryption, plugin channels, registry data, chunk format, entity metadata and the Forge handshake. They came from **wiki.vg**, which shut down on 30 November 2024 and was imported into minecraft.wiki (https://minecraft.wiki/w/Minecraft_Wiki:Projects/wiki.vg_merge). The wiki itself warns that the docs "may or may not be completely up to date".
2. **Data generators** inside the official server jar: `java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports` (or `--all`). These output `blocks.json`, `registries.json`, packet reports, plus loot tables, recipes, tags and worldgen JSON (https://minecraft.wiki/w/Tutorial:Running_the_data_generator). Oxide's README uses exactly this.
3. **The vanilla code itself.** Up to 1.21.11, Mojang shipped obfuscated jars with official obfuscation mappings, licensed: "You may copy and use the mappings for development purposes, but you may not redistribute the mappings complete and unmodified … Use and modification of … the source code (in any form) of Minecraft: Java Edition is governed by the Minecraft EULA" (https://minecraft.wiki/w/Obfuscation_map). **Since 26.1 Snapshot 1 (16 Dec 2025) all Java Edition builds, including the server jar, are unobfuscated**, so there are no more mapping files. Mojang says this "doesn't affect our Minecraft End User License Agreement and Minecraft Usage Guidelines", and each jar now carries a LICENSE file pointing to the EULA (https://www.minecraft.net/en-us/article/removing-obfuscation-in-java-edition, https://www.minecraft.net/en-us/article/minecraft-java-edition-26-1). Exact behaviour (worldgen noise, redstone update order, mob AI) can only be reproduced by reading this code. SteelMC's parity approach is built on it.
4. **Black-box behaviour:** packet captures with a proxy against vanilla (Oxide ships one), plus bot-driven differential testing (Azalea).

**Legal and EULA considerations (I'm not a lawyer, so take advice before commercialising):**
* The EULA lets you make "tools, plug-ins and services as long as they do not seem official", and defines Mods as original work that "doesn't contain a substantial part of our copyrightable code or content". You "may not distribute any Modded Versions of our game or software" (https://www.minecraft.net/en-us/eula). An independent Rust server is generally treated as an original tool. *Line-by-line translation* of decompiled Mojang Java into Rust is the grey area: it could be argued to contain or derive from their copyrightable code. Behaviour-level reimplementation (understand it, then write your own) is safer. Do not commit decompiled Mojang source, jars, mappings or assets to your repo.
* Don't use Mojang or Minecraft logos or anything that implies official status. Use your own name and branding.
* Hosting rules from the Usage Guidelines still apply to servers you run: online mode for paid accounts, monetisation rules, and so on (https://www.minecraft.net/en-us/usage-guidelines). Mojang can block servers from clients, whatever the legal theory.
* Your licence choice is separate from these issues: GPL doesn't make any Mojang-derived content open.

---

## 6. Mod compatibility: what's realistic

| Category | Examples | Works on a Rust server? |
|---|---|---|
| Client-only mods | Sodium, Iris, minimaps, Lithium-client, HUD mods, shaders | **Yes, mostly already.** They speak vanilla protocol. Some client mods use optional plugin channels (e.g. Xaero's or VoxelMap minimap permissions), which you can implement one by one |
| Vanilla-protocol "server-side only" content | data packs, resource packs, Polymer-style mods (server-side content mapped onto vanilla blocks/items) | Data packs: **achievable**, but it's a big job (functions, predicates, loot tables, worldgen JSON, recipes, advancements all need implementing). Resource packs: easy (a URL + hash packet). Polymer-style mods are Java, so they can't run directly, but the *technique* can be copied in native plugins |
| Fabric / Forge / NeoForge **server-side mods** | Create, Applied Energistics, most content mods | **Not realistically.** These are Java bytecode that mixes into and calls `net.minecraft.*` server classes (Mixin/patches). Running them would mean an embedded JVM *plus* a reimplementation of vanilla's internal Java API as a compatibility layer. That's effectively rebuilding the Java server, which defeats the purpose |
| Modded **clients** joining | a Fabric client with optional mods | Possible. Fabric registry sync sends `fabric:registry/sync` only for registries that are both SYNCED *and* MODDED, so a server that adds nothing modded mostly doesn't trigger it (https://deepwiki.com/FabricMC/fabric/5-registry-synchronization). NeoForge negotiates channels in the configuration phase: a modded client can join a vanilla-like server only if it has no *required* modded channels (https://github.com/neoforged/NeoForge, `NetworkRegistry`/`NetworkComponentNegotiator`). Forge's handshake is documented on minecraft.wiki |

**Realistic options, ranked:**
1. **Native Rust/WASM plugin API** (Pumpkin already has this; Feather's Quill was similar). This is what makes the server worth using.
2. **Bukkit/Paper API through an embedded JVM.** Pumpkin's PatchBukkit already does this (JNI/FFM, Java 25+). Bukkit is a *stable, documented, internals-free* API, so it can be reimplemented. Fabric and NeoForge are not like that. Expect gaps around NMS (`net.minecraft.server`) hacks that many plugins use.
3. **Implement specific mods' network protocols** so their clients can connect and run in reduced mode: plugin channels via custom payload, registry sync, configuration-phase tasks.
4. **Hybrid deployment:** a Velocity proxy in front, with the Rust server for lobby, minigames or high-population vanilla worlds and real Fabric/NeoForge servers for modded worlds. This is how big networks already mix server software. It's the honest answer to "works with all mods".
5. Embedding a JVM to host real Fabric/NeoForge mods: research only. I don't recommend it.

---

## 7. Recommended path forward

**Decision first:** do you need permissive licensing (e.g. a commercial closed fork)?
* **No:** build on **Pumpkin** (GPL-3.0). Either contribute upstream or keep a fork that tracks upstream closely. You get the biggest community, the latest protocol, the broadest coverage, plugins, and the Bukkit JVM bridge.
* **No, and vanilla-exact parity is the priority:** pick **SteelMC**, and accept AGPL plus a smaller feature set today.
* **Yes:** start a new MIT/Apache workspace on **Azalea's protocol and registry crates** (MIT, 26.2), `simdnbt`/`valence_nbt`, and Valence's architecture ideas (Bevy ECS). Port nothing from Pumpkin, Temper, Oxide or SteelMC, and don't read their source while writing equivalent code if you want a clean-room record. This is a multi-year effort.

**Phases (assuming the Pumpkin-based route):**

* **Phase 0 (1–2 weeks): set up.** Fork Pumpkin into your org. Set up CI that downloads the official 26.3 server jar *at build time* and runs the data generator. Never commit Mojang jars, assets or decompiled source. Read Pumpkin's 1.0 tracking issue (#449) and the open trackers: redstone #1402, world #1403, combat #1404.
* **Phase 1 (1–2 months): conformance harness.** This is the most valuable thing you can add. Use Azalea/BotMark bots and a packet-diff proxy to run scripted scenarios against vanilla 26.3 and against your server, then diff packets, block states and entity states. Add a SteelMC-style worldgen parity suite (chunk hashes vs vanilla for N seeds/chunks) and redstone test worlds (vanilla gametests / `/test` structures; Pumpkin has a `pumpkin-gametest` crate). This turns "fully compatible" into a measurable number.
* **Phase 2 (ongoing): close vanilla gaps by priority.** Combat and survival loop, redstone update order (use MCHPRS for performance ideas but keep vanilla semantics), mob AI breadth, villages and structures, data-pack execution (functions, predicates, loot, recipes), then the long tail. Upstream everything to keep fork drift low.
* **Phase 3 (parallel): extensibility.** Stabilise the WASM/native plugin API. Push PatchBukkit coverage and use top Paper plugins (LuckPerms, WorldEdit/FAWE, EssentialsX, Vault) as acceptance tests. Add a typed plugin-channel API so plugins can talk to modded clients.
* **Phase 4: modded-client interoperability.** Handle Fabric registry sync and NeoForge channel negotiation gracefully, and implement chosen mods' channels (e.g. minimap permissions, Voice Chat). Document the Velocity hybrid setup for full Fabric/NeoForge modpacks.
* **Phase 5: performance and production.** Publish reproducible benchmarks (fixed hardware, players simulated with BotMark, compared against Paper/Folia), test crash-safe Anvil persistence against vanilla round-trips, and add online-mode hardening.

**Version policy:** follow latest-release-only, as Pumpkin, SteelMC, Valence and Azalea all do, and point people to ViaVersion/ViaBackwards proxies for older clients. Multi-version support inside the server (PicoLimbo-style) multiplies the work.

---

## 8. Source index
* Repos: https://github.com/Pumpkin-MC/Pumpkin · https://github.com/Steel-Foundation/SteelMC · https://github.com/ferrumc-rs/ferrumc · https://github.com/temper-mc/temper · https://github.com/valence-rs/valence · https://github.com/feather-rs/feather · https://github.com/azalea-rs/azalea · https://github.com/MCHPR/MCHPRS · https://github.com/T-x-T/oxide · https://github.com/Quozul/PicoLimbo · https://github.com/Moulberry/Graphite · https://github.com/Pumpkin-MC/PatchBukkit · https://github.com/Pumpkin-MC/BotMark
* Pumpkin claims: https://pumpkinmc.org/ · https://docs.pumpkinmc.org/about/benchmarks · https://www.reddit.com/r/rust/comments/1evhurt/
* SteelMC benchmarks: https://steelmc.dev/reference/benchmarks/
* FerrumC site: https://www.ferrumc.com
* PicoLimbo benchmarks: https://picolimbo.quozul.dev/about/benchmarks.html
* Protocol docs: https://minecraft.wiki/w/Java_Edition_protocol · wiki.vg merge: https://minecraft.wiki/w/Minecraft_Wiki:Projects/wiki.vg_merge
* Data generator: https://minecraft.wiki/w/Tutorial:Running_the_data_generator
* Mappings licence: https://minecraft.wiki/w/Obfuscation_map
* De-obfuscation: https://www.minecraft.net/en-us/article/removing-obfuscation-in-java-edition · https://www.minecraft.net/en-us/article/minecraft-java-edition-26-1 · https://fabricmc.net/2026/03/14/261.html
* EULA: https://www.minecraft.net/en-us/eula · Usage guidelines: https://www.minecraft.net/en-us/usage-guidelines
* Fabric registry sync: https://deepwiki.com/FabricMC/fabric/5-registry-synchronization · NeoForge negotiation: https://github.com/neoforged/NeoForge (src/main/java/net/neoforged/neoforge/network/)

Raw API JSON, READMEs and Cargo.toml files are saved in `/workspace/rust-mc-servers/raw/`. Shallow clones are in `/workspace/rust-mc-servers/src/`.
