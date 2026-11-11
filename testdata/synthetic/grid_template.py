"""A project to work on walls that finish on grids, with made-up seam grids at the coordinates of ramp_template.py.

The grids are a small coal model invented for testing: a lower, middle and upper seam (L, M, U) dipping north at
3 degrees, each roof and floor rolling a few metres every 100 to 200 m the way real seam grids do, under a weathered
zone and a topography with a creek across the strips. Three things in it are there because they break walls:
- the rolls, so a line draped on a grid wrinkles along and up and down;
- a washout where M thins to nothing, so its roof lies on its floor;
- U running into the base of weathering at the south end and under the creek, so it lies on the weathering there.

Ten strips 80 m apart from the bent base toe of ramp_template.py, draped on the L floor, with ramp strings A, B and C,
and seven designs, each on its own layer of faces:
- Grid walls: the toe on the L floor and walls to the grids, 65 degrees through rock and 45 through the weathering,
  with 30 m berms. No ramps, so a fault is the wall's own.
- Grid ramps: the same with ramps A (held at its start) and B (held at its end) up every wall.
- Flat toe to grids: the toe flat on RL -20, below every L floor, and the same walls with the ramps.
- Grid toe to RLs: the toe on the L floor, walls to RL 50, 80, 110 and 140, with the ramps.
- Every horizon: a wall to every roof and floor with 10 m berms and a 3 m minimum bench, so coal too thin to stand
  a berm on, the washout and the weathered U all leave a berm out.
- Grid switchbacks: the walls of Grid walls with ramp C, held at its start, turning back on every berm, so its pads
  lie on the M floor, the U floor and the base of weathering (decision 0038).
- RL switchbacks: the walls of Grid toe to RLs with ramp C turning back on every berm, on level pads at RL 50, 80 and
  110.

    .venv\\Scripts\\python testdata/synthetic/grid_template.py [folder]

Writes the grids as XYZ files in grids/ beside this script, which import by hand with the Surface button, and
"Grid template.zdp" with them imported, beside this script or in the folder given. Then it runs every design
without saving the runs and prints what each strip didn't build, any face with triangles crossing or that isn't one
sheet, and every road that stopped: the list of faults to work through.
"""

import math
import pathlib
import re
import sys
import time

import zd

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from ramp_template import E, N, at, made, ramp_string, strips  # noqa: E402

STRIPS = 10
CELL = 25.0
# The grids reach well past the strips: 1 km before and after them along strike, and to the north past the crest
# of the last strip.
WEST, EAST, SOUTH, NORTH = -1000.0, 4500.0, -1000.0, 2000.0
DIP = math.tan(math.radians(3.0))
ROAD = {"width": 30.0, "grade": 10.0}


