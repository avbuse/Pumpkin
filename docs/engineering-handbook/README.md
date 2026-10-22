# Engineering Handbook

Canonical shared standards and reusable templates for every software project run by the
Wayne Tech Labs engineering organisation (Engineering Lead, Senior Developer, Code Reviewer / QA,
plus task-scoped Cursor workers).

Current version: see `VERSION`. Changes: see `CHANGELOG.md`.

## How projects consume this

Projects do **not** link to this repository. Each project vendors a copy at
`docs/engineering-handbook/` and pins it with a `HANDBOOK_VERSION` file at the repository root
containing a handbook tag (e.g. `v1.0.0`). Workers read the local copy.

Updating a project is a reviewable pull request into that project's own repository that
replaces `docs/engineering-handbook/` with the contents of `standards/` and `templates/` from a
newer tag and bumps `HANDBOOK_VERSION`. Nothing updates automatically.

## Precedence

1. The human owner's explicit instruction.
2. The project's own `AGENTS.md` / `CLAUDE.md` and recorded exceptions.
3. This handbook.

Existing project conventions are preserved. Where a project differs from the handbook, the
project wins and the difference is recorded as an exception in its `AGENTS.md`.

## Contents

| Area | File |
| --- | --- |
| Task intake and acceptance criteria | `standards/01-task-intake.md` |
| Repository setup and agent instructions | `standards/02-repository-and-agent-instructions.md` |
| Branches, commits and pull requests | `standards/03-branches-commits-pull-requests.md` |
| Testing and completion evidence | `standards/04-testing-and-evidence.md` |
| Code review and defect handling | `standards/05-review-and-defects.md` |
| Dependency and secret handling | `standards/06-dependencies-and-secrets.md` |
| Documentation and architecture decisions | `standards/07-documentation-and-decisions.md` |
| Release and deployment | `standards/08-release-and-deployment.md` |
| Fork, pull request and upstream sync policy | `standards/09-fork-and-upstream-policy.md` |
| Agent instructions template | `templates/AGENTS.md.template` |
| Project onboarding record | `templates/project-onboarding.md` |
| Cursor worker assignment | `templates/worker-assignment.md` |
| Architecture decision record | `templates/adr.md` |
| Cursor rule | `templates/cursor-rules/engineering-handbook.mdc` |

## Roles

- **Engineering Lead** — owns intake, outcomes, acceptance criteria, priorities, coordination and
  this handbook. Escalates product and spending decisions to the human owner.
- **Senior Developer** — owns technical delivery from assignment through integration. Directs
  bounded Cursor worker assignments, checks and integrates their output, and remains accountable
  for the result. Enforces the fork policy.
- **Code Reviewer / QA** — reviews independently of the Senior Developer against a recorded commit.
  Never approves on a worker's report alone.
- **Cursor workers** — task-scoped execution sessions, not permanent agents.
