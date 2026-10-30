# 0018 — One file, one solid

Date: 2026-10-31 · Status: proposed (raised by Tim, 2026-10-31)

## Why

Decision 0011 made one solid per piece where closed pieces touch along an edge. On the golden pit that
turned 990 files into 3,042 solids, up to 18 from one file, named `<name> part 1` and so on. A solid has
to compare with the file it came from, and with Deswik's or Spry's figure for it: one name and one volume
there, up to 18 here. Files whose pieces do not touch were kept whole all along, so the split also hung on
a detail of the geometry nobody sees.

## Decision

1. **One file makes one solid**, under the file's name, holding all its pieces. Its volume is the file's.
   The output of `solid.import` is now `solid`, not `solids`.
2. **Touching edges are still taken apart** (vertices copied, no coordinate moved); the pieces stay in the
   solid as closed shells, and the run record says how many.
3. **The solid is searched as it will be saved.** Taken apart, faces that shared an edge no longer do, so
   pieces lying on each other show only then. As read, such a file is refused. With repair, the union
   works from the faces as they met, before they were taken apart, and the change in volume is recorded;
   if it cannot resolve them, the file is refused and says why.
4. **Nothing comes off by size here.** Import stays exact, so it compares. A small-piece limit is a
   separate decision; the figures for one are below.

A cut already keeps one part to a side, whatever pieces it holds, so import and cut now follow one rule.

## Evidence

- Two 10 m cubes meeting along an edge: one solid, named for the file, 2,000 m³; cut at half height, two
  parts of 1,000 m³ (to 1e-6 m³), each closed. Two tetrahedra hinged on one edge with a face each folded
  flat on the other: refused as read; with repair one solid of 500/3 + 100 m³ (to 1e-6 m³), no crossing.
- The golden pit's 42,223 relimiting solids, file by file against decision 0017's build:
  - **42,218 solids from 42,218 files**, and no file's volume changes (to 1e-6 m³). 41,917 import as read,
    301 with repair and 5 are refused, each file the same as before.
  - **4,009 solids hold several pieces**, 10,833 in all, 82 at most: the 990 that used to split and 3,019
    that never did, because their pieces do not touch.
  - Every one of the 4,009, searched again from its saved mesh, is clean. One of them, a 0.695 m³ coal
    solid, crossed once joined: its union resolves 12 crossings with no change in volume at 3 decimals.
- The whole-pit cut, by the mined-out topography: 42,218 solids into **49,764 parts in 107 s** (2.5 ms a
  solid), every part checked, no whole-surface retries. **One solid is left uncut and named** (two
  before); the 0.695 m³ solid now cuts. Each solid and its parts agree to 0.27 m³ at most (3.2e-5 of
  8,407 m³).
- For a small-piece limit, taking off every piece under the limit but the largest in each solid:

  | Limit | Pieces off | Solids changed | Volume off | Most off one solid |
  |---|---|---|---|---|
  | 0.001 m³ | 486 | 284 | 0.1 m³ | 0.01 m³ |
  | 1 m³ | 4,621 | 2,991 | 781 m³ | 4.1 m³ |
  | 5 m³ | 5,577 | 3,448 | 3,050 m³ | 17.7 m³ |
  | 10 m³ | 5,911 | 3,616 | 5,432 m³ | 29.3 m³ |

  The pit is 206 M m³, so 5 m³ takes off 1.5e-5 of it. It would not make the pieces go away: 961 solids
  hold two or more pieces of 5 m³ or more, 11 at most. 1,933 solids are under 5 m³ in all (3,964 m³).
