# Pumpkin vs vanilla Java Edition

Inventory of this tree against the Java Edition version it targets. Statuses are based on the code that is in the repo, not on a full play-through of every system.

The protocol target is Java **26.3**, protocol **777**. `CURRENT_MC_VERSION` and `LOWEST_SUPPORTED_MC_VERSION` are both `JavaMinecraftVersion::V_26_3` in `crates/pumpkin-data/src/generated/packet.rs`. The protocol number is `V_26_3 => 777` in `crates/pumpkin-util/src/version.rs`. Older Java clients are rejected. Bedrock is a separate stack and is not covered here except where it affects the Java server.

Statuses:

- **implemented**: the path exists and is wired into normal play
- **partial**: a real subset works, and a normal player will hit the missing part
- **missing**: not present, or present only as a type with no behaviour

## Priority

These are ordered by how badly they break a normal survival or creative session on a current Java client. The first three stop or scramble a world. The rest are holes inside a session that otherwise starts.

1. **A world saved by the 26.3 dedicated server will not load, and there is no DataFixer.** `MINIMUM_SUPPORTED_WORLD_DATA_VERSION` is 4435 (commented 1.21.9) and `MAXIMUM_SUPPORTED_WORLD_DATA_VERSION` is 4903 (commented 26.2) in `crates/pumpkin-world/src/world_info/mod.rs`. Chunks are written with `WORLD_DATA_VERSION` 4903 in `crates/pumpkin-world/src/chunk/format/anvil.rs`. The 26.3 server jar's built-in `DetectedVersion.createBuiltIn` constructs `DataVersion` **5023**. 5023 is above 4903, so a fresh 26.3 world is rejected. Worlds from before 1.21.9 are rejected too.
2. **The live clock, weather, and world border are not restored from the save.** `World::load` in `crates/pumpkin-core/src/world/mod.rs` builds `LevelTime::new()`, `Weather::new()`, and `Worldborder::new(0.0, 0.0, 5.999_996_8E7, ...)`. `LevelTime::load_from` exists in `crates/pumpkin-core/src/world/time.rs` and has no callers. The scoreboard on that world is `Scoreboard::default()` in the same constructor. A reloaded world comes back at a fresh time, with clear weather and the default border, even when `level.dat` says otherwise.
3. **World generation is not block-for-block with vanilla.** `crates/pumpkin-world/src/generation/proto_chunk_test.rs` allows 6000 noise mismatches, 7500 surface mismatches, and 8000 carver mismatches against the vanilla dumps in `assets/tests/`. Structures (villages, bastions, ancient cities, trial chambers, mineshafts, strongholds, monuments, mansions, end cities, fortresses, and others) are wired through the jigsaw and template code, but those tests still tolerate piece differences. `ConfiguredFeature::NoOp` in `crates/pumpkin-world/src/generation/feature/configured_features.rs` is the stand-in for a feature the generator does not run.
4. **Several hostile mobs do not have their vanilla attacks.** Goal selectors do tick (`crates/pumpkin-core/src/entity/mob/mod.rs`). Phantom only looks around; `PhantomCircleAroundAnchorGoal` and `PhantomSweepAttackGoal` are still a TODO in `crates/pumpkin-core/src/entity/mob/phantom.rs`. Breeze swims, wanders, and targets the player, and does not throw wind charges (`crates/pumpkin-core/src/entity/mob/breeze.rs`). The wind-charge entity itself exists (`crates/pumpkin-core/src/entity/projectile/wind_charge.rs`). Warden emerges, applies darkness, and melee-attacks; it has no sonic boom (`crates/pumpkin-core/src/entity/mob/warden.rs`). `from_type` in `crates/pumpkin-core/src/entity/type.rs` falls back to a generic living entity when the type has attributes and to a bare entity otherwise. New mobs are constructed with `Brain::default()` in `crates/pumpkin-core/src/entity/mob/mod.rs`.
5. **Redstone and pistons are a partial port.** `crates/pumpkin-core/src/block/blocks/redstone/` has wire, repeater, comparator, observer, dispenser, dropper, sculk, crafter, rails, and the other files in that directory (33 `.rs` files). Piston movement exists, with `// TODO: more checks` in `crates/pumpkin-core/src/block/blocks/piston/piston.rs`. Update order is implemented in the neighbor updater, and it has not been shown to match vanilla on quasi-connectivity, sticky-block chains, or locational order.
6. **Advancement triggers are a short hardcoded list.** `AdvancementTrigger` in `crates/pumpkin-core/src/entity/player/advancement/trigger.rs` covers inventory, kills, sleep, fishing, placed blocks, dimensions, eating, and a handful of challenge triggers. `EnterBlock` is an empty match arm. Most vanilla criteria (location predicates, player predicates, item predicates, effects, nether travel, and so on) are not fired. Definitions exist in generated advancement data; firing them does not.
7. **Datapacks load a subset of pack contents.** `load_pack_contents` in `crates/pumpkin-core/src/data/datapack/mod.rs` loads recipes, functions, function tags, damage types, loot tables, villager trades, structure NBT, worldgen template pools, processor lists, test instances, context number providers, and some dynamic registries. There is no predicate loader. Worldgen JSON other than template pools and processor lists is not loaded there. A pack that changes biomes, density functions, or placed features through JSON will not change the running world.
8. **Most items and many blocks use a default behaviour.** `default_registry` in `crates/pumpkin-core/src/item/items/mod.rs` has 58 `manager.register(...)` calls. Other items fall through to the default item behaviour. Block behaviours live under `crates/pumpkin-core/src/block/blocks/` (212 `.rs` files) and are registered from `block/blocks/mod.rs` and `block/registry.rs`. Blocks without an entry use the generic block path, so interactions such as stripping, tilling, and many right-clicks are missing even when the block can be placed.
9. **Configuration and reconfiguration are not finished.** Handshake, status, login, encryption (`crates/pumpkin-core/src/net/java/login/encryption_response.rs`), compression, and the known-pack registry sync are implemented. In the configuration state, `handle_config_packet` in `crates/pumpkin-core/src/net/java/pending.rs` dispatches client information, plugin messages, finish acknowledgement, known packs, resource packs, cookies, pong, and the code of conduct. Any other config packet, including keep-alive and custom click, returns `ReadingError` and the client is kicked. `handle_configuration_acknowledged` in `crates/pumpkin-core/src/net/java/play/configuration_acknowledged.rs` only stores `ConnectionState::Config`. It does not send registries again, so a client that the server moves back into configuration does not complete the vanilla reconfiguration handshake. Client information in 26.3 also carries a particle-status field (`ClientInformation` in the 26.3 server jar). `SClientInformationConfig` in `crates/pumpkin-protocol/src/java/server/config/client_information.rs` stops at server listing. Pumpkin ignores the extra byte on read, so a real client still gets in. A vanilla 26.3 server rejects a packet written by that codec.
10. **Only one Java protocol is accepted, and modded clients are out of scope of this tree.** Both version constants are 26.3. There is no Via-style translation. Bedrock play is a parallel implementation and is still in progress. Fabric, Forge, and NeoForge are not loaded. Plugins are Pumpkin's own native API (`PLUGIN_API_VERSION` is 2 in `crates/pumpkin-core/src/plugin/mod.rs`) and the Wasm hosts `pumpkin-wasm-host-v0_1` and `pumpkin-wasm-host-v0_2`.

