# Working with Claude Code on Design Engine: review and opportunities

19 November 2026. Looked at: the repo's Claude Code setup (`.claude/`, settings, CLAUDE.md), CI, the git history since
28 October, and the current Claude Code documentation (commands, skills, hooks, memory, worktrees, goals, workflows).
Not seen: your user-level `~/.claude` (personal settings, skills, session transcripts), which Claude can't read from
here. Run `/insights` once (section 3.5) and it fills that gap.

---

## 1. Bottom line

- **You're getting a lot out of Claude Code with almost none of its configuration.** 239 merged PRs and 582 commits in
  23 days, each PR one workstream, each commit naming its decision record. All of it rides on one file, CLAUDE.md, and
  on what you type. The repo has no `.claude/settings.json`, no skills or commands, no subagents, no hooks, no rules.
- **The biggest single gain is making CLAUDE.md smaller.** It's 34 KB and loads in full at every session start and
  after every compaction. Claude Code's guidance is under 200 lines per file, because longer files lower adherence.
  Move the object model detail into path-scoped rules that load only when Claude touches that code.
- **Your loop repeats the same five steps every PR.** Pick up a workstream, plan, build with tests, check like CI does,
  write the decision record and PR. Each is a skill (a slash command) waiting to be written once.
- **Several built-in commands fit your work and probably aren't in use:** `/insights`, `/code-review`, `/simplify`,
  `/goal`, `/doctor prompt-audit`, `/fewer-permission-prompts`, `/verify`, background sessions, and workflows.
- **One thing to fix today:** CLAUDE.md opens with "It replaces Deswik for…". You've said Design Engine doesn't replace
  Deswik; it does one small job. Claude Code reads that line first in every session.

---

## 2. How you work today (from the repo)

| What | Seen |
|---|---|
| Pace | 239 merged PRs, 582 commits, 28 Oct to 19 Nov; 15 to 52 commits a day |
| Unit of work | One workstream a PR, branch `area/topic` (`filters/engine`, `reserves/fields-ws2`), commit naming the decision and workstream (`(0088, ws3)`) |
| Planning | Build Plan doc tabs (Claude Docs) → decision record → workstreams; briefs in `docs/briefs/` |
| Change shape | Average commit 11.5 files, +915 lines, −43 lines: the code only grows |
| CLAUDE.md | 34 KB, 359 lines, most lines a paragraph (one bullet runs to about 1,000 words); changed in 119 commits, 27 of them on 16 Nov |
| Decision records | 91, about 1 MB; the largest 41 KB (0038) |
| Claude Code config | None in the repo: only `.claude/worktrees/` |
| Worktrees | 11 under `.claude/worktrees/`, 9 of them `prunable` (left over); `.claude/worktrees/` isn't in `.gitignore` |
| Real-pit tests | Opt-in through environment variables (`ZD_WALL_STRING`, `ZD_FACE_GRIDS`, `ZD_SPRY_FILE`, `ZD_DESWIK_DCF`, `ZD_VULCAN_GRIDS`…); golden data is gitignored, so a fresh session or worktree has neither the data nor the paths |
| CI | Engine tests and clippy on Windows and Linux, Python module, app typecheck/test/build, Tauri shell clippy and tests. No real-pit tests, no format check |

---

## 3. Opportunities, in order of payoff

### 3.1 Cut CLAUDE.md to a core, move the rest into rules (high payoff, an afternoon)

**Why:** CLAUDE.md is loaded in full every session and re-read after every `/compact`. At 34 KB it's roughly 9,000
tokens of context before you've typed anything, and Claude Code's own guidance is that "longer files consume more
context and reduce adherence". It has also become the place every decision lands (27 edits in one day), so it reads as
a changelog, not a brief.

**How:**

- **CLAUDE.md, under 200 lines:** what this is (corrected framing), priorities, the locked stack table, the nine engine
  rules, how to work, build and test commands, repo layout, where decisions and briefs live. One line per object type
  with a pointer to its decision record.
- **`.claude/rules/` with `paths:` frontmatter**, so each loads only when Claude reads matching files:

