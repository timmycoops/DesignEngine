"""A large made-up pit to try the app on at scale: thousands of block solids and a topography of a couple of million
triangles, at made-up coordinates.

It's the workflow demo's pit (workflow_demo.py) made bigger, built the same way, command by command:

- the geology is grid_template.py's invented coal model (L, M and U dipping north at 3 degrees, rolling, M washed out
  in places, U cut off by the weathering) as Vulcan grids at 25 m with made-up qualities, and a seam model on them;
- a base toe 5 km along strike, offset north 60 m a strip (30 strips unless set), numbered across and draped on the L
  floor; one highwall design on them all: 65 degrees to the base of weathering, a 30 m berm, 45 degrees to the
  topography, carried on to RL 400 up and RL -300 down so each face passes right through the pit (no roads);
- the pit from the topography to the L floor inside the limit, sliced into strips, cut by the seam model into
  overburden, U, U-M, M, M-L and L, then cut into 100 m blocks by polygons made from block lines; every block measured
  and every coal block interrogated;
- a second topography, sampled every 5 m from the same ground over the whole area (about 1.8 million triangles), on
  'topography 5 m', to merge the blocks into and to load the view.

    .venv\\Scripts\\python testdata/synthetic/large_pit.py [--strips 30] [--length 5000] [--benches] [folder]

--benches cuts every seam piece at 10 m bench levels too, several times the blocks. Writes the grids and "Large
pit.zdp" to the folder (testdata/synthetic/reserves/large pit unless given), and prints each step's count and time.
"""

import argparse
import math
import pathlib
import struct
import sys
import time

import zd

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from grid_template import horizons, topography  # noqa: E402
from ramp_template import E, N, at, made  # noqa: E402

PREFIX = "lrg"
SEAMS = ["U", "M", "L"]
CELL = 25.0
FINE_CELL = 5.0
STRIP_WIDTH = 60.0
BLOCK_LENGTH = 100.0
BERM, MIN_BENCH = 30.0, 3.0

SHOWN = {"0 pit limit", "1 toes", "9 blocks", "topography 5 m"}


def qualities(seam, x, y, west, east):
    """Made-up raw ash (%), total sulphur (%) and air-dried RD at a place: ash rising east, so blocks differ."""
    base = {"U": 14.0, "M": 18.0, "L": 10.0}[seam]
    ash = base + 6.0 * (x - west) / (east - west) + 2.0 * math.sin(y / 260.0 + x / 410.0)
    sulphur = 0.4 + 0.15 * math.cos(x / 520.0) + 0.002 * ash
    return ash, sulphur, 1.30 + 0.009 * ash


def vulcan_grid(heights, cols, rows, west, south):
    """A plain Vulcan grid: header, extents, node count, cell sizes and shape, then each node's value and flag."""
    out = bytearray(b"MADE-UP LARGE PIT GRID          ")
    for v in (E + west, E + west + (cols - 1) * CELL, N + south, N + south + (rows - 1) * CELL):
        out += struct.pack(">d", v)
    out += struct.pack(">ii", cols * rows, 0) + struct.pack(">ff", CELL, CELL) + struct.pack(">ii", cols, rows)
    for z in heights:
        out += struct.pack(">fi", z, 1)
    return bytes(out)