def smoothstep(edge0, edge1, value):
    t = min(max((value - edge0) / (edge1 - edge0), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def rolls(x, y, phase):
    """A seam's rolls: a few metres up and down over 100 to 200 m."""
    return 1.5 * math.sin(x / 170.0 + phase) + 1.2 * math.cos(y / 130.0 + x / 450.0 + phase) + 0.6 * math.sin((x - y) / 95.0 + 2.0 * phase)


def topography(x, y):
    creek_at = 1900.0 + 120.0 * math.sin(y / 400.0)
    creek = 14.0 * math.exp(-(((x - creek_at) / 110.0) ** 2))
    return 135.0 + 4.0 * math.sin(x / 700.0) + 3.0 * math.cos(y / 500.0) + 0.8 * math.sin(x / 60.0 + y / 85.0) - creek


def horizons(x, y):
    """Every grid's height at a place, bottom to top. Seams are cut off at the base of weathering, so where one
    reaches it its roof and floor lie on it."""
    topo = topography(x, y)
    bow = topo - (18.0 + 4.0 * math.sin(x / 450.0 + y / 350.0))
    l_floor = 30.0 - DIP * y + 0.003 * x + rolls(x, y, 0.0)
    l_roof = l_floor + 5.5 + 0.8 * math.sin(x / 600.0) + 0.4 * math.cos(y / 350.0)
    m_floor = l_roof + 25.0 + 5.0 * math.sin(x / 520.0 + y / 700.0) + 2.5 * math.cos(y / 260.0) + 0.8 * math.sin(x / 110.0 + 1.3)
    washout = ((x - 2600.0) / 380.0) ** 2 + ((y - 350.0) / 260.0) ** 2
    m_roof = m_floor + 3.2 * smoothstep(1.0, 1.8, washout)
    u_floor = m_roof + 50.0 + 4.0 * math.sin(x / 380.0) + 2.0 * math.cos(y / 300.0)
    u_roof = u_floor + 1.6 + 0.5 * math.sin(y / 300.0 + x / 900.0)
    seams = [min(z, bow) for z in (l_floor, l_roof, m_floor, m_roof, u_floor, u_roof)]
    return dict(zip(GRIDS, seams + [bow, topo]))


GRIDS = ["L floor", "L roof", "M floor", "M roof", "U floor", "U roof", "BOW", "Topo"]


def write_grids(folder):
    """One XYZ file per grid, node by node, and the paths by grid name."""
    folder.mkdir(parents=True, exist_ok=True)
    cols, rows = round((EAST - WEST) / CELL) + 1, round((NORTH - SOUTH) / CELL) + 1
    lines = {name: [] for name in GRIDS}
    for row in range(rows):
        y = SOUTH + row * CELL
        for col in range(cols):
            x = WEST + col * CELL
            for name, z in horizons(x, y).items():
                lines[name].append(f"{E + x:.1f} {N + y:.1f} {z:.4f}")
    paths = {}
    for name in GRIDS:
        path = folder / (name.lower().replace(" ", "_") + ".xyz")
        path.write_text("\n".join(lines[name]) + "\n")
        paths[name] = path
    print(f"Wrote {len(GRIDS)} grids of {cols} x {rows} nodes at {CELL:g} m to {folder}")
    return paths


def import_grids(s, paths):
    return {name: made(s.run("surface.import", path=str(path), layer="grids", name=name))[0] for name, path in paths.items()}


def wall(to, batter, ramps, turns=()):
    return {"step": "project", "to": to, "batter": batter, "ramps": ramps, **({"turns": list(turns)} if turns else {})}


def berm(width, least=None):
    return {"step": "berm", "width": width, **({"min_bench": least} if least is not None else {})}


def surface(grid):
    return {"kind": "surface", "id": grid}


def rl(value):
    return {"kind": "rl", "rl": value}


def grid_rows(g, ramps, turns=()):
    """Walls to the grids: through L and its interburden to the M floor, through M to the U floor, through U to the
    base of weathering, then the weathering to the topography. A row on to a grid at the same batter with no berm
    carries the wall straight on, and marks where it crosses. Ramps in `turns` turn back on every berm."""
    rock = 65.0
    return [
        wall(surface(g["L roof"]), rock, ramps),
        wall(surface(g["M floor"]), rock, ramps),
        berm(30.0, 3.0),
        wall(surface(g["M roof"]), rock, ramps, turns),
        wall(surface(g["U floor"]), rock, ramps),
        berm(30.0, 3.0),
        wall(surface(g["U roof"]), rock, ramps, turns),
        wall(surface(g["BOW"]), rock, ramps),
        berm(30.0, 3.0),
        wall(surface(g["Topo"]), 45.0, ramps, turns),
    ]


def rl_rows(ramps, turns=()):
    """Walls to RL 50, 80, 110 and 140, with 30 m berms. Ramps in `turns` turn back on every berm."""
    rows = []
    for k, value in enumerate([50.0, 80.0, 110.0, 140.0]):
        if k > 0:
            rows.append(berm(30.0, 3.0))
        rows.append(wall(rl(value), 45.0 if value == 140.0 else 65.0, ramps, turns if k > 0 else ()))
    return rows


def designs(s, g, toes_layer, ramp_a, ramp_b, ramp_c):
    two = ["A", "B"]
    ramps = [
        {"name": "A", "string": ramp_a, "fixed": "start", "heading": "along", **ROAD},
        {"name": "B", "string": ramp_b, "fixed": "end", "heading": "along", **ROAD},
    ]
    turning = [{"name": "C", "string": ramp_c, "fixed": "start", "heading": "along", **ROAD}]
    every = []
    for k, name in enumerate(GRIDS[1:]):
        if k > 0:
            every.append(berm(10.0, 3.0))
        every.append(wall(surface(g[name]), 45.0 if name == "Topo" else 65.0, []))
    plans = [
        ("Grid walls", surface(g["L floor"]), grid_rows(g, []), []),
        ("Grid ramps", surface(g["L floor"]), grid_rows(g, two), ramps),
        ("Flat toe to grids", rl(-20.0), grid_rows(g, two), ramps),
        ("Grid toe to RLs", surface(g["L floor"]), rl_rows(two), ramps),
        ("Every horizon", surface(g["L floor"]), every, []),
        ("Grid switchbacks", surface(g["L floor"]), grid_rows(g, ["C"], ["C"]), turning),
        ("RL switchbacks", surface(g["L floor"]), rl_rows(["C"], ["C"]), turning),
    ]
    saved = {}
    for name, register, steps, its_ramps in plans:
        profile = {"name": name, "direction": "up", "register": register, "condition": 0.1, "steps": steps, "ramps": its_ramps}
        line = {"layer": toes_layer, "profile": name, "side": "left", "run_on": [100.0, 100.0]}
        content = {"layer": f"faces {name}", "lines": [line], "profiles": [profile]}
        saved[name] = made(s.run("face.save_design", name=name, content=content))[0]
    return saved


def boundary_loops(mesh):
    """How many loops of open edges a mesh has: one for a single sheet with no holes in it."""
    uses = {}
    for a, b, c in mesh["triangles"]:
        for edge in ((a, b), (b, c), (c, a)):
            key = (min(edge), max(edge))
            uses[key] = uses.get(key, 0) + 1
    links = {}
    for u, w in (edge for edge, n in uses.items() if n == 1):
        links.setdefault(u, []).append(w)
        links.setdefault(w, []).append(u)
    seen, loops = set(), 0
    for start in links:
        if start in seen:
            continue
        loops += 1
        stack = [start]
        while stack:
            x = stack.pop()
            if x not in seen:
                seen.add(x)
                stack.extend(links[x])
    return loops


def local(text):
    """A message with grid coordinates as metres from the template's origin."""
    return re.sub(r"(\d{6}\.\d+), (\d{7}\.\d+)", lambda m: f"({float(m.group(1)) - E:.0f}, {float(m.group(2)) - N:.0f})", text)


def check(s, saved):
    """Runs each design and lists, strip by strip, what didn't build. Returns the count of faults."""
    faults = 0
    for name, design in saved.items():
        started = time.perf_counter()
        try:
            record = s.run("face.project", design=design)
        except zd.CommandError as refused:
            print(f"\n{name}: refused: {local(str(refused))}")
            faults += STRIPS
            continue
        ms = (time.perf_counter() - started) * 1000.0
        faces = {o["name"]: o["id"] for o in s.objects(layer=f"faces {name}") if o["kind"] == "surface"}
        crossing = sum(s.object(face)["crossings"]["pairs"] > 0 for face in faces.values())
        in_pieces = sorted(name for name, face in faces.items() if boundary_loops(s.mesh(face)) != 1)
        triangles = sum(s.object(face)["triangles"] for face in faces.values())
        lines = [local(m) for m in record["messages"]]
        not_built = [m for m in lines if "Nothing is built" in m]
        no_height = [m for m in lines if "no height" in m]
        roads = [m for m in lines if re.match(r"Ramp \w: Strip \d+: steps? ", m) and "stopped" not in m]
        stopped = [m for m in lines if m.startswith("Ramp ") and m not in roads]
        print(f"\n{name}: {ms:,.0f} ms, {len(faces)} of {STRIPS} faces, {triangles:,} triangles, {crossing} with triangles crossing, "
              f"{len(in_pieces)} not one sheet, {len(no_height)} with walls of no height, {len(roads)} roads to the top")
        for m in not_built + stopped + [f"{face} isn't one sheet" for face in in_pieces] + no_height:
            print("  " + m[:240])
        faults += len(not_built) + crossing + len(in_pieces) + len(stopped)
    return faults


def main():
    here = pathlib.Path(__file__).resolve().parent
    folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else here
    paths = write_grids(here / "grids")
    s = zd.Session()
    g = import_grids(s, paths)
    toes = strips(s, count=STRIPS, z=0.0)
    for toe in toes:
        s.run("string.drape", string=toe, surface=g["L floor"], crossings=False)
    north = 80.0 * STRIPS + 300.0
    ramp_a, ramp_b = ramp_string(s, "Ramp A", 400.0, north), ramp_string(s, "Ramp B", 3100.0, north)
    ramp_c = ramp_string(s, "Ramp C", 1000.0, north)
    toes_layer = next(layer["id"] for layer in s.layers() if layer["name"] == "toes")
    saved = designs(s, g, toes_layer, ramp_a, ramp_b, ramp_c)
    path = folder / "Grid template.zdp"
    s.save(path)
    print(f"Saved {path}: {len(GRIDS)} grids, {len(toes)} strips, 3 ramp strings, designs {', '.join(saved)}")
    faults = check(s, saved)
    print(f"\n{faults} faults")
    sys.exit(1 if faults else 0)


if __name__ == "__main__":
    main()
