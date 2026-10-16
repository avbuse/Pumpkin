# Human test plan

Testers confirm each unmerged task branch before it is merged into `core-functionality` `f453f24a` (`Merge branch 'Pumpkin-MC:master' into core-functionality`). A-16 bed spawn is already on that core (`6b0d37545`). Do not treat `core/A-16-bed-spawn` as an open task.

Use a licensed Java Edition **26.3** client. Protocol target on this tree is 26.3. Console-only steps are marked **Console**. Anything that needs the client is marked **Client**.

Check out one branch at a time, in its own scratch directory. Do not merge the branches together until the merge-order section.

The plan on each task branch is `docs/playtest/<TASK>.md`. This file is the combined list. The steps match.

Each open branch tip includes a merge of `core-functionality` `f453f24a`. Follow `docs/playtest/<TASK>.md` on the branch if this combined list and that file ever disagree.

| Branch | Code | Checklist | Tip |
| --- | --- | --- | --- |
| `core/A-06-player-data` | `136e748bd` | `4b0865413` | `ce1e406b7` |
| `core/A-16-bed-spawn` | `1650c9413` | `ed2bfe6e4` | merged as `6b0d37545` |
| `core/A-20-container-persist` | `cf0f3dfd8` | `a37f7ce13` | `c64cc09ac` |
| `core/A-24-melee` | `b88bccbea` | `c2c0ea980` | `b26788191` |
| `core/A-42-predicates` | `62f37b12a` | `6744fae3d` | `faf16d98c` |
| `core/A-45-worldgen-baseline` | `c2a34dcdc` | `62b41b555` | `45f97dcda` |
| `core/A-48-chunk-5023` | `9af839d07` | `cfe488f87` | `fd7738efe` |

## Skipped

`core/A-10-particle-status` (`462ec525c`) is not an ancestor of `core-functionality`, but the same change is already on it as `217b6409f` (`fix(protocol): read and write client information particle status`). `git range-diff` of the two commits shows only context from the rebase (imports in `tools/pumpkin-compat`). There is nothing left to playtest on that branch.

## How to run a branch

From a scratch directory, so the server writes `pumpkin.toml` and `world/` there:

```bash
git checkout <branch>
mkdir -p /tmp/pumpkin-<task> && cd /tmp/pumpkin-<task>
cargo run --manifest-path <repo>/Cargo.toml -p pumpkin
```

Debug builds only. The release profile is not required for these checks.

Wait until the log says `Started server; took`. The next line is `Server is now running. Connect using port: Java Edition: 0.0.0.0:25565`. Those two lines were confirmed on a debug boot of `core-functionality`. Stop with `stop` in that console. `latest.log` in the scratch directory is the same log.

A fresh boot enables three datapacks: `vanilla`, `pumpkin-unit-test`, `pumpkin-unit-test-example`. That list is normal on this tree. It is not the predicate count from A-42.

Defaults that the steps rely on, from the generated `pumpkin.toml`:

- `default_gamemode = "Survival"`, `force_gamemode = false`. Leave `force_gamemode` false or a gamemode save will look like it failed.
- `default_level_name = "world"`. The level is the `world/` directory next to the binary's working directory. `level.dat` sits directly inside it.
- Java port `25565`, `online_mode = true`, `encryption = true`. A licensed 26.3 client joins with those defaults.
- `spawn_protection = 16`. From the server console, `op <playername>` before breaking or placing blocks. `op` sets permission level 4.
- `allow_nether = true`, `allow_end = true`.
- `[world] autosave_ticks = 6000` (5 minutes at 20 TPS). A-20 lowers this.
- `[player_data] save_player_data = true`. Player files are `world/players/data/<uuid>.dat` (hyphenated UUID, gzip NBT). That is not the vanilla `playerdata/` folder.

Each step below has a checkbox. Record pass, fail, or not run, plus the evidence named on the step. A fail is the condition under **Fail**. A crash, a panic line, or a log error that names the thing you just did is also a fail.

## A-06 player data

Branch `core/A-06-player-data`. Code `136e748bd` (`fix(player): write the vanilla respawn compound`). Checklist `4b0865413`. Plan `docs/playtest/A-06.md`.

A survival player who logs out comes back with the same health, hunger, inventory, spawn point, and gamemode. The branch writes the 26.3 `respawn` compound (`dimension`, `pos`, `yaw`, `pitch`, `forced`). Yaw is wrapped into -180..180 on write (270 becomes -90, 180 becomes -180). Pitch is clamped to -90..90 (120 becomes 90). Legacy `SpawnX` / `SpawnY` / `SpawnZ` / `SpawnAngle` / `SpawnDimension` / `SpawnForced` are read from old files and are not written. Old Pumpkin files that stored `respawn.angle` still load that value as yaw.

Death item drops are existing behaviour and are not this task. `previousPlayerGameType` is still not updated by `/gamemode`. That is a separate gap. Do not fail this branch because F3+F4 has nothing to switch back to.

Use a new world. One player, opped. Leave `hardcore` false. Upstream `#4041` sets a dead hardcore player to spectator after respawn, and A-06 then saves that gamemode. A hardcore world will not match A-06.4 and A-06.5.

- [ ] **A-06.1 New player** (Client)

  Join. Expect full health, a full hunger bar, an empty inventory, survival: no flying, blocks break at survival speed.
  **Fail:** creative flight, empty hearts, or a pre-filled inventory on a brand new world.
  **Evidence:** screenshot of the HUD and hotbar on first join.

