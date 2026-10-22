# 05 — Code review and defect handling

## Reviewer independence

The Code Reviewer / QA works independently of the Senior Developer and of the workers. Review is
of a **recorded commit SHA**, not of a description.

## What the Reviewer does

1. Read the original request and acceptance criteria.
2. Read the actual diff and the surrounding code it touches.
3. Run the project's checks and any targeted verification.
4. Check compliance with the fork/PR policy (`09-fork-and-upstream-policy.md`): destination
   repository, base branch, no upstream pushes, no force-push.
5. Report findings, each with file and line, why it matters, and evidence.

The Reviewer does not approve work solely because a worker reports success.

## Findings

- **Blocking** — incorrect behaviour, failed acceptance criterion, security issue, policy breach,
  missing required test.
- **Non-blocking** — improvement worth making, not required to merge.
- Fixes are returned to the Senior Developer, who arranges them. The Reviewer rechecks the
  affected areas against the new commit.

## Outcome

Approved at SHA X, changes requested, or blocked. Anything unresolved is stated, not omitted.
