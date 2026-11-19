---
name: decision
description: Write the next numbered decision record in docs/decisions in the house style, with a number no other branch has taken. Use when a plan is agreed or a design choice is locked.
argument-hint: "<title>"
---
1. **Pick the number.** The highest in `docs/decisions/` on `main` plus one, unless that number is already taken on an
   open branch: check `gh pr list --state open --json headRefName,files` for any `docs/decisions/NNNN-*` file, and
   `git ls-tree -r --name-only origin/<branch> docs/decisions` for branches without a PR. Two 0053s exist because this
   was skipped; never reuse a number.
2. **Name the file** `NNNN-<title in a few lowercase words, hyphens>.md`, e.g. `0090-reserves-filter-in-the-view.md`.
3. **Write it in the house style** (read two recent records first, e.g. `0088-filters.md` and `0090-…`):

   ```markdown
   # NNNN — $ARGUMENTS

   Date: YYYY-MM-DD · Status: accepted (Tim, YYYY-MM-DD: what he agreed, his words where short)

   ## Why

   What Tim asked or hit, quoted where he said it, and what was wrong or missing.

   ## Decision

   What's decided, numbered where the plan had numbered decisions, in mining terms, each with the reason.

   ## Built

   Added as workstreams land: what landed, the tests (synthetic known answers, golden data), timings.

   ## Known limits
   ```

   Plain words a planner uses; no code-speak where a mining term exists. Short sentences. Cite other records by
   number.
4. **Point to it.** If the decision changes how a type behaves, add a line or a pointer to the matching
   `.claude/rules/` file; CLAUDE.md gets a line only if it changes how every session works.