- [ ] **A-06.2 Inventory, armour, off-hand, ender chest, hotbar slot** (Client)

  `give @s diamond 3`, `give @s iron_helmet`, `give @s emerald`. Wear the helmet. Put one diamond in the off-hand. Put the emerald in an ender chest. Select hotbar slot 4 (the fifth slot).
  Disconnect, `stop`, start the same directory, rejoin.
  Expect three diamonds, one of them in the off-hand, the helmet still worn, the emerald still in the ender chest, and slot 4 still selected.
  **Fail:** any of those stacks missing, moved, or the selected slot reset to 1.
  **Evidence:** screenshots of the inventory and the ender chest before `stop` and after rejoin.

- [ ] **A-06.3 Health and hunger** (Client)

  `damage @s 7`. Expect the health bar to drop by three and a half hearts. Sprint and jump until the hunger bar loses at least one shank. `data get entity @s Health` and `data get entity @s foodLevel`. Write both numbers down.
  Disconnect, `stop`, start, rejoin. Run the same two `data get` commands.
  Expect the same health and the same food level. Hearts stay short of full if you left them short. You still cannot fly.
  **Fail:** health back at 20, food back at 20, or the player is creative.
  **Evidence:** the four `data get` lines, two before the restart and two after.

- [ ] **A-06.4 Respawn compound, yaw wrap, pitch clamp** (Client)

  Stand on a block you can recognise, away from world spawn. Look slightly downward. `spawnpoint @s ~ ~ ~ 270 120`.
  The command feedback may print yaw 270 and pitch 120. That is the value you typed.
  `data get entity @s respawn`.
  Expect a compound with `dimension`, `pos` (the block you are standing on), `yaw` -90, `pitch` 90, and `forced` true. Expect no `angle` tag and no `SpawnX` / `SpawnY` / `SpawnZ`.
  Disconnect, `stop`, start, rejoin. `data get entity @s respawn` again. Same compound.
  `kill @s`. Expect to respawn on that block, facing the same horizontal direction as yaw 270 (west), looking steeply up (pitch 90). Item drops on the ground are not a fail for this task.
  **Fail:** `yaw` left at 270, `pitch` left at 120, the compound missing, legacy `SpawnX` written, respawn at world spawn, or the facing after death level with the horizon.
  **Evidence:** both `data get` transcripts and a screenshot of the respawn spot, including the facing.

- [ ] **A-06.5 Gamemode and flight** (Client)

  `gamemode adventure`. Disconnect, `stop`, start, rejoin.
  Expect adventure: you cannot break or place blocks, and you cannot fly. `data get entity @s playerGameType` is 2.
  `gamemode creative`. Disconnect, `stop`, start, rejoin.
  Expect creative, including flight. `playerGameType` is 1.
  `gamemode survival` before you leave the world.
  **Fail:** the mode resets to survival across a restart, or creative returns without flight.
  **Evidence:** the two `playerGameType` lines and a short clip of adventure failing to break a block, then creative flight.

- [ ] **A-06.6 Missing player file** (Console, then Client)

  `stop`. Delete only that player's file under `world/players/data/` (the `<uuid>.dat` written when they left). Do not delete `world/players/advancements/`. Start and rejoin.
  Expect a new survival player: full health, full hunger, empty inventory, no personal spawn (death sends you to world spawn), survival.
  **Fail:** the old inventory or the old spawn comes back after the file is gone.
  **Evidence:** directory listing showing the file removed, then the fresh HUD.

- [ ] **A-06.7 Clean stop must not blank the file** (Client)

  Put one dirt block in the hotbar, `stop`, start, rejoin.
  Expect the dirt still there. The log must not contain `Failed to read player data` on this clean stop.
  **Fail:** a clean `stop` loads you as a new player.
  A hard kill during the logout save can truncate the `.dat`, because the file is written in place. If you only see the fresh-player result after `kill -9`, record it as the known in-place write gap and do not treat it as an A-06 field failure. A clean `stop` that does this is a fail.
  **Evidence:** the rejoin inventory and the log around startup.

- [ ] **A-06.8 Daytime bed spawn survives stop** (Client)

  A-16 is already on `core-functionality`, and this branch includes that core. Day, clear weather, standing next to an unoccupied bed. Right-click it.
  Expect the action-bar line that you can only sleep at night, and a chat line that the respawn point was set. You stay standing.
  `data get entity @s respawn`.
  Expect `pos` on the bed head, `yaw`, `pitch`, and `forced` false. Expect no `angle` tag.
  `stop`, start, rejoin. `data get entity @s respawn` again. Same compound.
  `kill @s`. Expect to respawn at that bed.
  **Fail:** the compound uses `angle` and omits `pitch`, the bed spawn is gone after `stop`, or death sends you to world spawn.
  The A-16 section below says pitch is absent. That is the A-16 branch on its own. It does not apply on this A-06 tip.
  **Evidence:** both `data get` transcripts and a screenshot of the respawn spot.

## A-16 bed spawn

Branch `core/A-16-bed-spawn` is already merged into `core-functionality` as `6b0d37545`. The steps below describe that branch on its own. Do not playtest the old branch tip. Use A-06.8 on `core/A-06-player-data` for the bed plus the saved `respawn` compound.

Code `1650c9413` (`fix(block): set the bed spawn before the night and monster checks`). Checklist `ed2bfe6e4`. Plan `docs/playtest/A-16.md`.

Right-click order on an overworld bed: explosion in the Nether and the End, then occupied, then too far, then a solid block above the bed, then the respawn write, then the night check, then the monster check. A daytime click and a monster click still set the spawn when the bed is in range and clear. Too far, obstructed, occupied, and an explosion do not.