def write_grids(folder, area):
    """Each seam's roof, floor, ash, sulphur and RD, the base of weathering and the topography at 25 m, the seam
    definition, and the topography every 5 m as XYZ."""
    west, east, south, north = area
    cols, rows = round((east - west) / CELL) + 1, round((north - south) / CELL) + 1
    values = {}
    for r in range(rows):
        y = south + r * CELL
        for c in range(cols):
            x = west + c * CELL
            h = horizons(x, y)
            for seam in SEAMS:
                ash, sulphur, rd = qualities(seam, x, y, west, east)
                for variable, z in (("sr", h[f"{seam} roof"]), ("sf", h[f"{seam} floor"]), ("as", ash), ("ts", sulphur), ("rd", rd)):
                    values.setdefault(f"{seam.lower()}.{variable}g", []).append(z)
            values.setdefault("bow.sfg", []).append(h["BOW"])
            values.setdefault("topo.sfg", []).append(h["Topo"])
    for name, heights in values.items():
        (folder / f"{PREFIX}{name}").write_bytes(vulcan_grid(heights, cols, rows, west, south))
    lines = ["horizon,parent,material,working,roof,floor,rd,as,ts"]
    for seam in SEAMS:
        stem = f"{PREFIX}{seam.lower()}"
        lines.append(f"{seam},,coal,yes,{stem}.srg,{stem}.sfg,{stem}.rdg,{stem}.asg,{stem}.tsg")
    definition = folder / "stack.csv"
    definition.write_text("\n".join(lines) + "\n")

    fine_cols, fine_rows = round((east - west) / FINE_CELL) + 1, round((north - south) / FINE_CELL) + 1
    fine = folder / "topography_5m.xyz"
    with fine.open("w") as out:
        for r in range(fine_rows):
            y = south + r * FINE_CELL
            out.write("".join(f"{E + west + c * FINE_CELL:.1f} {N + y:.1f} {topography(west + c * FINE_CELL, y):.3f}\n" for c in range(fine_cols)))
    print(f"Wrote {len(values)} Vulcan grids of {cols} x {rows} nodes at {CELL:g} m, {definition.name}, "
          f"and the topography at {FINE_CELL:g} m, {fine_cols} x {fine_rows} nodes")
    return definition, fine


