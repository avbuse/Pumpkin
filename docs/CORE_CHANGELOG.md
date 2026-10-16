# Core-functionality changelog

What has landed on `core-functionality` on top of upstream `master`, newest first. Each merge of a task branch adds one entry here in the same merge commit. Task details are in `docs/tasks/<ID>.md`. Playtest steps are in `docs/playtest/<ID>.md`.

## How to add an entry

When you merge `core/<ID>-name` into core:

1. Add a line under **Unreleased**: `- **<ID>** <what changed for a player or admin>. Merge \`<sha>\`. Playtest: done / pending / n/a.`
2. Move the task out of **Waiting to merge**.
3. Don't carry the branch's root `TestPlan.md` into core. Drop it in the merge commit (`git rm -f TestPlan.md` before committing) and keep the plan at `docs/playtest/<ID>.md`.

## Unreleased

### 2026-10-15

- **Upstream sync.** Merged Pumpkin-MC `master` up to `5b53f064`. That includes upstream `b0c48b2d`, which changes worldgen output, so A-45's baseline needs re-recording. Merge `f453f24a`.

### 2026-10-14

- **A-16** Sleeping in a bed now sets the spawn point, in vanilla order: range check, obstruction, set spawn, then the night and monster checks. So a bed sets the spawn even when you can't sleep yet. Merge `6b0d3754`. Playtest: `docs/playtest/A-16.md`.

### 2026-10-11

- **A-05** Time, weather, the world border, gamerules and the scoreboard are restored after a restart. Level files are written once, and border lerps are sent in ticks. Merge `afb0d2be`.
- **A-36** `pumpkin-gametest` redstone order fixture. Merge `43781cf6`, then `70a0b846`.
- **B-01** `/tps` reports MSPT percentiles and tick-phase timings. Merge `50a0a2e6`.
- **A-01** The compat harness has a named scenario registry. Merge `2a4f08e5`.
- **A-08** Notes on chunk field changes from data version 4903 to 5023 (`docs/compat/CHUNK_FORMAT_5023.md`). Docs only. Merge `ff08f50d`.
- **A-10** Particle status in the client information packet is read and written. Commit `217b6409`.
- **A-09** Keep-alives are answered during configuration, so clients that send one before finishing configuration no longer get stuck. Merge `f8827896`.

### 2026-10-10

- **A-04** Worlds with data version 5023 (26.3) are accepted. **Known issue:** the 5023 chunk format changes are not applied yet. Pumpkin reads vanilla 26.3 chunks as mostly air, and vanilla 26.3 reads Pumpkin-saved chunks as mostly air (31,759 of 98,304 blocks in testing). A-48 fixes both directions. Commit `c6f540cb`.
- **Compat harness.** Gap inventory (`docs/compat/GAP_ANALYSIS.md`), login smoke test, roadmap, task list and branching rules. Commits `7e045045`, `e26484b2`, `96f92302`.

## Waiting to merge

Status as of the 2026-10-16 review. Each branch has the full notes in `docs/review/<ID>.md`.

| Task | Branch tip | Status |
| --- | --- | --- |
| A-48 Apply the 5023 chunk field changes | `fca1d677` | **Ready. Merge first.** Fixes the A-04 known issue. Checked against the 26.3 server jar in both directions. |
| A-50 Scoreboard and team display names as components | `f2500689` | Ready. |
| A-53 Save player files atomically | `3a4be7c5` | Works. A periodic save can overwrite a newer logout save, and the leave save blocks a Tokio worker. Merge before or with A-06. |
| A-06 Player health, inventory, spawn and gamemode | `1336718a` | Needs fixes; see notes. |
| A-13 Axe strip, shovel path, hoe till | `fb3b7dea` | The shield-in-off-hand check is missing. |
| A-20 Chest and barrel inventories persist | `e9b24790` | Fixes the autosave gap, but autosave now marks every chunk with a block entity as dirty. |
| A-24 Melee damage and attack cooldown | `e0bbc3e4` | The Sweeping Edge fix is good. Remove `sync_mining_efficiency`. Armour enchantment attributes become active. |
| A-37 Dust power cases | `299e014b` | Say where the expected values come from. |
| A-42 Predicate subset | `f7dede10` | Wire up `predicate_lookup`, or datapack loot that uses predicate references stops dropping. |
| A-45 Worldgen mismatch baseline | `89edc2bf` | Re-record after the upstream sync before merging. |
| docs/human-test-plan | `fc94dbf8` | Refresh the branch table first. |

## Known issues on core

- Chunks don't round-trip with vanilla 26.3 until A-48 is merged (see A-04 above).
- The Spelling CI job fails on two false positives: a compression crate name and a Creative Commons licence abbreviation. Both need entries in `typos.toml`.