Reach is 3 blocks horizontally from the centre of either half, and 2 blocks vertically from the bed block's y, using the player's feet. The monster box is 8 blocks horizontally and 5 vertically around the **head** only, and it tests the mob's bounding box. Slimes, magma cubes, ghasts, phantoms, and shulkers do not block sleep. A zombified piglin blocks only while it is angry at you. Creative skips the monster check. Survival and adventure do not.

This branch does not change player NBT. `data get entity @s respawn` on this branch alone still shows the current core compound (`pos`, `angle`, `forced`), plus the legacy `SpawnX` keys. `pitch` arrives with A-06. Do not fail A-16 for a missing `pitch`.

Messages, action bar unless noted:

- `You can sleep only at night or during thunderstorms`
- `You may not rest now; there are monsters nearby`
- `This bed is obstructed`
- `This bed is occupied`
- `You may not rest now; the bed is too far away`
- chat, only when the spawn actually changes: `Respawn point set`

The night line mentions thunderstorms. Sleep is gated on darkness (`time set night`), not on a thunderstorm by itself. A day-plus-thunder check is not a pass/fail for this branch.

Death and the respawn screen are A-27. Confirm the stored position with `data get`, not by dying, unless you are on the combined smoke after A-06 is merged.

New overworld, survival, opped. `time set day` and `weather clear` to start. Beds are white unless a step says otherwise.

- [ ] **A-16.1 Daytime click sets the spawn and does not start sleep** (Client)

  Stand next to an unoccupied bed during the day. Right-click it.
  Expect the action-bar night line and a chat line `Respawn point set`. You stay standing. The bed does not look occupied.
  `data get entity @s respawn`. `pos` is the bed **head** block. Write it down.
  **Fail:** you lie down, the bed shows occupied, there is no chat line, or `pos` is missing.
  **Evidence:** recording of the action bar, the chat line, and the `data get` output.

- [ ] **A-16.2 Night, monster beside the head** (Client)

  `time set night`. Place a second bed. `summon minecraft:zombie ~ ~ ~ {NoAI:1b}` so the zombie stands on or next to that bed's head, inside 8 blocks horizontally and 5 vertically of the head. Right-click the bed.
  Expect the monsters action bar and chat `Respawn point set`. You do not lie down. The bed stays unused.
  `data get entity @s respawn` now has this bed's head, not the bed from A-16.1.
  **Fail:** you lie down, or the spawn `pos` stays on the first bed.
  **Evidence:** recording of the action bar, the chat, and the zombie on the head.

- [ ] **A-16.3 Same bed again does not repeat the chat line** (Client)

  `kill @e[type=zombie]`. With the night still set and no monster in the head box, right-click the same bed.
  Expect to lie down and the bed to look occupied. Chat does not repeat `Respawn point set`.
  Leave the bed (sneak, or `time set day` and wait). You stand beside the bed and the bed is free. No extra chat line.
  **Fail:** a second `Respawn point set`, or the bed stays occupied after you get up.
  **Evidence:** the same recording, continued.

- [ ] **A-16.4 A zombie only beside the foot does not block** (Client)

  `time set night`. Place a bed. Put a `NoAI` zombie next to the foot, far enough that it does not overlap the head's box (about nine blocks from the head along the bed). Right-click.
  Expect to lie down.
  **Fail:** the monsters line while the zombie is only on the foot.
  **Evidence:** screenshot with the zombie visible at the foot and you in the bed.

- [ ] **A-16.5 Mobs that do not block, and an angry piglin** (Client)

  At night, beside a fresh bed, summon one at a time and right-click: slime, magma cube, ghast, phantom, shulker. Expect to lie down each time. Get up between tries.
  Summon a zombified piglin. Leave it calm and right-click: you lie down. Get up. Hit the piglin so it is angry at you, and right-click again: monsters line, you stay standing, and chat sets the spawn if this is a new bed.
  **Fail:** a slime, magma cube, ghast, phantom, or shulker blocks sleep, or a calm zombified piglin blocks sleep.
  **Evidence:** a short clip of the calm piglin letting you lie down and the angry one refusing.

- [ ] **A-16.6 Creative skips the monster check** (Client)

  `time set night`. Zombie on the head. `gamemode creative`. Right-click.
  Expect to lie down.
  `gamemode survival` afterwards.
  **Fail:** creative gets the monsters line.
  **Evidence:** clip of creative lying down with the zombie still there.

- [ ] **A-16.7 Too far wins over a blocked bed, and neither moves the spawn** (Client)

  Note `data get entity @s respawn` `pos`. Place a bed with a solid block on top of both halves. Stand several blocks away, far enough that the bed is past the 3-by-2 reach, and right-click.
  Expect the too-far action bar. `pos` is unchanged.
  Stand next to that same blocked bed and right-click.
  Expect the obstructed action bar. `pos` is still unchanged.
  **Fail:** the obstructed line when you are also too far, or `pos` changes on either click.
  **Evidence:** both action-bar lines and the two `data get` lines.

- [ ] **A-16.8 Daytime with a monster shows the night line** (Client)

  `time set day`. Zombie on the head of a new bed. Right-click from beside it.
  Expect the night action bar, not the monsters line, plus chat `Respawn point set`. You stay standing.
  **Fail:** the monsters line during the day.
  **Evidence:** the action bar with the zombie in frame.

- [ ] **A-16.9 Occupied bed** (Client, second account)

  At night, one player lies down in a bed. The other player right-clicks it.
  Expect the occupied action bar. The second player's `respawn` `pos` does not change.
  **Fail:** the second player lies down, or their spawn moves.
  **Evidence:** both names visible, the action bar, and the second player's `data get`.
  If you only have one account, mark this not run. Do not mark it pass.

