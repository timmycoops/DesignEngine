# 0029 — Strips are numbered across

Date: 2026-11-02 · Status: accepted. Tim tried ramps on five strips of PIT C and couldn't tell one string from the next

## Why

A planner's standard process is to offset a base line a number of times, then set an attribute by direction: the base line
becomes, say, strip 20, and a from and to position numbers the rest in order. We had one offset at a time and a name that grew
by "offset 60 m right" each time, with a face's strings putting all of that in front of their own. What told strips apart ran
off the end of every box. Attributes were in the Layers plan's workstream 6, but that's colouring and filtering by them. Nothing
could set one by hand.

## Decision

1. **A strip's number is an attribute.** `strip` on the string, a number, so commands can read it, and it doesn't have to be
   worked out from a name.
2. **Offset makes as many as it's asked for.** Each is measured from the source, so nothing builds up from offsetting an offset.
3. **A line across numbers them.** `string.number_across` takes two points. Each string gets the next number in the order the
   line first crosses it, from a start and a step, which can be negative. Drawing it the other way reverses them. It revises
   the strings, so undo takes the whole numbering back. Geometry doesn't change.
4. **A name comes with the number.** `Strip #`, or anything else with a `#` in it, such as `S#`. With no `#` the number goes on
   the end. Everything a run makes takes its name from the string, so a toe is `Strip 20 Highwall toe 1`.
5. **It numbers what's marked, or else the string's layer.** The Number row in Properties takes what's marked. With nothing
   marked it takes what's showing on the selected string's layer that a face run didn't make, so old toes and crests under the
   line aren't numbered. Its button says how many.
6. **A string the line misses is left alone,** and the run names it. One it crosses twice goes by the first, and the run says so.
7. **What a run makes keeps its string's attributes,** as each part of a cut solid does (0016), so a road edge and a face know
   their strip. Phase 4's strip solids and the reserves record by strip can read it from there.
8. **The tree groups by strip.** A layer's objects with one number share a row, in order, and names sort as a person would put
   them, with Strip 9 before Strip 10.
9. **A click on a line lights up its string,** or everything on its layer, in the viewport and the tree.

## Results

Synthetic, known answers: three offsets of an L-shaped string at 10 m land on their mitres to the micrometre. Parallel strings
number 20, 21, 22 whichever order they came in, and reverse when the line's drawn the other way. A closed string counts its last
side, and a string through the line at a vertex crosses once.

## What this leaves

- **Colouring and filtering by attribute** stay in workstream 6, with setting any other attribute by hand.
- **The number isn't drawn in the viewport.** Labels on strings wait until they're wanted: for now a click on a line shows it.
- **Nothing checks two strips for the same number.** Running it on two layers from one start gives them both.
