# 03 — Branches, commits and pull requests

## Branches

- Use the project's recorded default branch (it is often `master`, not `main`). Never assume.
- Never commit directly to the default branch.
- One branch per task, named `feature/…`, `fix/…`, `docs/…` or `refactor/…`, unless the project
  defines otherwise. A branch name should make the issue identifiable.

## Commits

- Small, coherent, with a message that says why, not only what.
- No secrets, generated artefacts, or unrelated formatting changes.

## Pull requests

- Open against the project's own repository and recorded base branch. Always pass the repository
  explicitly; never rely on `gh` defaults:

  `gh pr create --repo OWNER/REPO --base BASE --head BRANCH`

- Before every push and every PR, verify the destination remote and repository against the
  project's recorded configuration.
- Description includes: issue link, outcome, acceptance criteria status, test evidence, risks,
  and the reviewed commit SHA.
- **Agents open PRs; the human owner merges.** Nothing is merged or deployed without explicit
  approval. Merge authority is delegated only when the human owner says so in writing.
- Never force-push a shared branch. Never rewrite history that has been reviewed.

## Verify after creating

Confirm the PR's base repository is ours: `gh pr view --repo OWNER/REPO NUMBER --json baseRefName,url`.
