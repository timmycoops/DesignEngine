"""A made-up pit taken from grids to interrogated blocks, the way the app does it today, to look at the workflow end to end.

The grids are grid_template.py's invented coal model (seams L, M and U dipping north at 3 degrees, rolling, M washed
out in places and U cut off by the weathering), written here as Vulcan grids with made-up qualities, with a seam
model on them. Then, in order, each step a command the app has:

1. Grids: each seam's roof, floor, ash, sulphur and RD imported as grid objects (grid.import_vulcan_seam), and the
   seam model built on the same files (seam.build_model).
2. Faces: a base toe draped on the L floor, offset north five times 60 m apart and numbered across, and one face
   design (65 degrees to the base of weathering, then 45 degrees to the topography) run on all six. No berm: one on
   the base of weathering lies on U's roof where U runs into the weathering, and the cut by that roof is refused there.
   Each face is carried on past the pit, up at 45 degrees to RL 400 and down from the toe at 65 degrees to RL -300, so
   it passes right through the pit it slices: one that stops on the pit's top or floor is refused.
3. Strips: the solid from the topography to the L floor inside the pit's limit, sliced by the faces.
4. Seams: the strips cut by each seam's roof and floor grids, keeping both sides, so each strip ends as overburden,
   U, U-M interburden, M, M-L interburden and L. Each floor grid is renamed to its seam's code first, so the coal
   that lies above it carries the seam's name in `cut_by`: nothing sets a block's `seam` attribute yet.
5. Blocks: lines across the strips every 100 m along strike, numbered `block` west to east and made into vertical
   faces by a second face design, then every seam and waste solid sliced by them.
6. Qualities: every coal block interrogated against the seam model, its seam read from `cut_by`.

    .venv\\Scripts\\python testdata/synthetic/reserves_workflow.py [folder]

Writes the grids, the seam definition and "Reserves workflow.zdp" to testdata/synthetic/reserves/ (or the folder
given), and prints what each step made and how long it took.
"""

import math
import pathlib
import struct
import sys
import time

import zd

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from grid_template import horizons  # noqa: E402
from ramp_template import E, N, at, made  # noqa: E402

PREFIX = "grn"
SEAMS = ["U", "M", "L"]
CELL = 25.0
WEST, EAST, SOUTH, NORTH = -400.0, 2000.0, -500.0, 1100.0
STRIP_WIDTH, STRIPS = 60.0, 6
BLOCK_LENGTH = 100.0
# The base toe, 1.6 km along strike, bent a little; the pit's limit 1.5 km of it.
BASE = [(0.0, 0.0), (800.0, 25.0), (1600.0, 10.0)]
LIMIT_WEST, LIMIT_EAST, LIMIT_SOUTH, LIMIT_NORTH = 50.0, 1550.0, -60.0, 520.0


def qualities(seam, x, y):
    """Made-up raw ash (%), total sulphur (%) and air-dried RD at a place: ash rising east, so blocks differ."""
    base = {"U": 14.0, "M": 18.0, "L": 10.0}[seam]
    ash = base + 6.0 * (x - WEST) / (EAST - WEST) + 2.0 * math.sin(y / 260.0 + x / 410.0)
    sulphur = 0.4 + 0.15 * math.cos(x / 520.0) + 0.002 * ash
    return ash, sulphur, 1.30 + 0.009 * ash


def vulcan_grid(heights, cols, rows):
    """A plain Vulcan grid: header, extents, node count, cell sizes and shape, then each node's value and flag."""
    out = bytearray(b"MADE-UP RESERVES WORKFLOW GRID  ")
    for v in (E + WEST, E + WEST + (cols - 1) * CELL, N + SOUTH, N + SOUTH + (rows - 1) * CELL):
        out += struct.pack(">d", v)
    out += struct.pack(">ii", cols * rows, 0) + struct.pack(">ff", CELL, CELL) + struct.pack(">ii", cols, rows)
    for z in heights:
        out += struct.pack(">fi", z, 1)
    return bytes(out)


def write_grids(folder):
    """Each seam's roof, floor, ash, sulphur and RD, the base of weathering and the topography, and the definition."""
    folder.mkdir(parents=True, exist_ok=True)
    cols, rows = round((EAST - WEST) / CELL) + 1, round((NORTH - SOUTH) / CELL) + 1
    nodes = [(WEST + c * CELL, SOUTH + r * CELL) for r in range(rows) for c in range(cols)]
    values = {}
    for x, y in nodes:
        h = horizons(x, y)
        for seam in SEAMS:
            ash, sulphur, rd = qualities(seam, x, y)
            for variable, z in (("sr", h[f"{seam} roof"]), ("sf", h[f"{seam} floor"]), ("as", ash), ("ts", sulphur), ("rd", rd)):
                values.setdefault(f"{seam.lower()}.{variable}g", []).append(z)
        values.setdefault("bow.sfg", []).append(h["BOW"])
        values.setdefault("topo.sfg", []).append(h["Topo"])
    for name, heights in values.items():
        (folder / f"{PREFIX}{name}").write_bytes(vulcan_grid(heights, cols, rows))
    lines = ["horizon,parent,material,working,roof,floor,rd,as,ts"]
    for seam in SEAMS:
        stem = f"{PREFIX}{seam.lower()}"
        lines.append(f"{seam},,coal,yes,{stem}.srg,{stem}.sfg,{stem}.rdg,{stem}.asg,{stem}.tsg")
    definition = folder / "stack.csv"
    definition.write_text("\n".join(lines) + "\n")
    print(f"1. Wrote {len(values)} Vulcan grids of {cols} x {rows} nodes at {CELL:g} m and {definition.name}")
    return definition


