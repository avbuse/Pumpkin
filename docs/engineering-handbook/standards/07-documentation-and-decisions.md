# 07 — Documentation and architecture decisions

## Where things live

- **Paperclip issues** — status, decisions, blockers, evidence.
- **Git** — source code, standards, and durable technical documentation.

Do not duplicate tracking across systems. Keep handoffs concise.

## Documentation with code

A change that alters behaviour, setup, commands or architecture updates the relevant documentation
in the same PR. Follow any documentation checklist the project already has.

## Architecture decision records

Record a significant decision (a choice that is costly to reverse, or that a future reader would
otherwise question) as an ADR in `docs/adr/NNNN-title.md` using `templates/adr.md`. One page:
context, decision, alternatives considered, consequences. Superseded ADRs are marked, not deleted.