- [ ] **A-16.10 Nether bed explodes and does not set the spawn** (Client)

  Note `pos`. `execute in minecraft:the_nether run tp @s 0 80 0`. Place a bed and right-click it.
  Expect an explosion. No `Respawn point set`. `pos` is unchanged when you return: `execute in minecraft:overworld run tp @s <x> <y> <z>` using the coordinates you wrote down.
  The End does the same. One of the two dimensions is enough.
  **Fail:** the bed becomes your spawn, or there is no explosion.
  **Evidence:** clip of the explosion and the `data get` from before and after.

## A-20 container persistence

Branch `core/A-20-container-persist`. Code `cf0f3dfd8` (`fix(world): flush live block entities before autosave`). Checklist `a37f7ce13`. Plan `docs/playtest/A-20.md`.

Closing a chest or barrel only updates the live inventory. The chunk snapshot is written when the chunk unloads, when you `save-all`, when you `stop`, and on the periodic autosave. Before this branch the autosave set the save flag without copying live block entities, so a crash after the autosave line could store an empty chest while the chunk stayed loaded.

The autosave log line is `Saving <N> chunks (collected from <M> holders)...`. There is no separate "autosave" line. `stop` and `save-all` already copied live inventories on current core, so they do not prove this fix. The hard kill after the Saving line does.

The flush is every live block entity in the chunk, not only chests. A furnace in the same chunk is the regression check. Hoppers, shulker boxes, and lock/custom-name tags are not part of the acceptance.

Upstream `#4016` keeps a spectator out of a chest whose loot table has not rolled. These steps use a survival player and a chest you fill yourself. A spectator refusal on an unrolled loot chest is that fix, not an empty save.

- [ ] **A-20.1 Chest window** (Client)

  New world, opped. Place a chest. Put a stack of cobblestone in the first slot. Close it. Open it.
  Expect the cobblestone still in that slot.
  **Fail:** the slot is empty after a close and open with no restart.
  **Evidence:** the open chest.

- [ ] **A-20.2 Double chest** (Client)

  Place a second chest against the first so they join. Put diamonds in a slot on the left half and emeralds in a slot on the right half. Close. Open either half.
  Expect the large double-chest window and both stacks in those slots.
  **Fail:** a single-chest window, or one half empty.
  **Evidence:** the double-chest window.

- [ ] **A-20.3 Stop and start** (Client)

  `stop`, start the same directory, open the double chest.
  Expect both stacks still there.
  **Fail:** either stack missing after a clean `stop`.
  **Evidence:** the chest after the server is back.

- [ ] **A-20.4 Barrel** (Client)

  Place a barrel. Put one stack of oak planks in it. Close, open, then `stop` and start.
  Expect the planks still in that slot.
  **Fail:** the barrel is empty after the restart.
  **Evidence:** the barrel after the restart.

- [ ] **A-20.5 Break one half** (Client)

  Break one chest half. Expect the diamonds and emeralds to drop. Open the barrel again. The planks are still there.
  **Fail:** the drops are missing, or breaking the chest clears the barrel.
  **Evidence:** the drops on the ground and the open barrel.

- [ ] **A-20.6 Chunk unload** (Client)

  Place a chest, put 16 cobblestone in the first slot, close it. Note the coordinates. `gamemode creative` and fly at least 400 blocks away (view distance is 16 chunks). Wait until the chunk can unload, then fly back and open it. `gamemode survival` when you are done.
  Expect the cobblestone still there.
  **Fail:** the chest is empty after you return.
  **Evidence:** F3 coordinates before and after, and the open chest.

- [ ] **A-20.7 Autosave, then a hard kill** (Client)

  `stop`. In `pumpkin.toml`, under `[world]`, set `autosave_ticks = 200` (10 seconds). Start again.
  Place a chest and a furnace in the same chunk. Put 16 cobblestone in the chest and close it. Put one coal in the furnace's fuel slot and close it. Stay in the chunk. Do not run `save-all`. Do not type `stop`.
  Wait for `Saving <N> chunks (collected from <M> holders)...` in the log. Then `kill -9` that server process.
  Start again. Open the chest and the furnace.
  Expect the cobblestone and the coal still there.
  **Fail:** the chest or the furnace is empty after a kill that happened after the Saving line, while you were still standing in the chunk.
  **Evidence:** the Saving line with a timestamp, and both containers after the restart.
  Put `autosave_ticks` back to 6000 when you finish this branch.

- [ ] **A-20.8 Trapped chest** (Client)

  Place a trapped chest. Put one iron ingot in it. Close it. `stop`, start, open it.
  Expect the ingot still there. Redstone output is not part of this check.
  **Fail:** the trapped chest is empty after `stop`.
  **Evidence:** the open trapped chest after the restart.

## A-24 melee

Branch `core/A-24-melee`. Code `b88bccbea` (`docs(review): add A-24 review notes`). The gameplay commits are `38bda99f0` and `5f588c72f`. Checklist `c2c0ea980`. Plan `docs/playtest/A-24.md`.