def timed(label, record):
    print(f"   {label}: {len(record['outputs'])} made")
    return made(record)


def import_grids(s, folder, definition):
    g = {}
    for seam in SEAMS + ["bow", "topo"]:
        for id in made(s.run("grid.import_vulcan_seam", dir=str(folder), prefix=PREFIX, seam=seam.lower(), layer="grids")):
            g[s.object(id)["name"]] = id
    # The quality grids are read by the seam model from their files, and shown on a seam's roof from its legend: as
    # objects they'd only draw as sheets at their values' heights.
    qualities_ = [id for name, id in g.items() if name.split()[-1] in ("as", "rd", "ts")]
    s.run("object.delete_many", objects=qualities_)
    g = {name: id for name, id in g.items() if id not in qualities_}
    model = made(s.run("seam.build_model", definition=str(definition), layer="geology", name="Seam model"))[0]
    print(f"   {len(g)} grids on 'grids': {', '.join(sorted(g))}; seam model on 'geology'")
    return g, model


def faces(s, g):
    base = made(s.run("string.create", name="Base toe", layer="toes", points=[at(x, y, 0.0) for x, y in BASE]))[0]
    offsets = made(s.run("string.offset", string=base, distance=STRIP_WIDTH, side="left", times=STRIPS - 1, name="Toe"))
    toes = [base, *offsets]
    s.run("string.number_across", {"strings": toes, "from": at(700.0, -100.0), "to": at(700.0, 600.0), "start": 1})
    for toe in toes:
        s.run("string.drape", string=toe, surface=g["L sf"], crossings=False)
    toes_layer = next(layer["id"] for layer in s.layers() if layer["name"] == "toes")
    steps = [
        {"step": "project", "to": {"kind": "surface", "id": g["BOW sf"]}, "batter": 65.0, "ramps": []},
        {"step": "project", "to": {"kind": "surface", "id": g["TOPO sf"]}, "batter": 45.0, "ramps": []},
        # On past the topography, so the face passes right through the pit's top.
        {"step": "project", "to": {"kind": "rl", "rl": 400.0}, "batter": 45.0, "ramps": []},
    ]
    register = {"kind": "surface", "id": g["L sf"]}
    up = {"name": "Highwall", "direction": "up", "register": register, "condition": 0.1, "steps": steps, "ramps": []}
    # And down from the toe on the L floor, so it passes right through the pit's floor.
    down = {"name": "Highwall down", "direction": "down", "register": register, "condition": 0.1, "ramps": [],
            "steps": [{"step": "project", "to": {"kind": "rl", "rl": -300.0}, "batter": 65.0, "ramps": []}]}
    lines = [{"layer": toes_layer, "profile": name, "side": "left", "run_on": [100.0, 100.0]} for name in ("Highwall", "Highwall down")]
    design = made(s.run("face.save_design", name="Highwall", content={"layer": "faces", "lines": lines, "profiles": [up, down]}))[0]
    s.run("face.project", design=design)
    return [o["id"] for o in s.objects(layer="faces") if o["kind"] == "surface"]


def strips(s, g, highwalls):
    limit = made(s.run("string.create", name="Pit limit", layer="limits", closed=True,
                       points=[at(LIMIT_WEST, LIMIT_SOUTH), at(LIMIT_EAST, LIMIT_SOUTH), at(LIMIT_EAST, LIMIT_NORTH), at(LIMIT_WEST, LIMIT_NORTH)]))[0]
    pit = made(s.run("solid.between_surfaces", top=g["TOPO sf"], bottom=g["L sf"], string=limit, layer="pit", name="Pit"))[0]
    pieces = made(s.run("solid.slice_by_faces", solid=pit, faces=highwalls, layer="strips"))
    kept = [p for p in pieces if "strip" in s.object(p)["attributes"]]
    print(f"   {len(kept)} strips and what's past the last face, from a pit of {s.object(pit)['volume']:,.0f} m3")
    return kept


def cut(s, solids, grid, above, below):
    """Cuts by a grid, the part above onto one layer and below onto another; returns (above, below)."""
    pieces = made(s.run("solid.cut_by_surface", solids=solids, surface=grid, keep="both", layer="cutting"))
    ups = [p for p in pieces if s.object(p)["attributes"].get("cut_side") == "above"]
    downs = [p for p in pieces if p not in ups]
    if above:
        s.run("object.move", objects=ups, layer=above)
    return ups, downs