| Rule file | `paths` | Holds |
|---|---|---|
| `faces-and-ramps.md` | `crates/zd-geom/src/{wall,road,ramp,project}*.rs`, `crates/zd-commands/src/builtin/face*` | FaceProfile, Ramp, switchbacks, the manual builder |
| `solids-and-seams.md` | `crates/zd-geom/**`, `crates/zd-core/**` | Solid, SeamModel, cuts, benches, blocks, validity |
| `reserves.md` | `crates/zd-reserves/**`, `crates/zd-script/**`, `crates/zd-commands/src/builtin/reserves*` | Reserves conventions, dataflow, parameter tables, Starlark |
| `exchange.md` | `crates/zd-io/**` | Formats, readers, `.zdp` store |
| `app-ui.md` | `app/src/**` | The Interface section: docks, kit, tools list, ribbon, shortcuts |

- Decision records stay as they are; rules point at them rather than restating them.
- Then run **`/doctor prompt-audit`**, which checks CLAUDE.md, rules and skills for outdated or conflicting
  instructions.
- **Keep it small:** CLAUDE.md gets a line only when it changes how every session works; anything about one area goes
  in that area's rule.

### 3.2 Turn your loop into project skills (high payoff, a day)

Skills in `.claude/skills/<name>/SKILL.md` become `/name` commands for every session and worktree, versioned with the
repo. Drafts are in the appendix.

| Skill | What it does | Replaces |
|---|---|---|
| `/ws <tab> <n>` | Reads the workstream from the Build Plan tab, writes the short plan, stops for your yes if it touches the object model, command interface or stack, then branches, builds with tests, and opens the PR | The opening prompt you type for each workstream |
| `/decision <title>` | Writes the next numbered record in the house style (context, decision, what it leaves, tests), checks the number isn't taken (there are two 0053s) | Hand-numbering and style drift |
| `/pr-ready` | Runs what CI runs, locally and in order: `cargo test` (as CI excludes), clippy `-D warnings`, the zd-py script, `pnpm typecheck`, `test`, `build`, Tauri clippy | A red CI run after the push |
| `/gate` | Runs the opt-in real-pit and grid tests with the `ZD_*` paths set, reruns the workflow demo, compares totals with the recorded figures, reports faults, volumes and timings | The Review 10 Nov item 9 "one-command local gate", and today's demo-totals drift |
| `/brief <file>` | Works through a `docs/briefs/` file item by item, a commit each, ticking each off in the brief | How the app-review brief was handed over |

### 3.3 Settings in the repo (medium payoff, an hour)

- **`.claude/settings.json`** with:
  - `env`: the `ZD_*` paths pointing at `D:\Dev\DesignEngine\testdata\golden\…` (absolute, so worktrees find them too).
    Real-pit tests then run in any session, and `/gate` needs no setup.
  - `permissions.allow` for the commands every workstream runs (`cargo test`, `cargo clippy`, `pnpm …`, `git …`,
    `gh pr …`). Let **`/fewer-permission-prompts`** draft this from your own transcripts rather than writing it by hand.
  - `permissions.deny` for writes under `testdata/golden/**` and any client folder (the Deswik process maps), so client
    data is never edited or copied into the repo.
- **`.gitignore`:** add `.claude/worktrees/`, as the worktree docs advise.
- **Personal preferences** (anything only you want) go in `CLAUDE.local.md`, gitignored.

### 3.4 A few hooks, kept cheap (medium payoff)

Hooks run deterministically, so they're for rules that must hold every time, not for advice.

- **PreToolUse guard** (as well as the deny rules): refuse `git push` to `main` and `--force`, and edits to
  `testdata/golden/**`.
- **Notification hook → Windows toast** when Claude is waiting for you, so long Rust builds don't sit idle on a prompt.
- **Not recommended:** a Stop hook that runs the whole test suite. Rust builds are slow and a failing test can loop
  the session (the docs warn about this). `/pr-ready` at the end of a workstream does the same job on purpose.

### 3.5 Built-in commands that fit your work