Survival. No Strength or Weakness. Difficulty does not change player-to-mob melee damage. A wooden sword's full-charge hit is 4 damage (health 20 down to 16). The attack bar fills in about 13 ticks at 20 ticks per second. An immediate second click inside the hurt flash plays the no-damage sound and does not change health. A hit before the bar is full deals less than a full hit. Critical hits are a falling hit, not sprinting, not in water, not blind, not climbing, not riding, and multiply the sword damage by 1.5. Sweep requires a sword, a full bar, on the ground, not sprinting, and not a critical hit. Sweep damage with no Sweeping Edge ratio is 1. Marker armour stands are skipped. A sprint hit adds knockback, plays the knockback sound, and stops the sprint. Leather chest armour (3 points, 0 toughness) turns a 4-damage hit into 3.84, so health ends at 16.16. Sharpness I adds 1 at full charge (20 down to 15). A critical hit with Sharpness I ends at 13 and shows both critical particles. Switching the held item resets the attack bar. A landed hit adds a small hunger exhaustion; one hit will not show a lost shank. The attacker takes no damage.

Sweeping Edge is the merge gate. `core-functionality` `f453f24a` still applies Sweeping Edge in the old attack code. This branch reads only the `sweeping_damage_ratio` attribute, and nothing applies the enchantment's attribute, so the ratio stays 0 and the sweep stays 1. That is a regression. **Do not merge this branch while A-24.8 fails.**

Upstream `#4040` cooks a mob drop when the mob is on fire or the weapon has Fire Aspect. A cooked drop is not an A-24 failure.

26.3 mob equipment is the `equipment` compound. `ArmorItems` is not read.

Record one continuous session.

- [ ] **A-24.1 Full-charge wooden sword** (Client)

  `gamemode survival`, `give @s wooden_sword`, `summon zombie ~ ~1 ~ {Health:20f,NoAI:1b}`. Wait until the attack bar is full. Hit the zombie once.
  Expect one strong hit sound. `data get entity @e[type=zombie,limit=1,sort=nearest] Health` reports 16. You take no damage.
  **Fail:** health other than 16, or you lose hearts.
  **Evidence:** the Health line and the hit.

- [ ] **A-24.2 Spam click** (Client)

  Hit the same zombie again immediately, then keep clicking until the bar is only partly full and the zombie can be hurt again.
  Expect the click inside the hurt flash to play the no-damage sound and leave Health unchanged. The next hit, before the bar fills, reduces Health by less than 4.
  **Fail:** the immediate click deals damage, or a partial-bar hit deals 4 or more.
  **Evidence:** Health before and after the partial hit.

- [ ] **A-24.3 Critical hit** (Client)

  Wait for a full bar. Jump and hit the zombie while falling. Do not sprint.
  Expect the critical-hit sound and particles. Health drops by 6, not 4.
  **Fail:** no crit particles, or the drop is 4.
  **Evidence:** the Health delta and the crit particles.

- [ ] **A-24.4 Sweep, and a marker armour stand** (Client)

  `summon zombie ~1 ~1 ~ {Health:20f,NoAI:1b}` so two zombies are within a block of each other. `summon armor_stand ~ ~1 ~ {Marker:1b,Health:20f}` in that same space. Stand on the ground, do not sprint, wait for a full bar, and hit one with the wooden sword.
  Expect the sweep sound and the sweep particle. The other zombie loses 1 health. The armour stand stays at 20. You take no damage.
  **Fail:** the second zombie loses 0 or more than 1 on an unenchanted sword, or the marker stand takes damage.
  **Evidence:** both Health values and the armour stand's Health.

- [ ] **A-24.5 Sprint knockback** (Client)

  Sprint into a zombie and hit it with a full bar.
  Expect the zombie knocked back farther than a standing hit. You hear the knockback sound and the strong-hit sound, and you stop sprinting.
  **Fail:** you keep sprinting, or there is no extra knockback.
  **Evidence:** a clip of the sprint hit.

- [ ] **A-24.6 Leather chest** (Client)

  `summon zombie ~ ~1 ~ {Health:20f,NoAI:1b,equipment:{chest:{id:"minecraft:leather_chestplate",count:1}}}`.
  The zombie must be wearing a leather chest. If it is not, the summon NBT did not apply and this step is not run.
  Full bar, standing hit, wooden sword.
  Expect Health about 16.16, not 16. The chest stays equipped.
  **Fail:** Health is 16, or the chest disappears.
  **Evidence:** the Health line and the worn chest.

- [ ] **A-24.7 Sharpness** (Client)

  `give @s wooden_sword[enchantments={sharpness:1}]`. The tooltip must show Sharpness I. If it does not, `give @s wooden_sword` and `enchant @s sharpness 1` while holding it.
  Full bar, standing hit on a fresh 20-health zombie. Then a falling critical hit on another fresh zombie.
  Expect the standing hit to drop Health from 20 to 15. The critical hit drops it from 20 to 13, with both the critical and the magic-critical particles.
  **Fail:** 16 or 14, which means Sharpness was not applied.
  **Evidence:** both Health lines.

- [ ] **A-24.8 Sweeping Edge III** (Client)

  Hold a wooden sword and `enchant @s sweeping_edge 3`. The tooltip must show Sweeping Edge III.
  Two fresh `NoAI` zombies within a block of each other, both at 20. Stand on the ground, do not sprint, full bar, hit one.
  Expect the second zombie to lose **more than 1** health.
  **Fail:** the second zombie loses exactly 1. That is the current code (the ratio stays 0). It is a regression against `core-functionality`, which still applies Sweeping Edge. The branch is not ready to merge.
  **Evidence:** the second zombie's Health. Write the number even when it fails.

- [ ] **A-24.9 Item swap resets the bar** (Client)

  Full bar with the sword. Switch to another hotbar slot and back.
  Expect the attack bar to empty and fill again.
  **Fail:** the bar stays full across the swap.
  **Evidence:** a clip of the bar.

