# Review notes: human test plan

Reviewed at `dc28d68b`. Delete this file in the commit that addresses it.

**Status: needs a refresh before testers use it.** The steps are clear, but the branch table is out of date.

1. **The "Code" column points at review-notes commits.**
   - A-24 lists `b88bccbea`, A-45 lists `c2a34dcdc` and A-48 lists `9af839d07`. Those are `docs(review): add … review notes` commits, not code.
   - A-24's code is `38bda99`, `5f588c7` and `409c9fb8` (Sweeping Edge).
   - A-45 has no code; it is docs only.
   - A-48's code is `0c173c4`.
2. **Tips have moved.** A-24's tip is now `68e2096f`, not `b26788191`. That commit adds the Sweeping Edge playtest step, which is missing from this plan's A-24 section. Re-read each tip before handing the plan out.
3. **New branches are missing:** A-13 (tool actions), A-37 (redstone dust), A-50 (scoreboard names) and A-53 (atomic player save).
4. **Merge order.** Put A-48 first, as it is the fix for core reading 26.3 worlds as air. A-53 should go before or with A-06, since both touch player data.
5. **`TestPlan.md` at the root of every branch** makes each merge after the first conflict on that file. Say how testers and mergers should resolve it: drop it in the merge into core.
