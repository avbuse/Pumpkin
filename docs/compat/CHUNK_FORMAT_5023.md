# Chunk fields from data version 4903 to 5023

Java 26.2 saves chunks at data version 4903. Java 26.3 saves them at 5023. This note is the input for A-48. A-48 updates `crates/pumpkin-world/src/chunk/format/` so a 4903 chunk still loads and a chunk Pumpkin writes is 5023. It does not build a general DataFixer.

The comparison is two dedicated-server worlds, both seed `A08chunk`, plus the 26.2 and 26.3 server jars. Class names below are pointers. Reimplement from the description.

The only chunk field Pumpkin reads incorrectly is the block-state palette. Everything else in this note is either a nested tag the codec already copies, or a field that was checked and needs no loader change.

## Fields the chunk codec must read and write

### `sections[].block_states.palette` (reinterpreted)

Lives on each section of the chunk compound, inside `block_states`. Pointers: `BlockState` (`CODEC`), `StateHolder`, and `BlockStateFieldNamesFix` (registered at schema 5006). The [Java Edition 26.3](https://minecraft.wiki/w/Java_Edition_26.3) changelog, section "Block states", records the rename and the compact block id.

4903. Every palette entry is a compound.

- `Name`: string, the block id, for example `minecraft:oak_stairs`.
- `Properties`: compound of string property values. Omitted when the block has no properties (`minecraft:air`, `minecraft:bedrock`). Present when the block has properties, including values that are the block's default. A default deepslate entry is `Name` `minecraft:deepslate` and `Properties` `axis` = `y`. A default furnace is `Name` `minecraft:furnace` and `Properties` `facing` = `north`, `lit` = `false`.

5023. A palette entry is either the block id or the full state. The codec writes the id when the state is that block's default state, and the full state otherwise. An NBT list has one element type, so the saved shape depends on the whole palette:

- Every entry is a default state: the palette is a list of strings. Each string is the block id. A fresh overworld section at `Y` -4 was this shape, including `minecraft:bedrock` and `minecraft:deepslate`. `minecraft:air` and `minecraft:stone` are the same kind of entry.
- Any entry is not the default: the palette is a list of compounds.
  - A non-default entry has `id` (string, the block id) and `properties` (compound of strings). Oak stairs facing east were `id` `minecraft:oak_stairs` and `properties` `facing` = `east`, `half` = `bottom`, `shape` = `straight`, `waterlogged` = `false`.
  - A default entry in that same list is a compound with one field. The field name is the empty string, and the value is a string, the block id. In the stairs section, air, stone, and the default furnace (`facing` north, `lit` false) were stored that way. This is how a string is stored inside a compound list. It is not a missing `id`.

Property values stay strings in both versions. `sections[].block_states.data` is still an optional long array of palette indices. `sections[].Y` is still a byte on a fresh chunk (`-4` for the bottom overworld section). `yPos` on the chunk root is still an int (`-4` in the overworld).

`sections[].biomes.palette` is still a list of biome-id strings in both versions, for example `minecraft:river`. Pumpkin already reads and writes that. Leave it.

What Pumpkin does today, and what A-48 changes:

- The reader resolves a compound through `PaletteEntry::from_nbt_compound`, which already accepts `id` or `Name` and `properties` or `Properties`. A string entry is not a compound, so it becomes air. An empty-named compound has neither `id` nor `Name`, so it becomes air too. A 5023 deepslate section, and the air and stone in a mixed section, would load as air. Non-default stairs can already resolve.
- The writer always emits a compound with `Name`, and `Properties` whenever the block has properties. That is the 4903 shape. A 5023 writer emits a string for a default state, and `id` plus `properties` for any other state.

Read all three shapes. A 4903 compound with `Name` and optional `Properties` must still resolve, including a default state that still carries `Properties`. A 5023 string is the default state of that block. A 5023 compound whose only field is an empty name and a string is the default state of that block id. A 5023 compound with `id` and `properties` is that exact state.

Write 5023:

- If every entry in the palette is the default state of its block, write a list of block-id strings.
- Otherwise write a list of compounds. A default entry in that list is the empty-named string compound. A non-default entry is `id` plus `properties`, with every property the state has, the way oak stairs were saved. Do not keep writing `Name` and `Properties`.

Treat "default" as the block's default state, not as "the block has no properties". Furnace facing north is a default state and is written as the block id. Oak stairs facing east is not.

## Nested tags the codec already copies

These live inside compounds that `chunk/format` already clones: `block_entities[]` on the chunk, and `Entities[]` on the entity chunk. A-48 does not add a field reader for them and does not upgrade an old value into the new type. Copy the compound with its original tag types. A 4903 chunk that omits an added field must still load. A 5023 chunk that contains the field must round-trip that tag because the compound is stored whole.

The [Java Edition 26.3](https://minecraft.wiki/w/Java_Edition_26.3) changelog records these under "Block entity data", "Block entity format", and "Entity data".

### `block_entities[]` for `minecraft:furnace`, `minecraft:smoker`, and `minecraft:blast_furnace`

Pointer: `AbstractFurnaceBlockEntity`.

Reinterpreted, on the block-entity compound:

- `cooking_time_spent`: short in 4903, int in 5023. Ticks spent on the current cook.
- `cooking_total_time`: short in 4903, int in 5023. Total ticks the current cook takes. A furnace that had been lit still had `200`.
- `lit_time_remaining`: short in 4903, int in 5023. Burn time left.
- `lit_total_time`: short in 4903, int in 5023. Burn time added by the last fuel. The same furnace still had `1600`.

Added, on the block-entity compound:

- `speed_multiplier`: float. Scales the next cook. Fresh furnaces in the 5023 world had `1.0`, including furnaces whose time fields were `0`.

`Items`, `RecipesUsed`, `id`, `x`, `y`, `z`, `keepPacked`, and `components` were already present in 4903.

### `block_entities[]` for `minecraft:brewing_stand`

Pointer: `BrewingStandBlockEntity`.

Reinterpreted, on the block-entity compound:

- `BrewTime`: short in 4903, int in 5023. Ticks the current brew has already taken.
- `Fuel`: byte in 4903, int in 5023. Fuel left. The sample stand still had `3`.

Added, on the block-entity compound:

- `total_brew_time`: int. Total ticks the current brew takes. The sample had `400`.
- `total_fuel`: int. Fuel added by the last refill. The sample had `20`.
- `speed_multiplier`: float. Scales the next brew. The sample had `1.5`.

### `block_entities[]` for `minecraft:decorated_pot`, field `sherds`

Pointer: `DecoratedPotBlockEntity`. The list-to-compound upgrade is `PotDecorationsBlockEntityUnflatteningFix` (schema 4996, sub-version 1). Do not run that fix in the chunk codec.

Reinterpreted, on the block-entity compound:

- 4903: `sherds` is a list of four strings, item ids, in order `back`, `left`, `right`, `front`. The sample list was `minecraft:angler_pottery_sherd`, then `minecraft:brick`, `minecraft:brick`, `minecraft:brick`.
- 5023: `sherds` is a compound of optional item stacks named `back`, `left`, `right`, and `front`. A missing side is omitted. The sample kept `back` and `front` and omitted `left` and `right`. Each saved stack was a compound whose only field was `id` (string): `back` was `minecraft:angler_pottery_sherd`, `front` was `minecraft:brick`. `count` was absent.

Copy whichever shape is on the compound. A 4903 list must still load. A 5023 compound must round-trip.

### `block_entities[]` for `minecraft:sign` and `minecraft:hanging_sign`

Pointer: `SignBlockEntity`. `SignsOpFlagFix` (schema 5002) sets the new field when an old sign is upgraded. Do not run that fix.

Added, on the block-entity compound:

- `allow_op_features`: boolean. Absent means false. A sign placed in a fresh 5023 world omitted the field. The fixer writes true for a sign upgraded from an older save. That upgraded case was not in the sample world.

`front_text`, `back_text`, and `is_waxed` were present in both versions. Leave them inside the copied compound.

### `Entities[]` on the entity chunk, field `invulnerable_time`

Pointer: `Entity`. The entity-chunk container itself did not change. See below.

Added, on each entity compound:

- `invulnerable_time`: int, ticks of temporary invulnerability. Written only when the value is greater than zero. Absent means zero. `Invulnerable` stays a byte (a boolean) and still means permanent invulnerability. The sample pig had `Invulnerable` `1` and no `invulnerable_time`, because the timer had expired before the save.

The numeric type is from `Entity`'s save path (`putInt` / `getIntOr` with default `0`), not from a non-zero value in a region file.

New entity ids in this version (`minecraft:poplar_boat`, `minecraft:poplar_chest_boat`, `minecraft:cushion`) are ids inside an entity compound. They are not new fields of the entity-chunk container. Copy the compound.

## Checked, no loader change

Full chunks in both worlds had the same root keys: `DataVersion`, `xPos`, `zPos`, `yPos`, `Status`, `LastUpdate`, `InhabitedTime`, `isLightOn`, `structures`, `sections`, `block_entities`, `block_ticks`, `fluid_ticks`, `PostProcessing`, `Heightmaps`. `DataVersion` was `4903` or `5023`. `Status` on those chunks was `minecraft:full`. Scheduled tick compounds still use `i`, `x`, `y`, `z`, `t`, and `p`. Section light is still `BlockLight` and `SkyLight` byte arrays. `structures` still has `References` and `starts`.

### `Status` (already accepted)

`MergeTerrainChunkStatusFix` (schema 5013) rewrites a protochunk status. `minecraft:carvers` becomes `minecraft:terrain`. `minecraft:noise` and `minecraft:surface` become `minecraft:biomes`. Any other status is left as written. `below_zero_retrogen.target_status` uses the same rewrite. When the old status, or that target, is `minecraft:noise` or `minecraft:surface`, the fixer also removes `Heightmaps` and `blending_data`, and it removes `block_states` from sections whose `Y` byte fails its keep test. Pumpkin already maps `minecraft:terrain`, `minecraft:noise`, `minecraft:surface`, and `minecraft:carvers` to the terrain status, and it writes `minecraft:terrain`. Do not implement this fixer. The sample worlds had no protochunk, so the strip was not seen on disk.

### `carving_mask` (removed, already ignored)

Removed from `SerializableChunkData` in 26.3. The 26.2 class still names the field. Neither full chunk contained `carving_mask` or `carvingMask`. Pumpkin does not read or write it. Leave that as it is.

### `Heightmaps` (same keys)

Both full chunks saved the same four long arrays: `OCEAN_FLOOR`, `MOTION_BLOCKING_NO_LEAVES`, `MOTION_BLOCKING`, `WORLD_SURFACE`. `WORLD_SURFACE_WG` and `OCEAN_FLOOR_WG` were not present. Pumpkin already reads and writes the three motion and world-surface keys. `OCEAN_FLOOR` staying unread is older than 4903. The 26.3 heightmap block tags change which blocks count. They do not change the saved keys.

### `blending_data.heights` (already dropped)

`BlendingDataNoValueHeightFix` (schema 5009) rewrites each element of `blending_data.heights` to a float. The empty-height sentinel, the largest double (`1.7976931348623157e308`), becomes the largest float (`3.4028235e38`). Any other number is narrowed to a float. `SerializableChunkData` still has `blending_data` in 26.3. Neither full chunk contained the compound. Pumpkin stores no blending data on a chunk it loads and does not write the compound. Do not add a reader. If a later path keeps the compound, the heights are floats.

### Entity-chunk container (unchanged)

Pointer: `EntityStorage`. Both versions saved `DataVersion` (int), `Position` (int array of length 2, chunk x then chunk z), and `Entities` (list of compounds). Pumpkin already reads `Position` as that array, or the older `Position-X` and `Position-Z` ints, and it writes the array. No container field was added or renamed.

### POI (unchanged)

Pointers: `PoiSection` packed form and `PoiRecord` packed form. Both versions saved `DataVersion` (int, `4903` or `5023`) and `Sections`, a compound keyed by the section `Y` as a string (`6` in the sample). Each section has `Valid` (byte, `1` in the sample) and `Records` (list of compounds). Each record has `pos` (int array of length 3, for example `[4, 100, 0]`), `type` (string, for example `minecraft:armorer`), and `free_tickets` (int). No record field was added or renamed. POI is outside `chunk/format`. Pumpkin's POI writer uses separate `x`, `y`, and `z` ints and an older data version. That mismatch is older than 4903. A-48 does not change it.

Other fixes between 4903 and 5023 are item components, map items, or `level.dat`. They are not chunk, entity-chunk, or POI fields. Do not implement them here. That includes pot-decoration components, explorer-map renames, the debug-profile `post_effect` rename, and the hardcore spectator file fix.

## What A-48 implements

In `crates/pumpkin-world/src/chunk/format/` only:

- Read a 4903 palette (`Name`, optional `Properties`) and a 5023 palette (a string list, or a compound list of `id` / `properties` and empty-named default entries).
- Write a 5023 palette, using the string form for a default state and the compound form otherwise, including the empty-named default entry when the list is mixed.
- Round-trip block states. A synthetic chunk with a default state (air or stone) and a non-default state (oak stairs facing east) must load back as those states. A 4903-shaped palette must still load. No Mojang region file goes in git.
- Keep copying `block_entities` and entity compounds. A 4903 chunk that omits `speed_multiplier`, `total_brew_time`, `total_fuel`, `allow_op_features`, or `invulnerable_time` still loads. A 5023 compound that has those tags keeps their types across a round trip.

A-48 does not upgrade sherd lists, furnace shorts, or chunk statuses, and it does not start reading `carving_mask`, `blending_data`, or POI.

## How this was checked

Official 26.2 and 26.3 server jars (data versions 4903 and 5023). Field names and tag types above were read from the saved overworld region, entity, and POI files after `save-all`, and from the jar constant pools and bytecode (`javap`). Vineflower emitted no Java for these class files, so nothing here is copied from decompiled source. The wiki page cited above matches the palette rename, the block-entity type changes, the sign flag, the pot `sherds` shape, and `invulnerable_time`.

Not seen on disk:

- A non-zero `invulnerable_time`. The type and the "omit when zero" rule are from `Entity`'s save and load bytecode.
- `allow_op_features` set to true on a sign upgraded from an older world. A newly placed 5023 sign omitted the field. The fixer writes true.
- A protochunk (`noise`, `surface`, `carvers`, `terrain`) and a chunk that actually contains `blending_data` or `carving_mask`.
- A decorated-pot sherd stack with `count` or other item fields. The saved stacks had only `id`.
- A non-zero `BrewTime`, `cooking_time_spent`, or `lit_time_remaining` after the servers ticked. Those fields were saved as `0` of the new integer type. The non-zero fields that did survive (`Fuel`, `total_brew_time`, `total_fuel`, `speed_multiplier`, `cooking_total_time`, `lit_total_time`) are what pin the types.
