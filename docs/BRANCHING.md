# Branching

Work on this fork uses two long-lived branches. `master` stays a clean mirror of [Pumpkin-MC/Pumpkin](https://github.com/Pumpkin-MC/Pumpkin) `master`. Do not commit task work to `master`.

## core-functionality

Vanilla server parity, which is Pumpkin's original mission. Phase A, the compat harness (`tools/pumpkin-compat`, `docs/compat/GAP_ANALYSIS.md`), the research notes, this roadmap, and the Phase B tasks whose default results match vanilla all land here. The list is in [ROADMAP.md](../ROADMAP.md).

Cut `core-functionality` from `master`. Keep it close to upstream so a merge of upstream `master` stays small: parity fixes and behaviour-preserving speedups, not opt-in mods and not a second tick scheduler.

## enhanced-functionality

Mods and added features. That is the rest of Phase B (opt-in knobs, commands vanilla does not have, policies that change who is connected or which chunks tick, and regionised ticking), plus Phase C and Phase D.

`enhanced-functionality` is built on `core-functionality`. Merge core into enhanced regularly. Never merge enhanced into core.

## Task branches

Each task file names its primary branch and its task branch. Cut the task branch from that primary branch, do the task, and merge it back into the same primary branch with a merge or a fast-forward. No pull requests.

Names look like this:

- `core/A-04-dataversion-5023`
- `enhanced/D-03-voice-udp`

The prefix is `core/` or `enhanced/`, then the task id, then a short name. One task per branch.

A core task must not depend on an enhanced task. An enhanced task may depend on a core task. Start it after that core task has been merged into `enhanced-functionality`.

```bash
git checkout core-functionality
git pull origin core-functionality
git checkout -b core/A-04-dataversion-5023
# ... work, git commit -s, git push -u origin core/A-04-dataversion-5023
git checkout core-functionality
git merge core/A-04-dataversion-5023
git push origin core-functionality
```

Use `enhanced-functionality` and an `enhanced/` branch the same way.

## How enhanced pulls in core

```bash
git checkout enhanced-functionality
git merge core-functionality
git push origin enhanced-functionality
```

Do this before starting an enhanced task whose dependency just landed on core.

## How core pulls from upstream

Fetch and merge only. Never push to Pumpkin-MC/Pumpkin.

```bash
git remote add upstream https://github.com/Pumpkin-MC/Pumpkin.git   # once
git fetch upstream master
git checkout core-functionality
git merge upstream/master
git push origin core-functionality
```

After core has the upstream commits, merge core into enhanced as above.

Leave the fork's `master` pointing at the same commit as `upstream/master`. Update that mirror with a fast-forward when a human asks. Do not push `core-functionality` or `enhanced-functionality` onto `master`, and do not push anything to `upstream`.

## CI

`.github/workflows/compat.yml` runs on push to `core-functionality`, `enhanced-functionality`, and task branches under `core/` and `enhanced/`.
