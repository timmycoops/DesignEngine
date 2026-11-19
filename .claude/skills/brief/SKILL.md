---
name: brief
description: Work through a brief in docs/briefs item by item, a commit an item, ticking each off in the brief. Use when Tim hands over a brief file.
argument-hint: "<brief file>"
arguments: [file]
---
1. Read `$file` whole. List its items (numbered sections, bullets or table rows that ask for a change) with what each
   needs, and which touch the object model, the command interface or the stack: those wait for Tim's yes, as
   CLAUDE.md says. Ask about anything the brief leaves open before starting it.
2. Branch from an up-to-date `main` as `/ws` does, one branch for the brief unless an item is big enough for its own PR.
3. For each item in order:
   - make the change, with tests for any engine change;
   - tick it off in the brief: `- [x]` before the item, or `Done (date, commit): one line` under it, keeping the
     brief's own words;
   - commit the change and the tick together, the message naming the brief and the item.
4. An item that can't be done or that the repo shows is wrong: don't force it. Mark it `Not done: <why>` in the brief and
   tell Tim.
5. Run `/pr-ready` before the PR. The PR lists each item: done, not done (why), or waiting on Tim.

If the brief isn't committed (`git ls-files $file` is empty), say so first: a worktree session can't see it, and the
ticks only travel once it's in git.