| Command | Use it for |
|---|---|
| `/insights` | **Run first.** An HTML report of your recent sessions on this machine: how you use Claude Code, where things go wrong, features to try. It sees what I couldn't |
| `/code-review high` (or `--fix`) | Before each PR: a correctness pass on the diff. `/code-review ultra` for big ones (the store rewrite, the column engine) |
| `/simplify` | Every few days: four parallel reviewers for reuse, simplification, efficiency and abstraction. Worth it with +915/−43 a commit |
| `/goal <condition>` | Workstreams with a checkable end: "`cargo test -p zd-reserves` passes and the demo's Base ROM totals match `bench/RESULTS.md`". Claude keeps going until a separate model judges it met |
| `/plan` | Enter plan mode for the object-model and command-interface changes CLAUDE.md says need your agreement |
| `/run-skill-generator`, `/verify` | Teach Claude how to build and launch `zd-app` (or its `--bench` modes) so it checks a change running, not just tests. The docs list CLI, server, TUI and browser apps, so try it on the Tauri app before relying on it |
| `/batch <instruction>` | Mechanical sweeps across many files in parallel worktrees, e.g. moving Face design and Properties onto the UI kit, or replacing the 430 lines of hand-copied Rust types |
| `ultracode:` / workflows | Audits that need many agents: "check every command against engine rule 4 (never modify inputs)". Save a good run with `s` in `/workflows` and it becomes a command |
| `/background`, `/fork`, agent view | Run two workstreams at once in their own worktrees instead of managing worktrees by hand |
| `/btw` | A side question that doesn't fill the conversation |
| `/context`, `/autocompact` | See what's using the context window (CLAUDE.md will show up), and set when it compacts |
| `/rewind`, `/branch` | Roll back a wrong direction, or try two approaches to one problem (two ramp algorithms) side by side |
| `/autofix-pr` | A cloud session that watches the PR and fixes CI failures. It can't reach golden data, so it suits engine and app checks only |
| `/skill-doctor` | Once skills exist: what each costs in context and whether it's used |

### 3.6 Subagents for review (lower payoff, when the skills are in)

`.claude/agents/` definitions that the skills call:

- **`geometry-reviewer`**: reads a diff against the engine rules (f64, typed commands, inputs untouched, validity
  flags, nothing fixed silently, tolerances stated in tests) and reports breaches with file and line. Read-only.
- **`spry-checker`**: compares a reserves change with the Spry reference (the dataflow snippets and the Spry skills) and
  lists where the engine's method differs and whether a decision record says so.

### 3.7 In the desktop app (Tim uses it)

- **Parallel sessions with the worktree option** are what made the folders under `.claude/worktrees/`
  (`heuristic-blackwell-8c53d5` and the rest). Keep using them for parallel workstreams; prune what they leave.
- **Where config is read from:** in a desktop worktree session, settings, hooks, skills, agents, commands and
  `.mcp.json` come from the **main checkout's root**, but `CLAUDE.md` and `.claude/rules/` load from the **worktree**.
  So:
  - Skills and settings work in every parallel session as soon as they're in `D:\Dev\DesignEngine\.claude\`, even
    before they're merged.
  - A slimmed CLAUDE.md and new rules only reach a session whose worktree branched after they were merged to `main`.
    Merge that change first, then start new sessions.
  - Hooks run in the main checkout's root; a hook that needs the worktree reads the `cwd` field of its input.
- **Workflows show in the Background tasks pane**, with an approval card (Once, Always, Deny) before one starts.
  `/goal` works in the desktop app too.
- **Local scheduled tasks** in the desktop app run on your machine, so they can reach the golden data. A nightly
  `/gate` belongs there, not in a cloud scheduled task.
- **Notifications:** check whether the desktop app already tells you when a session is waiting before adding the
  Notification hook in 3.4.
- Terminal-only views (`/focus`, fullscreen) and `/desktop` don't apply.

### 3.8 Housekeeping

- `git worktree prune`, then delete the nine left-over folders under `.claude/worktrees/` (needs delete permission;
  move them aside otherwise).
- The old build folders the 10 Nov review counted (about 100 GB across `target*`).

---

## 4. Suggested loop

1. **Start a session:** `/ws "Filtering plan" 4` (or `/brief docs/briefs/…`).
2. **Plan:** the skill writes a short plan and stops if the change needs your agreement; `/plan` for a bigger change.
3. **Build:** for a checkable end, `/goal "… tests pass and …"`; otherwise normal turns. `/btw` for side questions.
4. **Check:** `/pr-ready`, then `/code-review high --fix`; `/gate` when geometry or reserves changed.
5. **Ship:** the skill writes the decision record update and opens the PR; `/autofix-pr` if CI goes red.
6. **Weekly:** `/simplify` on the week's merges, `/doctor prompt-audit`, `/insights`.

---

## 5. Questions