def step(label, started):
    print(f"   {label} in {time.perf_counter() - started:.1f} s", flush=True)
    return time.perf_counter()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--strips", type=int, default=30)
    parser.add_argument("--length", type=float, default=5000.0)
    parser.add_argument("--benches", action="store_true")
    parser.add_argument("folder", nargs="?", default=str(HERE / "reserves" / "large pit"))
    a = parser.parse_args()
    folder = pathlib.Path(a.folder)
    folder.mkdir(parents=True, exist_ok=True)

    limit_west, limit_east = 50.0, a.length - 50.0
    limit_south, limit_north = -60.0, a.strips * STRIP_WIDTH - 40.0
    area = (-500.0, a.length + 500.0, -600.0, limit_north + 900.0)
    base = [(0.0, 0.0), (a.length / 2, 40.0), (a.length, 10.0)]
    middle = a.length / 2

    t0 = t = time.perf_counter()
    definition, fine = write_grids(folder, area)
    t = step("grids written", t)

    s = zd.Session()
    g = {}
    for seam in SEAMS + ["bow", "topo"]:
        for id in made(s.run("grid.import_vulcan_seam", dir=str(folder), prefix=PREFIX, seam=seam, layer="grids")):
            g[s.object(id)["name"]] = id
    qualities_ = [id for name, id in g.items() if name.split()[-1] in ("as", "rd", "ts")]
    s.run("object.delete_many", objects=qualities_)
    g = {name: id for name, id in g.items() if id not in qualities_}
    model = made(s.run("seam.build_model", definition=str(definition), layer="geology", name="Seam model"))[0]
    topo_fine = made(s.run("surface.import", path=str(fine), layer="topography 5 m", name="Topography 5 m"))[0]
    t = step(f"{len(g)} grids, the seam model and 'Topography 5 m' imported", t)

    toe = made(s.run("string.create", name="Base toe", layer="1 toes", points=[at(x, y, 0.0) for x, y in base]))[0]
    limit = made(s.run("string.create", name="Pit limit", layer="0 pit limit", closed=True,
                       points=[at(limit_west, limit_south), at(limit_east, limit_south), at(limit_east, limit_north), at(limit_west, limit_north)]))[0]
    offsets = made(s.run("string.offset", string=toe, distance=STRIP_WIDTH, side="left", times=a.strips - 1, name="Toe", layer="1 toes"))
    toes = [toe, *offsets]
    s.run("string.number_across", {"strings": toes, "start": 1, "from": at(middle, -100.0), "to": at(middle, limit_north + 200.0)})
    for k in toes:
        s.run("string.drape", string=k, surface=g["L sf"], crossings=False)
    toes_layer = next(layer["id"] for layer in s.layers() if layer["name"] == "1 toes")
    steps = [
        {"step": "project", "to": {"kind": "surface", "id": g["BOW sf"]}, "batter": 65.0, "ramps": []},
        {"step": "berm", "width": BERM, "min_bench": MIN_BENCH},
        {"step": "project", "to": {"kind": "surface", "id": g["TOPO sf"]}, "batter": 45.0, "ramps": []},
        {"step": "project", "to": {"kind": "rl", "rl": 400.0}, "batter": 45.0, "ramps": []},
    ]
    register = {"kind": "surface", "id": g["L sf"]}
    up = {"name": "Highwall", "direction": "up", "register": register, "condition": 0.1, "steps": steps, "ramps": []}
    down = {"name": "Highwall down", "direction": "down", "register": register, "condition": 0.1, "ramps": [],
            "steps": [{"step": "project", "to": {"kind": "rl", "rl": -300.0}, "batter": 65.0, "ramps": []}]}
    lines = [{"layer": toes_layer, "profile": name, "side": "left", "run_on": [100.0, 100.0]} for name in ("Highwall", "Highwall down")]
    design = made(s.run("face.save_design", name="Highwall", content={"layer": "2 faces", "lines": lines, "profiles": [up, down]}))[0]
    s.run("face.project", design=design)
    faces = [o["id"] for o in s.objects(layer="2 faces", kind="surface")]
    t = step(f"{len(toes)} toes, {len(faces)} faces", t)

    pit = made(s.run("solid.between_surfaces", top=g["TOPO sf"], bottom=g["L sf"], string=limit, layer="3 pit", name="Pit"))[0]
    strips = made(s.run("solid.slice_by_faces", solid=pit, faces=faces, layer="4 strips"))
    s.run("object.delete", object=strips[-1])
    strips = strips[:-1]
    t = step(f"the pit, {s.object(pit)['volume']:,.0f} m3, sliced into {len(strips)} strips", t)

    pieces = made(s.run("solid.cut_by_seams", solids=strips, model=model, layer="5 seam pieces"))
    t = step(f"{len(pieces)} seam pieces", t)
    if a.benches:
        pieces = made(s.run("solid.cut_by_benches", solids=pieces, layer="6 benches", rules=[{"from": -300.0, "to": 500.0, "every": 10.0}]))
        t = step(f"{len(pieces)} benches", t)

    first = made(s.run("string.create", name="Block line 0", layer="7 block lines",
                       points=[at(limit_west, limit_south - 200.0, -300.0), at(limit_west, limit_north + 200.0, -300.0)]))[0]
    for name, y in (("South closing line", limit_south - 100.0), ("North closing line", limit_north + 100.0)):
        s.run("string.create", name=name, layer="7 block lines", points=[at(limit_west - 100.0, y, -300.0), at(limit_east + 100.0, y, -300.0)])
    count = round((limit_east - limit_west) / BLOCK_LENGTH)
    s.run("string.offset", string=first, distance=BLOCK_LENGTH, side="right", times=count, name="Block line", layer="7 block lines")
    block_lines = [o["id"] for o in s.objects(layer="7 block lines", kind="string")]
    polygons = made(s.run("string.polygons_from_lines", strings=block_lines, layer="8 block polygons", name="B#"))
    blocks = made(s.run("solid.cut_by_polygons", solids=pieces, polygons=polygons, layer="9 blocks"))
    t = step(f"{len(polygons)} block polygons, {len(blocks):,} blocks", t)

    s.run("solid.measure", solids=blocks)
    coal = [b for b in blocks if s.object(b)["attributes"].get("material") == "coal"]
    s.run("solid.interrogate", solids=coal, model=model)
    t = step(f"{len(blocks):,} blocks measured, {len(coal):,} coal blocks interrogated", t)

    for layer in s.layers():
        s.set_layer_visible(layer["name"], layer["name"] in SHOWN)
    path = folder / "Large pit.zdp"
    s.save(path)
    triangles = len(s.mesh(topo_fine)["triangles"])
    step(f"saved {path} ({path.stat().st_size / 1e6:,.0f} MB)", t)
    print(f"Done in {time.perf_counter() - t0:.0f} s: {len(blocks):,} blocks on '9 blocks', 'Topography 5 m' {triangles:,} triangles; "
          f"on show: {', '.join(sorted(SHOWN))}")


if __name__ == "__main__":
    main()