- [ ] **A-24.10 Creative still hits, and does not take damage** (Client)

  `gamemode creative`. Full-charge sword hit on a 20-health zombie.
  Expect the zombie to take damage. You take none.
  `gamemode survival` when you finish.
  **Fail:** the zombie is immune, or you lose hearts.
  **Evidence:** the Health line and your unchanged hearts.

A mace smash (fall more than one and a half blocks, not elytra) is in this branch's combat code. A standing mace hit and a smash from about four blocks up should differ, and the smash plays the mace sound. Record it if you have time. A missing mace clip does not block the sword checks. Spear cooldown uses the same 20-ticks-per-second clock: a full spear hit hits harder than an immediate jab. That is a side check, not a merge gate.

## A-42 predicates

Branch `core/A-42-predicates`. Code `62f37b12a` (`fix(datapack): parse predicates as loot conditions`). Checklist `6744fae3d`. Plan `docs/playtest/A-42.md`.

Predicate files are parsed as the same loot conditions the loader already uses. The three embedded vanilla predicates load: `minecraft:tool/can_silk_touch`, `minecraft:tool/can_shear`, `minecraft:block/fast_cooking`. `match_block` is supported, so `fast_cooking` must not warn. Only `data/<namespace>/predicate/` is loaded. `predicates/` is ignored. An unknown type logs a warning and the server stays up.

Nothing in gameplay calls `get_predicate` yet. A-43 and A-44 are the callers. Block breaks still use the generated static loot tables, which already require Silk Touch for glass. A datapack loot table that references a predicate does not resolve it during a block break, because the lookup is unset. Do not expect this branch to change glass drops.

Upstream `#4040` cooks a mob drop when the mob is on fire or dies to Fire Aspect. That uses the static loot condition, not `get_predicate`. It does not change the glass results in A-42.3.

- [ ] **A-42.1 Boot** (Console)

  Start in an empty directory. Wait for `Started server; took`.
  Expect an info line `Loaded 3 embedded predicate(s)`. Expect no warning that skips `minecraft:block/fast_cooking`. The server stays up.
  The separate line `Loaded 3 embedded test instance(s)` is already on core. It is not this check.
  **Fail:** the predicate line missing, a `fast_cooking` skip warning, or a panic.
  **Evidence:** the boot log from `Started server` through the predicate line.

- [ ] **A-42.2 Datapack with one good file, one unknown type, one ignored directory** (Console)

  `stop`. Under the scratch directory create `world/datapacks/predicate_check/` with:

  `pack.mcmeta`

  ```json
  {
    "pack": {
      "description": "predicate check",
      "pack_format": 107
    }
  }
  ```

  `data/test/predicate/shears.json`, copied from `<repo>/assets/datapack/data/minecraft/predicate/tool/can_shear.json` (`minecraft:match_tool` for `minecraft:shears`).

  `data/test/predicate/unknown.json`

  ```json
  { "type": "minecraft:not_a_real_predicate" }
  ```

  `data/test/predicates/ignored.json`, the same shears file under the plural directory.

  Start the server. From the console: `datapack enable "file/predicate_check"` then `reload`.
  If enable says the pack is unknown, `datapack list` and use the name it prints.
  Expect a warning `Skipping predicate 'test:unknown': unknown type 'minecraft:not_a_real_predicate'`.
  Expect no line for `test:ignored` and no line for `minecraft:block/fast_cooking`. The server does not panic. The datapack load line for `predicate_check` includes a predicate count.
  **Fail:** a panic, a warning for `test:ignored` or `fast_cooking`, or no warning for `test:unknown`.
  **Evidence:** the console after `reload`.

- [ ] **A-42.3 Glass drops stay on the current rules** (Client)

  Opped survival player. Place glass. Break it with a normal pickaxe. Place glass again. Break it with a Silk Touch pickaxe (`give @s diamond_pickaxe` and `enchant @s silk_touch`, tooltip must show Silk Touch).
  Expect no glass drop from the normal pickaxe, and a glass drop from Silk Touch. That is the static loot table on current core. This branch must not change it.
  **Fail:** the normal pickaxe drops glass, or Silk Touch drops nothing.
  **Evidence:** both breaks.

- [ ] **A-42.4 Advancements are not granted from predicates yet** (Client)

  Pick up a crafting table (`give @s crafting_table` is enough if pickup is awkward).
  Expect `story/root` not to pop.
  **Fail:** the advancement pops because of this branch.
  **Evidence:** the HUD after the pickup, with no advancement toast. A toast that already existed before this branch is a separate note, not a pass.

## A-45 worldgen baseline

Branch `core/A-45-worldgen-baseline`. Code `c2a34dcdc` (`docs(review): add A-45 review notes`). Checklist `62b41b555`. Plan `docs/playtest/A-45.md`.

Documentation only. No generator, chunk format, or packet change. Terrain matches `core-functionality` at `f453f24a`. `docs/compat/WORLDGEN_BASELINE.md` on the A-45 branch is the measurement taken on `70a0b846e`, before `b0c48b2d7` replaced the suite. Do not compare a new run to those old suite totals.

A higher count than A-45.2 is a regression even when the test still passes under its allowance (noise 6000, surface 7500, carvers 8000, per chunk).

- [ ] **A-45.1 No Rust diff** (Console)

  `git diff --stat core-functionality...core/A-45-worldgen-baseline -- crates/`
  Expect no output.
  **Fail:** any file under `crates/` differs.
  **Evidence:** the empty diff command.

