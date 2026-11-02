# 0030 — A ramp is a string across the strips

Date: 2026-11-02 · Status: accepted. Tim agreed it after trying 0028 on five strips of PIT C. This changes items 1 to 3 of 0028, and the road itself is as it was. 0031 changes item 2: a held end goes by where the string crosses the toe, not the crest. 0032 changes item 6: a wall takes several roads

## Why

A planner's usual design is a toe offset down dip 10 to 15 times, a face for each strip, then strip solids, and ramps move from
strip to strip with coal dip and burden thickness. 0028 set a ramp at one point on one string, and a wall row named one ramp, for
every line on the profile. So a ramp on a second strip couldn't climb the walls of a ramp on the first, and every ramp after the
first started with no walls at all, which refused the whole run. Deswik uses a string that crosses the strips. Tim: "you manually
move the string to the correct place".

## Decision

1. **A ramp has a string drawn across the strips, and each line it crosses gets a road.** One vertex a strip in practice. A
   ramp on one strip is a short string across that strip.
2. **Where it lands comes from the strip's face with no roads on it.** With its start held, that's where the string crosses the
   toe of the ramp's first wall. With its end held, it's where the string crosses the crest of its last, and the run moves the
   start along the toe as 0028 item 3 says. That's one more run of each strip with a ramp on it.
3. **Moving a vertex and running again moves that strip's road,** and nothing else. A string that crosses more than once goes by
   the first along it, and the run says so.
4. **A strip it doesn't cross runs plain,** and the run names it once for the ramp, not once for each strip. If it crosses no
   line, or its string has been deleted, the ramp says that and the walls still build.
5. **Each ramp has a column in the profile grid.** A wall's cell fills in the ramp's colour with a click, so a ramp reads as a bar
   up the walls it climbs, across any berms. A wall can hold several ramps, as they usually cross different strips. Walls
   with one ramp must still come together.
6. **A wall takes one road a strip for now.** Where two ramps on a wall cross one strip, the first listed keeps it, and the other
   says so, for that strip only, and builds on its others. Two roads on one wall means two lots of cuts in one stretch, which
   needs its own work.
7. **`+ Ramp` draws the string.** Click across the strips and press Enter. It goes on a `ramps` layer as `Ramp A`, and the ramp
   takes every wall that finishes on a surface, so there's a road on the first run. Any string can take over from it, as one
   imported or drawn by hand. Removing a ramp takes the string drawn for it, and undo brings that back.
8. **A ramp string isn't a strip.** One left on a line's layer isn't run as a line.
9. **A ramp with no walls doesn't stop a run.** It's a ramp part set up, not a mistake: its own line says so. A refused run names
   everything wrong at once, and the refusal goes when the design changes.
10. **Strips run side by side.** They're independent, and held ends take a strip several runs. Reads were tracked behind a
    `RefCell`, which a `Mutex` now does.

## Results

Synthetic, known answers: a string bending across three strips puts each road's start on its crossing to 1e-6 m. Moving a vertex
moves that strip's alone. Held at the end, each road comes out within 0.25 m of where the string crosses its crest, 49 ms for all
three. A strip it misses runs plain, and two ramps on one wall take a strip each.

PIT B, a real toe offset 60 m to 5 strips one way and 6 the other, numbered by a line across, with one 30 m road at 10 % on a
string stepping 15 m a strip, up all five walls, each way, held at either end. 8 runs, 44 faces, 42,600 to 61,200 triangles out.
Every face built with nothing crossing, and every strip said how far its road got and why it stopped.

| Held at | Side by side | One at a time |
|---|---|---|
| The start, 5 or 6 strips | 300 to 400 ms | 1,230 to 1,960 ms |
| The end, 5 or 6 strips | 1,670 to 2,160 ms | 6,240 to 8,390 ms |

The roads of 0028 run the same through a ramp string: 40 runs from each of five walls, each one sheet with nothing crossing, 260
to 350 ms held at the start and 360 to 1,620 ms held at the end.

Real app bench: a line and a string picked by real clicks in the viewport, Enter finishing and Esc abandoning. Four strips with one
road each through the bridge in 572 ms, 46 objects made.

## What this leaves

- **Two roads on one wall of one strip.**
- **Neighbouring faces aren't checked against each other.** A road and its entrance move a wall back towards the next strip, and
  strip solids need faces that don't cross.
- **Held at the end is over the gate's 1 s,** at about 2 s for six strips. It could keep each ramp's last start to begin from.
- **A road that can't come out where it's held says how far off it is.** On PIT B that was up to 330 m, where the toe rises the
  wrong way along the strip, which is the data, but it's a lot to read.
- **A ramp string draws in its layer's colour,** not its ramp's. A click on its name lights it up.
- **Ramps saved before this are dropped when a design opens,** as they have no string to carry over, and need entering again.
