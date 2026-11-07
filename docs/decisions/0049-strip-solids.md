# 0049 — One face a strip, and strip solids

Date: 2026-11-07 · Status: accepted (Tim, 2026-11-07, on the Face cut plan tab in the Build Plan doc and its six decisions:
"Go ahead with all 6"). Phase 3 workstreams 5 (surface build: a face's sheets joined) and 6 (cut by a face), and the manual
builder's one face. After 0016 (cut by a surface), 0023 (wall step) and 0029 (strip numbers).

## Why

Tim's standard workflow is a toe offset down dip 10 to 15 times, numbered by a line across, a face per strip, then strip
solids. Tim, 2026-11-07: "the cut by face should allow us to take a large solid and slice it based in the surface
intersection yes? So we build 15 faces and then one large solid can be turned into multiple solids." And: "setting the
attribute for a strip number is all we need to separate the faces. When I do this in Deswik I would just filter the first toe
string and project them down at like -80 to an RL and then select the top strings and project up to RL 500 say. Once I do
that I would select all the strings and create surface by attribute so each surface is created off the unique attributes of
strip number."

Today's cut (0016) closes a surface straight down, so it works only for one that covers the solid in plan, like a
topography. A face covers a strip of ground, so it has to be closed to one side.

## Decision

1. **One face a strip.** A face is told apart by its strip number, the attribute the line drawn across sets (0029), which a
   face keeps from its toe. `face.join` takes faces, a layer's or a pick, and makes one surface for each strip number, as
   Deswik's create surface by attribute does: up and down, several runs, or a build's steps. A face with no strip number
   is refused, saying which. The attribute can be another, and text that isn't a number groups by name: the Builder's
   Join into one face joins a build's sheets by its `build` name, as a toe drawn by hand has no strip number.
2. **Joining.** A strip's faces are welded where their corners meet within 1 mm, stitched where a corner of one lies on an
   edge of another (the two share a line but not its vertices), and any gap they close all round is filled by triangulating
   it in plan. Every change is said: corners welded and the most any moved, triangles the weld collapsed, corners stitched,
   each gap's area and place. A face on its own goes through the same with only exact corners welded, so a sliver hole where
   a wall had no height is filled and said. Faces in pieces that don't meet are refused with how far apart and where, and
   faces that overlap so an edge is shared three ways are refused.
3. **The pit side.** A face's triangles wind upward, which on a pit wall faces the pit. So the cut knows front from behind
   without being told: the front of a face is its pit side.
4. **Closed behind.** A face is closed into the half-space behind it, past the solid (engine rule 5): below it straight down
   to a cap under everything, and over the parts of the solid's outline (in plan, 1 m and 1 % of the height wider) that lie
   behind its top line, from that cap to one above everything, 1 mm into the face's own plan so the two overlap. Which parts
   of the outline lie behind is read from the face's edges: an edge whose wall looks out of the face is its toe (front), one
   whose wall looks into it is its top (behind), with a berm beside an edge looked past to the wall beyond. Below the toe
   the cut goes straight down, and above the top straight up: the run says where the toe stands above the solid's floor, or
   the top is below its top, by how much and where, read straight down the face's own edges.
5. **A face short of the solid at an end is refused** (decision 3 of the plan), with about how much more run-on its line
   needs there and where: past its end, the outline touches both its toe and its top, so nothing says which side is which.
   Faces are built oversized and the cut does the fitting (0023).
6. **Slicing.** `solid.slice_by_faces` takes solids and faces, groups the faces by strip number and joins each strip's.
   Each face is closed behind it, and the faces go from the pit out by how much of the solid lies in front of each, not by
   their numbers. Strip n is what lies in front of face n of what the strips before it left (decision 2 of the plan): each
   is cut from what's left, so the strips and what's behind the last face add up to the solid, with no gaps and nothing
   counted twice. Named `<solid> strip <n>`, with the solid's attributes and `strip`; what's behind the last is
   `<solid> past strip <N>` with `past_strip`. A side with nothing on it is left out and said.
7. **Faces crossing inside the solid are sliced anyway and reported** (decision 4): the solid in front of an earlier face
   that lies behind a later one stays with the earlier strip, and the run says how much and where. Where two faces meet, a
   strip pinches to nothing and its solid would touch itself: that's refused, naming the two faces and the place, as the
   designs there overlap.
8. **A strip in several parts stays one solid** (decision 5), its parts counted. Small pieces are Tim's to filter (Phase 4).
9. **The large solid** (decision 6): `solid.between_surfaces` makes the solid inside a closed string, below one surface or
   grid and above another, with vertical sides on the string: the topography down to the lowest floor inside the pit's
   limit. Where either doesn't cover the string in plan there's no solid, and the area is said. Where the bottom is above the
   top there's nothing. An open string is taken as closed, and said.
10. **One panel for every cut** (UI plan, the kit). Cut solids, from the toolbar: Into strips (the pit picked, a layer of
    faces, with how many faces on how many strips and any with no number), By a surface (today's Cut, out of Properties:
    a surface, grid or RL, keeping below, above or both) and Between surfaces (inside a string, top and bottom). Join a face
    a strip puts one face a strip on a layer of its own. Properties shows the solid and points to the panel.

## Known answers and timings

- A 1000 x 500 x 100 m box sliced by fifteen 45° faces 50 m apart, given out of order: sixteen strips of 7,500,000,
  2,500,000 (fourteen) and 7,500,000 m³ to 1 m³ (exact to the millimetre in practice), in order, each watertight and one
  part. 12 + 240 triangles in, 192 out, 19 ms in a debug build.
- A face with a 20 m berm: 13,100,000 m³ in front of it. A toe at RL 20: straight down below it, 20 m above the floor said.
  A face to RL 60: 40 m below the top said. A face starting 100 m into the box: refused, 103 m more run-on (the outline's
  3 m margin past the side). A 45° and an 80° face crossing: the 273,000 m³ between them reported with the later face.
- An up and a down face with different toe vertices join into one, six corners stitched, area kept to 1 cm².
- A build from a toe drawn by hand on RL -10 (a 45° wall to RL 60, a 10 m berm, a 45° wall to RL 150): its two sheets join by
  its name into one face, refused by strip number, and the pit in front of it is 16,400,000 m³ to 1 m³. In the app the
  Builder's bench joins its three sheets the same way.
- Solid between: a flat top at RL 100 and a floor dipping 1 in 10, inside a 100 m square: 950,000 m³ to 1 mm³; with the
  top over half the square, 500,000 m³ and 5,000 m² said.
- Through the commands, as the app runs them: a toe offset 14 times, numbered across, up to RL 150 and down to RL -50, the
  30 faces joined into 15, the pit made between two surfaces inside its limit and sliced: the same sixteen strips, adding up
  within 0.000000 m³, 23 ms. In the app, through the panel's own buttons (`zd-app --bench --cut`): the same, and the pit
  cut at RL 50 into two halves of 25,000,000 m³.
- The made-up grid pit (`testdata/synthetic/Grid template.zdproj`, 488 Mm³ between Topo and the L floor inside a limit
  across its ten strips; `the_made_up_grid_pit_sliced_by_its_strips`, ignored): six of its seven designs slice, each into
  strips adding up within 0.00001 m³, in 0.4 s (Grid toe to RLs, 40,000 face triangles) to 2.5 s (Every horizon, 412,000).
  Grid ramps is refused for a face crossing itself, and RL switchbacks for strip 3's switchback box at RL 80 reaching strip
  4's berm, where the two faces meet: both faults of the designs, not the slice.