- [ ] **A-45.2 Proto chunk counts** (Console)

  From the repository root, debug profile:

  ```bash
  cargo test -j 2 -p pumpkin-world --lib generation::proto_chunk_test -- --nocapture --test-threads=2
  ```

  This is a long quiet run. A run on the updated A-45 tip finished in 196.46 seconds on 2 threads. A silent few minutes is the test working.
  Expect `46 passed; 0 failed`. The old `43 passed` line and the 8x8 grid suite totals (noise 2, surface 2846, carvers 9122) belong to `70a0b846e`. Those grid tests are gone.
  `count_dump_mismatches` still prints `[<name>] Total mismatches: N` only when `N > 0`. A passing test with no such line is 0. That run printed only:

  - `no_blend_no_beard_carvers_7_4`: 9
  - `no_blend_no_beard_surface_7_4`: 9
  - `no_blend_no_beard_surface_13579_-6_11`: 56
  - `no_blend_no_beard_surface_13579_-7_9`: 101

  **Fail:** any failed test, any of those four counts going up, or a new `Total mismatches` line.
  **Evidence:** the final `passed` line and every `Total mismatches` line.

- [ ] **A-45.3 Boot** (Console)

  Scratch directory, start, wait for `Started server; took`, `stop`.
  Expect no panic.
  **Fail:** a panic or an error this branch added. There should be no new error, because there is no code change.
  **Evidence:** the log from start through `stop`.

- [ ] **A-45.4 Walk around spawn** (Client)

  New world, seed 0 (`seed` in `pumpkin.toml` before the first boot, or delete `world/` and set it). Walk a few chunks around spawn.
  Expect the world to generate and the player to stand on solid ground. Expect no visible difference from `core-functionality`. This walk does not show whether terrain matches vanilla.
  **Fail:** a hole of air where the ground should be, or a crash while chunks generate.
  **Evidence:** optional. A clip of the player on generated ground is enough if you record one. No capture is required to merge a docs-only change.

## A-48 chunk format 5023

Branch `core/A-48-chunk-5023`. Code `9af839d07` (`docs(review): add A-48 review notes`). The format commit is `0c173c4ac`. Checklist `cfe488f87`. Plan `docs/playtest/A-48.md`.

This branch reads three block-state palette shapes and writes 5023:

- a 4903 compound with `Name` and optional `Properties`
- a 5023 list of block-id strings, when every entry is the block's default state
- a 5023 list of compounds, where a default entry is an empty-named string and any other entry is `id` plus `properties`

A default furnace (facing north, unlit) is a default state and is written as the block id. Oak stairs facing east are not. Biomes stay a string palette. Block-entity compounds are copied with their original tag types: furnace cook times, brewing numbers, and pot sherds are not rewritten. An unknown block id becomes air, with no log line. Lighting is copied, not recomputed.

This branch now includes `core-functionality` `f453f24a`, including A-05 time, weather, border, and scoreboard restore. A reset of those after `stop` is a fail. The chunk format file does not overlap the core commits after `50a0a2e6`. The 5023 writer on this branch is still this branch's change.

Worlds outside data version 4435..5023 are rejected with `Unsupported world version`. 26.2 is 4903. 26.3 is 5023. Both are in range. Copy a world. Do not test against the only copy.

`default_level_name = "world"`. The directory that contains `level.dat` must be named `world` and sit in the scratch directory, or you change `default_level_name` to match.

- [ ] **A-48.1 Vanilla 26.3 world opens** (Client)

  Create a world in vanilla 26.3. Remember the seed. Place, somewhere you can find again: oak stairs facing east, a furnace, a stone block, a chest with one diamond, a sign with text, and a decorated pot if you have sherds. Stand where you can see deepslate or dig to it. Stop vanilla so the region files flush.
  Copy that save in as Pumpkin's `world/`. Start this branch. Join with a 26.3 client.
  Expect the terrain you left, not a hollow of air. In the overworld the bottom (`y = -64`) is bedrock. The deepslate band is deepslate. Stone, dirt, grass, sand, and logs are the blocks you remember. The stairs still face east. The furnace is a furnace. The chest, sign, and pot still have their contents and text. Water and lava still sit where you left them.
  **Fail:** stone or deepslate replaced by air, stairs lost their facing, or the chest is empty.
  **Evidence:** the same spot in vanilla before the copy, then in Pumpkin. A short walk from a cave floor to the surface is enough. Deepslate or stone underfoot must be visible.

- [ ] **A-48.2 Edit, save, reopen in vanilla 26.3** (Client)

  In Pumpkin, break one block and place oak stairs facing east, a furnace, and a stone block in a chunk you can find again. Note the coordinates. `stop` and wait until the save finishes.
  Open the same world in vanilla 26.3.
  Expect vanilla to open with no `Recoverable errors when loading section` in its log. Terrain you did not touch is unchanged. The stairs, furnace, and stone are those blocks, with the facing you set. A furnace inventory and sign text you did not edit are still there.
  **Fail:** vanilla reports recoverable section errors, or the placed stairs or furnace are air.
  **Evidence:** the vanilla log around the load, and the same coordinates in vanilla.

- [ ] **A-48.3 A 26.2 world still loads** (Client)

  Create a small world in vanilla 26.2 (data version 4903). Stop it. Copy it in as `world/`. Start this branch and join.
  Expect terrain, bedrock at the bottom, and a placed furnace or stone block to still be that block. Tag upgrades are not required. The block has to be the block.
  **Fail:** the 26.2 world opens as air, or the server rejects it as an unsupported version.
  **Evidence:** one screenshot of solid ground and the furnace or stone you placed in 26.2.

