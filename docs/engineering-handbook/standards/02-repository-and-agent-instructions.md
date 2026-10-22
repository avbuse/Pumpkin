# 02 — Repository setup and agent instructions

## Every project repository contains

- `AGENTS.md` at the root, built from `templates/AGENTS.md.template`. It supplies concrete setup,
  build, test and run commands, architecture, constraints, the fork configuration, the handbook
  version and any justified exceptions.
- `docs/engineering-handbook/` — a vendored copy of this handbook.
- `HANDBOOK_VERSION` — one line, the handbook tag the vendored copy came from.

Cursor workers read these files from the checkout. A link to an inaccessible handbook is not
sufficient.

## Existing conventions win

If the repository already has `AGENTS.md`, `CLAUDE.md` or equivalent, extend it with a short
pointer block rather than replacing it. Do not change a project's established conventions without
an agreed reason, recorded as an exception.

Do not keep two competing instruction files. If `AGENTS.md` is a pointer to `CLAUDE.md` (or the
reverse), keep that arrangement.

## Fork repositories

Keep the footprint minimal so upstream syncs stay clean: the vendored directory, the version file,
and a single appended block in `AGENTS.md`. Do not reformat or reorder upstream content.

## Worktrees

Concurrent writers each get a separate git worktree. Separate context windows alone do not provide
filesystem isolation. Never edit a checkout the human owner is working in.
