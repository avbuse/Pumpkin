# Test plan: A-24 melee

Branch `core/A-24-melee`. Confirm this branch before it is merged into `core-functionality` `70a0b846e`.

Survival. No Strength or Weakness. Difficulty does not change player-to-mob melee damage. A wooden sword's full-charge hit is 4 damage (health 20 down to 16). The attack bar fills in about 13 ticks at 20 ticks per second. An immediate second click inside the hurt flash plays the no-damage sound and does not change health. A hit before the bar is full deals less than a full hit. Critical hits are a falling hit, not sprinting, not in water, not blind, not climbing, not riding, and multiply the sword damage by 1.5. Sweep requires a sword, a full bar, on the ground, not sprinting, and not a critical hit. Sweep damage with no Sweeping Edge ratio is 1. Marker armour stands are skipped. A sprint hit adds knockback, plays the knockback sound, and stops the sprint. Leather chest armour (3 points, 0 toughness) turns a 4-damage hit into 3.84, so health ends at 16.16. Sharpness I adds 1 at full charge (20 down to 15). A critical hit with Sharpness I ends at 13 and shows both critical particles. Switching the held item resets the attack bar. A landed hit adds a small hunger exhaustion; one hit will not show a lost shank. The attacker takes no damage.

Sweeping Edge is the merge gate. Current core applies Sweeping Edge in the old attack code. This branch reads only the `sweeping_damage_ratio` attribute, and nothing applies the enchantment's attribute, so the ratio stays 0 and the sweep stays 1. That is a regression. **Do not merge this branch while test 8 fails.**

26.3 mob equipment is the `equipment` compound. `ArmorItems` is not read.

Record one continuous session.

## Setup

Run this branch from a scratch directory so the server writes `pumpkin.toml` and `world/` there:

```bash
mkdir -p /tmp/pumpkin-A-24 && cd /tmp/pumpkin-A-24
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

- [ ] **1. Full-charge wooden sword** (Client)

  `gamemode survival`, `give @s wooden_sword`, `summon zombie ~ ~1 ~ {Health:20f,NoAI:1b}`. Wait until the attack bar is full. Hit the zombie once.
  Expect one strong hit sound. `data get entity @e[type=zombie,limit=1,sort=nearest] Health` reports 16. You take no damage.
  **Fail:** health other than 16, or you lose hearts.
  **Evidence:** the Health line and the hit.

- [ ] **2. Spam click** (Client)

  Hit the same zombie again immediately, then keep clicking until the bar is only partly full and the zombie can be hurt again.
  Expect the click inside the hurt flash to play the no-damage sound and leave Health unchanged. The next hit, before the bar fills, reduces Health by less than 4.
  **Fail:** the immediate click deals damage, or a partial-bar hit deals 4 or more.
  **Evidence:** Health before and after the partial hit.

- [ ] **3. Critical hit** (Client)

  Wait for a full bar. Jump and hit the zombie while falling. Do not sprint.
  Expect the critical-hit sound and particles. Health drops by 6, not 4.
  **Fail:** no crit particles, or the drop is 4.
  **Evidence:** the Health delta and the crit particles.

- [ ] **4. Sweep, and a marker armour stand** (Client)

  `summon zombie ~1 ~1 ~ {Health:20f,NoAI:1b}` so two zombies are within a block of each other. `summon armor_stand ~ ~1 ~ {Marker:1b,Health:20f}` in that same space. Stand on the ground, do not sprint, wait for a full bar, and hit one with the wooden sword.
  Expect the sweep sound and the sweep particle. The other zombie loses 1 health. The armour stand stays at 20. You take no damage.
  **Fail:** the second zombie loses 0 or more than 1 on an unenchanted sword, or the marker stand takes damage.
  **Evidence:** both Health values and the armour stand's Health.

- [ ] **5. Sprint knockback** (Client)

  Sprint into a zombie and hit it with a full bar.
  Expect the zombie knocked back farther than a standing hit. You hear the knockback sound and the strong-hit sound, and you stop sprinting.
  **Fail:** you keep sprinting, or there is no extra knockback.
  **Evidence:** a clip of the sprint hit.

- [ ] **6. Leather chest** (Client)

  `summon zombie ~ ~1 ~ {Health:20f,NoAI:1b,equipment:{chest:{id:"minecraft:leather_chestplate",count:1}}}`.
  The zombie must be wearing a leather chest. If it is not, the summon NBT did not apply and this step is not run.
  Full bar, standing hit, wooden sword.
  Expect Health about 16.16, not 16. The chest stays equipped.
  **Fail:** Health is 16, or the chest disappears.
  **Evidence:** the Health line and the worn chest.

- [ ] **7. Sharpness** (Client)

  `give @s wooden_sword[enchantments={sharpness:1}]`. The tooltip must show Sharpness I. If it does not, `give @s wooden_sword` and `enchant @s sharpness 1` while holding it.
  Full bar, standing hit on a fresh 20-health zombie. Then a falling critical hit on another fresh zombie.
  Expect the standing hit to drop Health from 20 to 15. The critical hit drops it from 20 to 13, with both the critical and the magic-critical particles.
  **Fail:** 16 or 14, which means Sharpness was not applied.
  **Evidence:** both Health lines.

- [ ] **8. Sweeping Edge III** (Client)

  Hold a wooden sword and `enchant @s sweeping_edge 3`. The tooltip must show Sweeping Edge III.
  Two fresh `NoAI` zombies within a block of each other, both at 20. Stand on the ground, do not sprint, full bar, hit one.
  Expect the second zombie to lose **more than 1** health.
  **Fail:** the second zombie loses exactly 1. That is the current code (the ratio stays 0). It is a regression against `core-functionality`, which still applies Sweeping Edge. The branch is not ready to merge.
  **Evidence:** the second zombie's Health. Write the number even when it fails.

- [ ] **9. Item swap resets the bar** (Client)

  Full bar with the sword. Switch to another hotbar slot and back.
  Expect the attack bar to empty and fill again.
  **Fail:** the bar stays full across the swap.
  **Evidence:** a clip of the bar.

- [ ] **10. Creative still hits, and does not take damage** (Client)

  `gamemode creative`. Full-charge sword hit on a 20-health zombie.
  Expect the zombie to take damage. You take none.
  `gamemode survival` when you finish.
  **Fail:** the zombie is immune, or you lose hearts.
  **Evidence:** the Health line and your unchanged hearts.

A mace smash (fall more than one and a half blocks, not elytra) is in this branch's combat code. A standing mace hit and a smash from about four blocks up should differ, and the smash plays the mace sound. Record it if you have time. A missing mace clip does not block the sword checks. Spear cooldown uses the same 20-ticks-per-second clock: a full spear hit hits harder than an immediate jab. That is a side check, not a merge gate.

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

This branch is step 7, last. Do not merge it while test 8 fails. It shares `player.rs` with A-06. Merge A-06 first, rebase this branch onto it, and keep both `attack` and A-06's save/load. After that rebase, test 1 still ends at health 16, test 8 still loses more than 1, and `stop` then rejoin still has the sword in the hotbar.
