# Cursor worker assignment

A worker is a task-scoped execution session, not a permanent agent. Every assignment contains all
of the following; the worker has no other context.

- **Paperclip project and issue:**
- **Repository (OWNER/REPO), working directory, starting branch and commit:**
- **Required outcome:**
- **Acceptance criteria:**
- **Standards and project instructions to read:** `AGENTS.md`, `docs/engineering-handbook/`
- **Owned files / components:**
- **Scope exclusions (do not touch):**
- **Dependencies and interface decisions:**
- **Required tests / checks:**
- **Fork policy (always include):** push only to `OWNER/REPO`; PRs only with explicit
  `--repo OWNER/REPO`; never push to or open a PR against the parent; never force-push; never merge
  or deploy; stop and report if the destination is unclear.

## Return evidence (the worker must provide)

- Changes made (branch, commit SHA, files)
- Test and check results (commands and outcomes, including anything not run)
- Assumptions made
- Unresolved issues

The Senior Developer inspects the actual diff and re-runs checks before integrating. A worker's
report is evidence, not approval.
