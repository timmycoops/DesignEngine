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

## Workstream 2: walls, ramps, steps, builds and strip solids

`face.project_step`, `face.project`, `face.build_step`, `face.rebuild`, `face.join`, `solid.slice_by_faces` and
`solid.between_surfaces` report notes, with 37 codes in the `wall`, `berm`, `toe`, `face`, `ramp`, `join`, `line`,
`build`, `slice` and `solid` families.

- **One line, one thing.** A line that used to join several things with semicolons, such as a strip's walls and
  triangles followed by "step 2 has no height along 3 stretch(es)…", is now a summary line followed by one note for each
  warning. The words of each are as before, with the strip's or wall's name in front, so each warning can carry its own
  code and place. The summary keeps what's only information: toe conditioned and corners welded.
- **Places.** Stretches with no height, no target or no berm are placed at the first stretch and measured by their
  length together. Crossings are placed at the first crossing, and a face that isn't one sheet at its first edge's
  middle. A road is placed where it starts; if it stopped, steeped or missed where it's held, it's placed where that
  happened (a stop first). Gaps filled, toes above the floor, tops below the top and faces crossing are each placed where
  they are.
- **Objects.** A strip's notes are said of its string. A step's notes are said of the line it was built from, and a
  crossing of the wall the step makes. A slice's strip lines are said of the strip solid the run makes.

Checked:

- A wall whose target dips under its toe for 50 m is placed on that stretch, with its 50 m length.
- A ramp that can't reach is placed where it stopped.
- A strip's volume line names its solid.
- On the made-up grid pit, every fault the geometry can place has its place: 233 of its 235 warnings. The two without
  are a step with no height anywhere and a strip that couldn't be built, which name the strip's string instead.

## Workstream 3: every other command

`Outcome` holds only notes now: `message` adds an info note and `said` several, in the order written. A command that
mixes plain lines with coded warnings keeps them in order, as the record has no separate list of messages any more.
The solid check that runs after every command (solids in pieces, cavities, the crossing search) adds its notes last,
as it always said its lines.

Twenty-two more codes cover:

- **Seam models:** a seam left out of a suggested definition, and each stack issue. A crossing, a floor above its roof,
  a daughter outside its parent or unpaired nulls is blocking or a warning as the model decides it, placed at its worst
  node in plan and measured by its overlap in metres (or in nodes where there's no overlap). A working horizon with no
  density grid is a warning.
- **Links:** a relink that finds files changed since the model was built.
- **Meshes:** a surface that crosses itself (kept, flagged, refused by cuts), placed at the first crossing. Solids made
  in pieces, and solids holding cavities.
- **Imports:** files or meshes refused, solids whose volume differs from Deswik's, Spry's or the source's own, meshes
  kept as surfaces as they couldn't be solids, and grids of a seam skipped.
- **Cuts:** a solid wholly on one side, solids the surface doesn't cover, and solids left uncut.
- **Exports:** solids left out because their files don't read back the same.
- **Strings:** vertices with no surface under them when draped, and strings a numbering line doesn't cross or crosses
  more than once.

What's only information stays an info note with no code: counts, timings, what a repair did, and what a check found
clean. Every phrase the app's word list looks for now comes from a coded warning. The list also caught some
information as warnings, such as "none crosses itself", and those are info notes now.

Checked: a surface crossing itself is placed at the crossing; a seam crossing is blocking at its worst node with its
1.5 m overlap; a changed link is flagged; refused files and strings not crossed carry their codes. All 611 engine tests
and the Python test pass.

## Workstream 4: the app reads notes

The app no longer judges a line by its words: the 17-word list (`kit/words.ts`) is gone.

- **Colours.** A run's notes show by their severity, in the tool panel's run result, the face design panel (its lines
  and each ramp's), and the run log's details. The first line says what the run made, information after it is muted,
  a warning is orange and what blocks later use is red, as the kit's tokens say.
- **Show.** A note with a place, or a warning about an object, has Show beside it. Show picks the object and takes the
  view to the place: a point, the stretch it names, or a point in plan at the middle of its object's RLs, as a seam
  issue has no RL of its own (seam issues now name the model their run makes). The kit holds the line (`NoteLine`)
  and what Show does comes from the app (`ShowNote`), so a panel needs nothing more to get it.
- **Counts.** The status line after a run and each run's row in the run log say how many notes to look at, as
  "1 blocking, 2 warnings".

Checked: the app's typecheck and its 27 tests, which now test the tones, when Show is offered, and the counts. Not
checked by hand in the app when this was written.

## Still to come

All four workstreams are done. Left for later, as agreed: the Issues tab in a Results area (UI plan decision 3), places
on refusals, and a scored gate run built on the codes (the review's housekeeping item).
