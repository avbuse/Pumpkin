# 08 — Release and deployment

- **Nothing is released or deployed without the human owner's explicit approval.**
- Use the project's existing release mechanism and versioning scheme. If none exists, propose one
  in an ADR; do not invent one silently.
- A release candidate is a recorded commit that is tested and reviewed.
- Tag releases with the project's version scheme; the tag is created by, or with the approval of,
  the human owner.
- Record in the issue what was released, from which commit, to which environment, and how it was
  verified. Keep "merged" and "deployed" distinct.
- Include rollback notes for anything that changes data or infrastructure.
- Never deploy from an unreviewed branch.
