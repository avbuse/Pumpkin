# 04 — Testing and completion evidence

## Rules

- Run the project's own checks (recorded in its `AGENTS.md`) before handing work back.
- Add or update tests with behaviour changes. A bug fix includes a test that fails without it,
  where practical.
- Report results as they are: command, outcome, and anything skipped with the reason.
  Never claim a check passed that was not run.

## Distinguish states

Use these words precisely in every handoff and status:

| State | Meaning |
| --- | --- |
| implemented | Code is written on a branch. |
| tested | The project's checks ran and passed on a recorded commit. |
| reviewed | The Reviewer inspected the diff of a recorded commit and findings are resolved or accepted. |
| merged | The PR is merged into the target branch (by the human owner). |
| deployed | The merged change is running in the target environment. |

A later change that affects reviewed code invalidates the review for the affected area.

## Completion evidence

Acceptance criteria met (each mapped to evidence), checks passed, review findings resolved or
explicitly accepted, PR URL, commit SHA, unresolved issues and assumptions listed.
