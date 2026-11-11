# 0059 — What a run reports: notes with a severity, a code and a place

Status: agreed 12 November 2026 (Tim: "Agree all five", the Build Plan's "Run report plan" tab). Workstream 1 of 4.

## Why

A run reported what it did as lines of text, from about 140 places in 12 command files. The app told a warning from
an ordinary line by looking for 17 words, so a warning worded another way showed as ordinary, and an ordinary line
with one of the words showed orange. The geometry knows where things go wrong (a wall step keeps each stretch with no
height as two points; a seam issue keeps its worst node), but that was lost in the sentence, so the app couldn't take
the planner there. A script, a test or a gate run couldn't count warnings or find them without reading sentences
(the review of 10 November, finding 4).

## Decision

Each line a run reports is a **note**:

- a **severity**: info (what it did), warning (look at this) or blocking (stops later use until fixed at the source,
  as a seam crossing does). A refused run stays the run's own state;
- a **code** from one fixed list in the engine (`zd_commands::Code`), named by family and what happened, such as
  `wall.no_height`. The list, each code's saved name and its meaning are written once, in `note.rs`, and the compiler
  checks every use. A run log from a later build naming a code this one doesn't know keeps the note and drops the code;
- the **text** a planner reads, unchanged from before;
- the **object** it's about: one the run read, by id, or one it makes, by its place in the run's changes. The session
  puts the made object's id in once the run is applied, as it has none before;
- a **place**: a point, a point in plan, or a stretch from one point to another, in mine grid coordinates (f64);
- a **value** with its unit, where there is one.

A refused run's kind of refusal (missing object, wrong kind, invalid, geometry, store and so on) is its `refused` code.

Agreed with it:

1. Three severities, as above.
2. Codes are a fixed list in the engine, not text each command makes up.
3. Run logs saved before notes are read, not rewritten: their lines read as info notes with no code.
4. For now notes show where messages show, in a tool panel's run result and the run log, with Show for a note that has
   a place. The Issues tab in a Results area waits on the UI plan's decision 3.
5. Python keeps `messages`, the notes' texts, beside `notes`, so scripts written before keep working.

## Workstream 1 (this record's first PR): the note type

`Note`, `Severity`, `Code`, `Place` and `Measure` are in `zd-commands`. A command reports plain `messages`, each read
as an info note, or `notes` with codes and places; the run record lists the messages first. `RunRecord` carries
`notes`, `messages` (their texts) and `refused`. Nothing a command says has changed yet, so the app and every test
that reads a run's messages work as before.

Checked: a made-up command's notes name what it read and what it made; a refusal carries its kind; a run log saved
before notes reads back whole. All 18 projects saved under `testdata` open, and their 923 logged runs read back as
4,842 info notes with their texts unchanged.

## Still to come

- **Workstream 2:** walls, ramps, steps, builds and strip solids give their warnings codes, places and values.
- **Workstream 3:** seam models, imports and repair, exports, links and the rest.
- **Workstream 4:** the app colours by severity, a note with a place gets Show, and the 17-word list goes.
