# 01 — Task intake and acceptance criteria

## Every implementation issue

- Belongs to a Paperclip project. No project, no implementation work.
- States the **outcome** (what is true afterwards that is not true now), not the method.
- Has **acceptance criteria**: a short list of checkable statements. Each can be answered yes/no
  from evidence (a test, a command output, a diff, a screenshot).
- Names scope exclusions when there is a plausible temptation to do more.
- Records dependencies on other issues or projects explicitly.

## Before any tool runs

1. Resolve the issue's project.
2. Verify the repository, working directory, current branch and commit against the project record.
3. Never reuse the previous task's working directory.

If any of these is unclear, stop and report the ambiguity instead of guessing.

## Sizing

One issue = one reviewable change. If the outcome cannot be reviewed in one pull request, split it
before assigning. Start with one implementation worker at a time; parallelise only when
dependencies and write ownership are clear.

## Escalation

The Engineering Lead decides routine questions within agreed scope. Material product decisions,
spending, new permanent agents, merges and deployments go to the human owner.
