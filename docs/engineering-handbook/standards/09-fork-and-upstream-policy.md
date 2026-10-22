# 09 — Fork, pull request and upstream sync policy

Applies to every project, every `AGENTS.md`, and every Cursor worker assignment. The Senior
Developer enforces it and passes it to every worker. The Reviewer checks compliance before
approving.

## Repository configuration (recorded per project)

- Our repository as `OWNER/REPO`.
- The parent repository, if forked.
- Our default branch, and the upstream branch used for synchronisation.

Git's default push remote and `gh`'s default repository point at **our** repository.
Verify the destination against the project record before every push and every PR.

## Pushes and pull requests

- Develop, push and open pull requests only within our repository.
- Open PRs from our working branches into the appropriate base branch of our repository.
- **Never push to the parent repository or open a PR against it** unless the human owner has
  explicitly authorised that specific contribution. Upstream remotes are read-only: fetch and
  compare only.
- Always specify the repository: `gh pr create --repo OWNER/REPO --base BASE --head BRANCH`.
- If ownership, destination or permissions are unclear, stop the affected action and report.

## Upstream synchronisation

The Senior Developer may fetch parent updates and prepare them for integration. For each sync:

1. Verify our fork, the parent, and the relevant branches.
2. Fetch upstream; record the upstream commit being considered.
3. Create a dedicated sync branch from our target branch and merge upstream into it.
4. On conflicts: stop, report the affected files, request a decision. Do not discard our changes.
5. If clean: run the project's build, tests and required checks.
6. The Reviewer assesses compatibility with our customisations and the acceptance criteria.
7. When checks pass, open a PR into our target branch and follow the normal merge approval rules.

A clean merge does not prove compatibility; failed checks or behavioural incompatibility block
integration.

**Never** overwrite our fork with the parent's branch, force-push, or discard our commits.

## Handoff contents

Destination repository; source and target branches; upstream commit included (for syncs);
validation results and unresolved issues; PR URL. Verify that every PR created targets our
repository.

## Interaction with a parent's own agent rules

A parent repository may contain agent instructions (for example, a rule that agents do not open
pull requests). Treat such a rule as governing contributions **to the parent**, which this policy
already forbids. It does not govern pull requests into our own fork. Record this reading as an
explicit, scoped project exception in the fork's `AGENTS.md`; do not apply it more widely.