1. ~~Desktop app or terminal?~~ Desktop app (Tim, 19 Nov): see 3.7.
2. ~~Does Claude Code read the Build Plan doc?~~ Yes (Tim, 19 Nov): it reads anything in his local environment. The
   repo holds no copy or link of the doc (CLAUDE.md and the records name its tabs only), so put the doc's link in `/ws`
   so a new session goes straight to it rather than searching by name.
3. Any **personal skills** in `~/.claude/skills/` already doing some of this?
4. Happy for me to **draft the slimmed CLAUDE.md and the rules files** on a branch for you to review?

---

## Appendix: drafts

### A. `.claude/skills/ws/SKILL.md`

```markdown
---
description: Pick up a workstream from the Build Plan doc and take it to a PR. Use when Tim names a plan tab and workstream.
argument-hint: <plan tab> <workstream number>
arguments: [tab, ws]
---
Read the "$tab" tab of the Pit Design Engine Build Plan doc (<paste the doc's link here>) and find workstream $ws.

1. Restate what lands, in mining terms, and the decision record it belongs to.
2. Write a short plan: files, commands, tests (a synthetic known answer, plus golden data when it exists).
   If it touches the object model, the command interface or the stack, stop and wait for Tim's yes.
3. Branch `<area>/<topic>` from main.
4. Build with tests. State volume tolerances in every geometry test. Report timings for anything geometry-heavy.
5. Run /pr-ready. Fix anything red.
6. Update the decision record ("Built" section) and the Build Plan tab's status line.
7. Commit as "<what changed> (<decision>, ws$ws)" and open the PR.
```

### B. `.claude/skills/pr-ready/SKILL.md`

```markdown
---
description: Run locally what CI runs, in CI's order, and report what fails. Use before pushing a branch.
allowed-tools: Bash(cargo *), Bash(pnpm *), Bash(.venv/Scripts/python *), Bash(.venv/Scripts/maturin *)
---
Run, stopping at the first failure and fixing it:

1. `cargo test --workspace --exclude zd-app --exclude bench --exclude zd-py`
2. `cargo clippy --workspace --exclude zd-app --exclude bench --exclude zd-py -- -D warnings`
3. `cargo clippy -p zd-py -- -D warnings`, then build and run `crates/zd-py/tests/test_session.py`
4. In `app/`: `pnpm typecheck`, `pnpm test`, `pnpm build`
5. `cargo clippy -p zd-app --all-targets -- -D warnings`, `cargo test -p zd-app`

Report each step's result and time in one table.
```

### C. `.claude/skills/gate/SKILL.md`

```markdown
---
description: Run the real-pit and grid gate locally: opt-in golden tests, the workflow demo, totals against the recorded figures.
disable-model-invocation: true
---
1. Run the opt-in golden tests with the ZD_* paths from settings (list each test and whether its data was found; a
   test whose data is missing is a failure, never a pass).
2. Rerun testdata/synthetic/workflow_demo.py and compare Base and 50m ROM coal and waste with bench/RESULTS.md.
3. Report: fault counts, volumes and their differences with tolerance, timings (triangles in, ms, triangles out).
4. If a total moved, name the commit that moved it (git log on the touched crates) and ask before updating the record.
```

### D. `.claude/settings.json` (shape; paths to be filled)

```json
{
  "env": {
    "ZD_SPRY_FILE": "D:\\Dev\\DesignEngine\\testdata\\golden\\…",
    "ZD_VULCAN_GRIDS": "D:\\Dev\\DesignEngine\\testdata\\golden\\…"
  },
  "permissions": {
    "deny": ["Edit(testdata/golden/**)", "Bash(git push --force*)", "Bash(git push origin main*)"]
  }
}
```

---

## Sources

- Commands reference: https://code.claude.com/docs/en/commands.md
- Skills (frontmatter, `/run`, `/verify`, `/doctor prompt-audit`, `/skill-doctor`): https://code.claude.com/docs/en/skills
- Memory (CLAUDE.md size, `.claude/rules/`, `CLAUDE.local.md`, what survives compaction): https://code.claude.com/docs/en/memory
- Hooks: https://code.claude.com/docs/en/hooks
- Worktrees: https://code.claude.com/docs/en/worktrees
- Goals: https://code.claude.com/docs/en/goal
- Dynamic workflows and ultracode: https://code.claude.com/docs/en/workflows