def seams(s, g, strip_solids):
    """Top down, each seam cut at its floor and what's above cut at its roof: the waste above the roof, the coal below
    it. The last seam is cut at its roof only, since the pit stops on its floor and a cut on the solid's own bottom
    leaves it uncut. Each roof grid is renamed to its seam's code first, so the coal's `cut_by` names its seam."""
    for seam in SEAMS:
        s.run("object.rename", object=g[f"{seam} sr"], name=seam)
    rest, counts = strip_solids, {}
    waste = ["overburden", "U-M interburden", "M-L interburden"]
    for k, seam in enumerate(SEAMS):
        if k + 1 < len(SEAMS):
            upper, below_floor = cut(s, rest, g[f"{seam} sf"], None, None)
            if rest is not strip_solids:
                s.run("object.delete_many", objects=rest)
        else:
            upper, below_floor = rest, []
        above_roof, coal = cut(s, upper, g[f"{seam} sr"], waste[k], None)
        s.run("object.move", objects=coal, layer=f"{seam} coal")
        counts[waste[k]], counts[f"{seam} coal"] = len(above_roof), len(coal)
        # A solid the roof cut refused is left whole on its own layer to be seen, not lost.
        cut_names = [s.object(p)["name"] for p in above_roof + coal]
        refused = [u for u in upper if not any(n.startswith(s.object(u)["name"] + " ") for n in cut_names)]
        if refused:
            s.run("object.move", objects=refused, layer=f"refused at {seam} roof")
            counts[f"refused at {seam} roof"] = len(refused)
        s.run("object.delete_many", objects=[u for u in upper if u not in refused])
        rest = below_floor
    print("   " + ", ".join(f"{n} on '{layer}'" for layer, n in counts.items()))


def blocks(s):
    lines = []
    for k in range(1, round((LIMIT_EAST - LIMIT_WEST) / BLOCK_LENGTH)):
        x = LIMIT_WEST + k * BLOCK_LENGTH
        lines += made(s.run("string.create", name="Block line", layer="block lines", points=[at(x, LIMIT_SOUTH - 200.0, -300.0), at(x, LIMIT_NORTH + 200.0, -300.0)]))
    s.run("string.number_across", {"strings": lines, "from": at(LIMIT_WEST - 50.0, 200.0), "to": at(LIMIT_EAST + 50.0, 200.0), "start": 1,
                                   "attribute": "block", "name": "Block line #"})
    lines_layer = next(layer["id"] for layer in s.layers() if layer["name"] == "block lines")
    profile = {"name": "Block cut", "direction": "up", "register": {"kind": "rl", "rl": -300.0}, "condition": 0.1,
               "steps": [{"step": "project", "to": {"kind": "rl", "rl": 400.0}, "batter": 89.9, "ramps": []}], "ramps": []}
    line = {"layer": lines_layer, "profile": "Block cut", "side": "right", "run_on": [50.0, 50.0]}
    design = made(s.run("face.save_design", name="Block cuts", content={"layer": "block faces", "lines": [line], "profiles": [profile]}))[0]
    s.run("face.project", design=design)
    cutters = [o["id"] for o in s.objects(layer="block faces") if o["kind"] == "surface"]
    for layer in ["overburden", "U-M interburden", "U coal", "M-L interburden", "M coal", "L coal"]:
        solids = [o["id"] for o in s.objects(layer=layer) if o["kind"] == "solid"]
        pieces = made(s.run("solid.slice_by_faces", solids=solids, faces=cutters, attribute="block", layer=f"{layer} blocks"))
        print(f"   {len(solids)} on '{layer}' into {len(pieces)} on '{layer} blocks'")


def interrogate(s, model):
    coal = [o["id"] for seam in SEAMS for o in s.objects(layer=f"{seam} coal blocks") if o["kind"] == "solid"]
    record = s.run("solid.interrogate", solids=coal, model=model, seam_attribute="cut_by")
    warnings = [m for m in record["messages"] if "warning" in m.lower()]
    print(f"   {len(coal)} coal blocks interrogated; {len(record['messages'])} lines in the run record, {len(warnings)} warnings")
    for m in record["messages"][:8]:
        print("     " + m[:200])


def main():
    here = pathlib.Path(__file__).resolve().parent
    folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else here / "reserves"
    definition = write_grids(folder)
    s = zd.Session()
    started = time.perf_counter()
    g, model = import_grids(s, folder, definition)
    print(f"   {time.perf_counter() - started:.1f} s")
    for label, step in [
        ("2. Faces", lambda: faces(s, g)),
        ("3. Strips", lambda: strips(s, g, highwalls)),
        ("4. Seams", lambda: seams(s, g, strip_solids)),
        ("5. Blocks", lambda: blocks(s)),
        ("6. Qualities", lambda: interrogate(s, model)),
    ]:
        print(label)
        started = time.perf_counter()
        result = step()
        if label == "2. Faces":
            highwalls = result
            print(f"   {len(highwalls)} faces on 'faces'")
        if label == "3. Strips":
            strip_solids = result
        print(f"   {time.perf_counter() - started:.1f} s")
    s.run("layer.delete", layer="cutting")
    path = folder / "Reserves workflow.zdp"
    s.save(path)
    print(f"Saved {path}")


if __name__ == "__main__":
    main()