## Area inventory

| Area | Status | Where to look |
| --- | --- | --- |
| Protocol states (handshake, status, login, configuration, play) | implemented | `crates/pumpkin-protocol/src/lib.rs` (`ConnectionState`), `crates/pumpkin-core/src/net/java/` |
| Login, encryption, compression | implemented | `net/java/login/`, `packet_encoder.rs` / `packet_decoder.rs` compression |
| Configuration registry sync | partial | Known packs and registry data are sent. Keep-alive is not dispatched. Reconfiguration does not resend registries. |
| Play packet dispatch | partial | Serverbound play ids are dispatched. An unknown play id is a warning in `net/java/mod.rs` and does not kick. An unknown config id does kick. |
| Chunk IO / Anvil | partial | `crates/pumpkin-world/src/chunk/format/anvil.rs` reads gzip, zlib, lz4, and uncompressed. Custom compression (id 127) returns `UnknownCompression`. Linear and Pump are Pumpkin formats, not vanilla region files. |
| World gen | partial | `crates/pumpkin-world/src/generation/`, with the mismatch tolerances in `proto_chunk_test.rs` |
| Lighting | implemented | `crates/pumpkin-world` lighting, selected by the world config (Default, Full, Dark) |
| Dimensions | partial | Overworld, Nether, and End exist. `World::load` does not restore time, weather, or the border. |
| Entities and mob AI | partial | `entity/type.rs`, `entity/mob/`, `entity/passive/`, `entity/ai/` (108 `.rs` files, 57 under `ai/goal/`) |
| Combat and hunger | partial | Damage, attack reach, and hunger exist on the living entity and player. Warden sonic boom and breeze wind charges do not. |
| Redstone and pistons | partial | `block/blocks/redstone/`, `block/blocks/piston/` |
| Blocks and items | partial | 212 block behaviour files, 58 registered item behaviours. The rest use defaults. |
| Crafting, loot, trades | partial | Datapack recipe, loot, and trade loaders under `crates/pumpkin-core/src/data/datapack/` |
| Advancements | partial | `entity/player/advancement/trigger.rs` |
| Data packs | partial | `data/datapack/mod.rs` `load_pack_contents` |
| Commands | partial | 90 files under `crates/pumpkin-core/src/command/commands/`, including `execute.rs`, `scoreboard.rs`, `team.rs`, `gamemode.rs`, `gamerule.rs`, `fill.rs`, `datapack.rs`, `function.rs`, `ban.rs` |
| Scoreboard and teams | partial | Commands exist. The world's scoreboard is constructed empty in `World::load`, so objectives do not survive a restart. |
| Plugins | partial | Native API v2 and Wasm v0.1 / v0.2. No Bukkit, Fabric, Forge, or NeoForge in this tree. |

