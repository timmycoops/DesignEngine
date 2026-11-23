# 0104 — The AI script brief: one Copy for AI on every script editor, names with their meanings, an agent that checks its own work

Date: 2026-11-24 · Status: accepted (Tim, 2026-11-24: "Go for ai script option next", on the four parts noted that day)

## Why

Tim, 24 November, after the Tables and scripts and Table setup plans: "the best option is to be able to copy this into
some AI agent and have it build the scripts right. How will it know the best way to do that and use the fields in the
objects?" Scripting is meant to be AI-writable from pasted ideas (0077).

Copy for AI handed over how scripts are written, the names and the script, but each editor built its own text, an
object script had none, a table script got no columns of the table it runs on, and nothing said what a name meant
(`dip` measured in degrees, `thickness` vertical, `rd` air dried) or what words a column holds. An AI was writing blind
on the very things that make a script right, and a refused script came back by hand.

## Decision

1. **One brief, built by the engine, on every script editor** (`brief`, `BriefFor`): the reference; what this kind of
   script is and what it may set; a dictionary of every name it can read, words or a number, its unit, its meaning
   where the engine knows it, and how many rows have it; the parameter tables with their columns; the project's
   scripts and their functions; the script as it stands; the ask (write, explain or fix); "answer with one code block".
   A dataflow's formula or allocation is briefed on the dataflow as edited.
2. **Words only when asked.** The words each word column holds (seam, horizon, material names) make an AI's `if` lines
   right, but on a client's project they're the client's: they go in only when "with words" is ticked, off by default,
   each time. A number never goes in.
3. **Copy problems for AI:** where the check found problems, they go with their lines, and the script, for the AI to
   fix its own work.
4. **An agent checks its own work** (`check_script`, `try_script`, and the same in Python): a script checked against the
   project as its editor checks it, and tried without writing anything: a calculated column on a row line by line, an
   object script's sets on the blocks it would change, a table script's table and first rows. So an agent writes,
   checks, tries and only then runs.

## Built

- **Engine** (`builtin/brief.rs`, `builtin/script_try.rs`): `brief(project, BriefFor)` gives the Markdown; `BriefFor`
  carries the kind (`formula`, `allocation`, `calculated`, `object`, `table`, `library`), the dataflow (saved, or as
  edited in `content`), the table or the Objects table, the layers an object script is meant for, the script and its
  name, `words`, the ask and the problems. A name's meaning comes from what the engine writes: the measures (0074),
  the cuts' attributes, the run's densities and moistures. `check_script` checks each kind as its editor does;
  `try_script` runs it on the project as it stands through the commands' own `run`, never applied.
- **Python:** `session.brief(kind=…, …)`, `session.check_script(…)` and `session.try_script(row=…, …)`, keywords as
  `BriefFor`'s.
- **The app:** one `CopyForAi` on the calculated column, formula, allocation, table step, project script and object
  script editors: Copy for AI, the "with words" tick (off), Explain, and Copy problems for AI where there are problems.
  The object script strip is checked as it's typed. Each editor's own Copy for AI text is gone.

| Test | Known answer | Tolerance |
|---|---|---|
| `a_brief_names_every_column_with_its_meaning_and_the_words_only_when_asked` (reserves_run.rs) | an object script's brief on the starter's blocks: the reference first; `true_thickness` a number meaning vertical × cos dip, `seam` words taking waste below, 6 of 6 rows; no block's value (no 1.4, no 100000); no words unless asked, then `horizon`: A, OB, below A; the parameter tables; the fix brief lists line 2; a formula's brief reads the dataflow as edited | exact |
| `a_script_is_checked_and_tried_without_writing_anything` (reserves_run.rs) | `row.thicknes` refused on line 2, "did you mean `thickness`"; an object script tried on the blocks' layer: 4 changed, `1_B1_OB: process = "dragline"`, nothing revised; a calculated column on A's coal row sets 14,000 (10,000 m³ at 1.4); a table script on Objects gives 2 rows, `block, volume`, no table made; a formula's misspelt field on line 2; a library's problem | exact |
| `the_measures_say_what_they_are` (brief.rs) | the measures' meanings; a site's own name gets none | exact |
| The demo pit (by hand, from Python) | an object script's brief of 37,190 characters; a misspelt name on line 2; tried on `9 blocks`: 1,340 of 1,815 changed; a calculated column traced; a table script on Objects grouped by seam | — |

## Known limits

- A formula or allocation step is checked here but tried in the Dataflow tab, on a block's column.
- The brief carries the whole reference, about 37,000 characters: long, but every kind's calls are in it.
- A meaning is given only for the names the engine writes; a site's own names (and seam grid variables such as `as`) get
  none, and the AI reads them by their names.
- Object script problems from the formula engine say "a field of this dataflow" where they mean a column of the Objects
  table.
