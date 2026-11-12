# 0060 — The app at scale: only what changed is sent

Status: agreed 12 November 2026 (Tim: "Agree all four", the Build Plan's "App at scale plan" tab). Workstream 1 of 4.

## Why

At 43,000 blocks a change to one solid cost seconds, because the app was sent what didn't change. After every run,
undo, hide or show the shell sent the whole project: every object with every attribute, 152 MB of text on the store
bench's made-up project (43,000 blocks of 108 triangles, 250 attributes each), 0.4 s to build in the shell alone. The app
reads 12 attribute names in all. The run log was sent whole after every change too (the review of 10 November, finding 2
and item 5).

## Decision

Agreed with the plan:

1. **While a run goes you can look:** camera, snap, pick, box select, section, Show and Properties. Anything that changes
   the project waits until it ends (workstream 2).
2. **Progress and Cancel now** for the long commands (workstream 2).
3. **A row carries the attributes the app reads by name**, about 12, and Properties fetches the rest for the object
   picked.
4. **The targets** as on the plan tab: hide one solid under 50 ms with under 10 kB sent; a change to one solid on screen
   under 200 ms; the project under 10 MB when opened; box select all under 100 ms with nothing but ids sent; camera and
   snap answering through a 60 s run; a slice cancelled within 1 s with nothing applied.

## Workstream 1 (this record's first PR): only what changed is sent

- **The shell keeps what it last sent** (`app/src-tauri/src/changes.rs`): each object's revision as sent and whether it
  was hidden, and their order. After any change it compares the project against that and sends the objects added or
  changed and the ids of those gone. One rule covers runs, undo, redo, hide, show and open, so no command reports
  anything new for it.
- **Each reply says what it follows on from.** A reply carries the number of what the app should hold before it and
  after it. The app takes it only if it holds that; if it doesn't (two replies crossed, or the window reloaded) it asks
  for the whole project once. Opening, a new project, or the scene origin moving sends the whole project.
- **Order.** Objects are listed in the order they were made. A new object goes at the end; an object brought back by an
  undo goes back to its place, so then the reply carries every id in order (1.5 MB at 43,000), which it otherwise leaves
  out.
- **A row's attributes** are those the app reads by name (`ROW_ATTRIBUTES`: build, build_id, build_on, build_record,
  build_step, face_design, not_a_solid, road, road_face, step, strip) and the `road_top` ones a crest records (0041).
  Properties shows every attribute from the object's details, as before. The Export solids dialog fetches every
  attribute of the solids it names files for (`objects_attributes`). Rows other than strings leave out the string
  fields.
- **The run log** is fetched from where the app has got to, by session, so a run sends its own entry and not the log.
- **The app** (`app/src/projectStore.ts`) takes each reply in turn and gives the panels and viewport the whole project
  as they've always had it. Objects that didn't change keep their rows. No panel or tool works differently.

## Checked

On the store bench's made-up 43,000 blocks (`store_scale -- 43000 250`), release build, in the shell
(`changes::tests::at_scale`, ignored, run with `ZD_SCALE_PROJECT`):

| Action | Before | Now |
|---|---|---|
| Open: the whole project | 152.4 MB, 0.21 s to build, 0.19 s to encode | 9.0 MB, 85 ms to build, 12 ms to encode |
| Hide one solid | 152 MB, 0.42 s | 0.7 kB, 13 to 34 ms |
| Move one solid to another layer, or undo it | 152 MB, 0.40 s | under 1 kB, 13 to 34 ms |
| Delete one, then undo it | 152 MB each | 0.5 kB, then 1.5 MB (the order) |
| Nothing changed | 152 MB | 0.4 kB |

Reading the whole project back as JSON in Rust, a stand-in for the window, took 2.1 s before and 65 ms now.

Tests: the shell's changes taken in turn (a create, a revision, a hide, a delete, two undos and two redos) give the same
objects in the same order as the whole project, a change is sent alone, and nothing changed sends nothing. In the app,
the same in `projectStore.test.mjs`, with replies that don't follow on refused, unchanged rows kept, and the run log
added to and started again for a new session.

The store bench now writes an empty run log, so the project it makes opens in the app (it wrote `[1]`, which no run
log reads as).

## Still to come

The shell still builds the snap index and sends a layer's solids as one mesh after a change to one of them, and hiding a
solid still sends its layer's mesh again: workstreams 3 (hide and select without a mesh) and 4 (plan tiles). A run still
holds the session for its length: workstream 2.