## What a normal client can already do

A 26.3 Java client can complete handshake, status, offline or online login, compression, and configuration, including registry and tag sync when it answers known packs. It receives chunks and can move (movement of the player's own avatar is client-authoritative; see the root `AGENTS.md`). Block entities exist for inventories, beds, portals, furnaces, and brewing stands. The command set above is registered. Lighting is implemented. Anvil worlds whose data version is inside 4435..=4903 can be opened.

## What this note did not verify

- Block-for-block worldgen, redstone update order, or mob behaviour against a running vanilla server. The parity tests that do exist allow thousands of block mismatches. The harness milestone diff only checks that both servers emit the login, registry, teleport, and chunk packets.
- In-game feel: particles, sounds, animations, and screens. Those need a real client.
- Whether every registered command matches vanilla syntax. The files exist; each argument tree was not compared with the Java source.

## Running the harness

`tools/pumpkin-compat` is a headless Java client built on `pumpkin-protocol`, so it speaks protocol 777. Azalea is not used: current Azalea crates speak 26.2.

The vanilla jar is downloaded at test time from `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` and the server URL in that version's JSON. The cache is `target/compat-cache` or `PUMPKIN_COMPAT_CACHE`. `eula=true` is written only in the temporary world directory. Nothing from Mojang is committed.

```bash
cargo build -p pumpkin -p pumpkin-compat
cargo test -p pumpkin-compat --lib
PUMPKIN_BIN=target/debug/pumpkin cargo test -p pumpkin-compat --test smoke -- pumpkin_smoke --exact --ignored --test-threads=1
```

The same smoke test from the binary:

```bash
PUMPKIN_BIN=target/debug/pumpkin cargo run -p pumpkin-compat -- smoke
```

Login milestones against vanilla (Java 25, which the 26.3 server jar requires):

```bash
JAVA_HOME=/path/to/java-25 PUMPKIN_BIN=target/debug/pumpkin PUMPKIN_COMPAT_VANILLA=1 \
  cargo test -p pumpkin-compat --test smoke -- vanilla_milestones --exact --ignored --test-threads=1
```

Or `cargo run -p pumpkin-compat -- diff`. The diff checks that both servers produced login success, registry data, finish configuration, play login, a player position, and a chunk. It does not compare chunk bytes.

`.github/workflows/compat.yml` builds Pumpkin and runs `pumpkin_smoke`. A second job installs Temurin 25 and runs `vanilla_milestones`. The smoke test is `#[ignore]` so `.github/workflows/rust.yml` does not start a server during `cargo nextest`.