- [ ] **A-48.4 A world saved by current core opens here, then in vanilla** (Client)

  On `core-functionality` `f453f24a`, new world, place oak stairs facing east, a furnace, and stone. `stop`.
  Copy that `world/` onto this branch. Start this branch and join.
  Expect those blocks, not air. Current core writes `Name` / `Properties` palettes labelled 5023, which this branch still reads.
  `stop` on this branch. Open that same world in vanilla 26.3.
  Expect no `Recoverable errors when loading section`. The stairs, furnace, and stone survived. Vanilla reading the world **before** this branch rewrites it is the bug: core's palette is not a 5023 palette. You do not need to demonstrate that failure again unless the rewrite step still produces the recoverable-error line.
  **Fail:** Pumpkin shows air for those blocks, or vanilla logs recoverable section errors after this branch has saved.
  **Evidence:** the blocks in Pumpkin, then the vanilla log and the same blocks.

- [ ] **A-48.5 Boot of a brand new world** (Console)

  Empty scratch directory on this branch. Start, wait for `Started server; took`, `stop`.
  Expect no panic and no `Unsupported world version`.
  **Fail:** a panic while creating or saving the level.
  **Evidence:** the log.

- [ ] **A-48.6 Time survives stop** (Console)

  A-05 is on this branch. `time set 1000`, `stop`, start, `time query daytime`.
  Expect about 1000, plus any ticks that passed before `stop`.
  **Fail:** daytime back at the new-world value. Weather, the world border, and the scoreboard follow the same rule.
  **Evidence:** the two query lines.

## Merge order

File overlap between the open branches: only A-06 and A-24 both change `crates/pumpkin-core/src/entity/player.rs`. A-48's chunk format file does not overlap the core commits after `50a0a2e6`. A-16 is already on `core-functionality`.

Runtime interactions:

- A-16 is already on core. A-06 writes that bed point into the `respawn` compound, including pitch. On the current A-06 tip, a daytime bed click must survive a restart. That is A-06.8.
- A-20 copies live block entities into the chunk snapshot. A-48 is the codec that then writes that chunk. A chest that survives A-20's kill must still be that chest, with its items, after A-48 writes the chunk, including when vanilla opens it.
- A-45 changes no code. `b0c48b2d7` already replaced `proto_chunk_test`. The old suite totals do not apply. Re-run A-45.2 only if a later merge touches `crates/pumpkin-world/src/generation/`.
- A-42 does not change chunk IO or player NBT. Glass drops must stay as in A-42.3 after the other merges.
- A-24's attack path and A-06's NBT path share `player.rs`. Merge A-06 first, then rebase A-24 onto it and keep both: A-24's `attack` and A-06's save/load. A-24.8 (Sweeping Edge) is a fail on the branch as it stands. Rebase after that is fixed, or do not merge A-24.

Suggested order into `core-functionality` `f453f24a`:

1. `core/A-45-worldgen-baseline`. Docs only.
2. `core/A-42-predicates`. Isolated from world save.
3. `core/A-48-chunk-5023`. Chunk codec before the saves you want vanilla to read.
4. `core/A-20-container-persist`. Autosave flush into the chunk A-48 writes.
5. `core/A-06-player-data`. Persists the bed spawn and the rest of the player file.
6. `core/A-24-melee` last, after Sweeping Edge deals more than 1, rebased onto the A-06 `player.rs`.

### Combined smoke, after each merge that can interact

Run this on the integrated tree, one new world, one opped survival player. Mark the row when the earlier branch is not in the tree yet.

- [ ] **S.1 Boot** (Console). `Started server; took`, no panic. If A-42 is in: `Loaded 3 embedded predicate(s)` and no `fast_cooking` warning.
- [ ] **S.2 Glass** (Client). After A-42: normal pickaxe drops no glass, Silk Touch drops glass.
- [ ] **S.3 Chunk round trip** (Client). After A-48: place oak stairs facing east and a stone block, `stop`, open the world in vanilla 26.3. No `Recoverable errors when loading section`. The stairs still face east.
- [ ] **S.4 Chest through autosave** (Client). After A-20 and A-48: `autosave_ticks = 200`, cobblestone in a chest, coal in a furnace in the same chunk, stay there, wait for the Saving line, `kill -9`, start, both items remain. Then `stop` and open that world in vanilla 26.3: the chest block exists and the cobblestone is still in it.
- [ ] **S.5 Bed, then player file** (Client). A-16 is already on core. On the current A-06 tip: daytime bed click sets `respawn` `pos` to the head and does not lie down. `stop`, start, `data get entity @s respawn` still has that `pos`, with `yaw` and `pitch` (not `angle`). `kill @s` respawns at that bed. Inventory you set before the restart is still there, and health you set with `damage @s 7` is still short.
- [ ] **S.6 Melee after the player.rs rebase** (Client). After A-24 is rebased onto A-06: A-24.1 (wooden sword, health 16) and A-24.8 (Sweeping Edge III, second zombie loses more than 1). Then `stop` and rejoin: the sword is still in the hotbar (A-06) and a full-charge hit is still 4 (A-24).
- [ ] **S.7 Worldgen count** (Console). Only if a later merge touches `crates/pumpkin-world/src/generation/`. Otherwise record "not run, no generation diff". If you run it, use A-45.2: `46 passed; 0 failed`, and the four `Total mismatches` lines listed there. The old noise 2 / surface 2846 / carvers 9122 totals are the `70a0b846e` grids.

**Fail the integrated tree** if any row that applies to the branches you merged fails. A failure in S.4 after both A-20 and A-48 means the autosave flush and the 5023 writer do not agree. A failure in S.5 after both A-16 and A-06 means the bed point is not the point that gets saved. A failure in S.6 means the `player.rs` rebase dropped either the attack path or the NBT path.
