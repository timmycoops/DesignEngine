"""A made-up mine at full scale, to find where the app runs out of speed: 10 km along strike and 3 km down dip, 20 coal
seams, 500 m deep at the final highwall, and a lidar topography (1 m unless set) over it all. Made-up coordinates.

The geology (25 m Vulcan grids, a seam model on them):

- 20 seams, S01 at the top to S20 at the base, dipping north at about 9 degrees, with rolls, so S20's floor is some 20 m
  down at the first strip and 500 m down at the last. The stack is about 240 m thick: seams 0.8 to 5 m, partings 4 to
  20 m. The upper seams subcrop part way down dip, so the first strips mine only the lower ones.
- Every seam cut off by the base of weathering, 15 to 25 m under the natural ground.
- Made complicated on purpose: S05 washed out over an oval 1.2 by 0.6 km; S12 pinching out to the east past 7 km; the
  parting between S08 and S09 thinning to nothing over an oval, so the two coalesce.
- Made-up ash, sulphur and air-dried RD for each seam.

The lidar topography (a Vulcan .tpg grid, read as a surface): rolling natural ground, a branching net of gullies, a
creek meandering along strike incised 10 to 12 m with steep banks, an old spoil dump 35 m high at 37 degrees with
benched faces, a haul road on a 5 m embankment crossing the pit, a dam, and lidar's own roughness, a few centimetres to
30 cm, everywhere: no two triangles are coplanar.

The design, each step a command the app has: a base toe along strike offset north 60 m a strip (50 strips), numbered
and draped on S20's floor; one highwall design on them all (65 degrees to the base of weathering, a 30 m berm, 45 degrees
to the topography, carried on up and down so each face passes right through); the pit from the topography to S20's floor
inside the limit, sliced into strips, cut by the seam model into 41 horizons, cut into 100 m blocks by polygons, measured
and interrogated.

    .venv\\Scripts\\python testdata/synthetic/large_mine.py [--lidar 1] [--strike 10000] [--dip 3000] [--block 100]
        [--until STEP] [folder]

--until stops after a step (grids, lidar, faces, strips, seams, blocks), saving what's made. Writes the grids, the
lidar and "Large mine <lidar>m.zdp" to the folder (testdata/synthetic/reserves/large mine unless given). Prints each step's
count, time and the process's peak memory.
"""

import argparse
import math
import multiprocessing
import os
import pathlib
import struct
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from ramp_template import E, N, made  # noqa: E402

PREFIX = "lm"
SEAMS = [f"S{k:02}" for k in range(1, 21)]
CELL = 25.0
STRIP_WIDTH = 60.0
BERM, MIN_BENCH = 30.0, 3.0
DIP = 0.16  # about 9 degrees
SHOWN = {"0 pit limit", "1 toes", "9 blocks", "topography"}

# Each seam's mean thickness (m) and the parting under it down to the next seam (m), S01 to S20.
THICK = [1.6, 2.4, 0.8, 3.1, 2.0, 1.2, 4.2, 2.6, 1.9, 0.9, 3.4, 1.5, 2.2, 5.0, 2.8, 1.1, 3.6, 2.0, 1.4, 4.5]
PARTING = [12.0, 8.0, 15.0, 6.0, 18.0, 10.0, 9.0, 5.0, 14.0, 20.0, 7.0, 11.0, 16.0, 4.0, 13.0, 9.0, 12.0, 6.0, 17.0]


def at(x, y, z=0.0):
    return [E + x, N + y, z]


def smoothstep(edge0, edge1, value):
    t = min(1.0, max(0.0, (value - edge0) / (edge1 - edge0)))
    return t * t * (3.0 - 2.0 * t)


def ground(x, y):
    """The natural ground, smooth: rolling hills, falling gently north."""
    return (140.0 - 0.004 * y + 6.0 * math.sin(x / 900.0) + 4.0 * math.cos(y / 650.0 + x / 2300.0)
            + 2.5 * math.sin(x / 310.0 + y / 420.0) + 1.2 * math.cos((x + 2.0 * y) / 170.0))


def creek(x, y):
    """A creek meandering along strike, incised 10 to 12 m, with steep banks."""
    centre = 1400.0 + 260.0 * math.sin(x / 1100.0) + 70.0 * math.sin(x / 240.0)
    d = abs(y - centre)
    depth = 10.0 + 2.0 * math.sin(x / 800.0)
    return depth * (1.0 - smoothstep(12.0, 45.0, d))


def gullies(x, y):
    """Branching gullies draining to the creek: sharp V's up to 3 m deep."""
    a = 1.0 - abs(math.sin(x / 140.0 + 0.9 * math.sin(y / 210.0)))
    b = 1.0 - abs(math.sin((x * 0.6 + y * 0.8) / 95.0 + 0.7 * math.cos(x / 330.0)))
    return 3.0 * a ** 8 + 1.8 * b ** 10


def bow(x, y):
    """The base of weathering: 15 to 25 m under the natural ground and the creek's bed."""
    return ground(x, y) - creek(x, y) - (20.0 + 5.0 * math.sin(x / 450.0 + y / 350.0))


def rolls(x, y, phase):
    return 2.0 * math.sin(x / 210.0 + phase) + 1.4 * math.cos(y / 160.0 + x / 520.0 + phase) + 0.7 * math.sin((x - y) / 110.0 + 2.0 * phase)


def seam_stack(x, y):
    """Each seam's (roof, floor), S01 to S20, built up from S20's floor, before the weathering cuts them off."""
    z = 120.0 - DIP * y + 0.002 * x + rolls(x, y, 0.0)
    out = [None] * 20
    for k in range(19, -1, -1):
        thick = THICK[k] * (1.0 + 0.25 * math.sin(x / (600.0 + 37.0 * k) + y / (500.0 + 23.0 * k) + k))
        if k == 4:  # S05 washed out over an oval
            thick *= smoothstep(1.0, 1.6, ((x - 4200.0) / 600.0) ** 2 + ((y - 2300.0) / 300.0) ** 2)
        if k == 11:  # S12 pinching out east
            thick *= 1.0 - smoothstep(7000.0, 8200.0, x)
        floor = z
        roof = floor + thick
        out[k] = (roof, floor)
        if k > 0:
            parting = PARTING[k - 1] * (1.0 + 0.3 * math.cos(x / (700.0 + 41.0 * k) - y / (450.0 + 19.0 * k)))
            if k == 8:  # S08 and S09 coalescing over an oval
                parting *= smoothstep(0.8, 1.5, ((x - 2600.0) / 900.0) ** 2 + ((y - 2500.0) / 400.0) ** 2)
            z = roof + max(0.0, parting)
    return out


def horizons(x, y):
    """Every seam grid's height at a place, cut off by the base of weathering, and the base of weathering."""
    b = bow(x, y)
    return [(min(r, b), min(f, b)) for r, f in seam_stack(x, y)], b


def qualities(k, x, y, length):
    base = 8.0 + (k * 7.3) % 20.0
    ash = base + 6.0 * x / length + 2.0 * math.sin(y / 260.0 + x / 410.0 + k)
    sulphur = 0.35 + 0.15 * math.cos(x / 520.0 + k) + 0.002 * ash
    return ash, sulphur, 1.30 + 0.009 * ash


def vulcan_grid(heights, cols, rows, west, south, cell):
    out = bytearray(b"MADE-UP LARGE MINE GRID         ")
    for v in (E + west, E + west + (cols - 1) * cell, N + south, N + south + (rows - 1) * cell):
        out += struct.pack(">d", v)
    out += struct.pack(">ii", cols * rows, 0) + struct.pack(">ff", cell, cell) + struct.pack(">ii", cols, rows)
    return out


def write_grids(folder, area, length):
    west, east, south, north = area
    cols, rows = round((east - west) / CELL) + 1, round((north - south) / CELL) + 1
    values = {}
    for r in range(rows):
        y = south + r * CELL
        for c in range(cols):
            x = west + c * CELL
            seams, b = horizons(x, y)
            for k, (roof, floor) in enumerate(seams):
                ash, sulphur, rd = qualities(k, x, y, length)
                stem = SEAMS[k].lower()
                for variable, z in (("sr", roof), ("sf", floor), ("as", ash), ("ts", sulphur), ("rd", rd)):
                    values.setdefault(f"{stem}.{variable}g", []).append(z)
            values.setdefault("bow.sfg", []).append(b)
    for name, heights in values.items():
        out = vulcan_grid(heights, cols, rows, west, south, CELL)
        out += b"".join(struct.pack(">fi", z, 1) for z in heights)
        (folder / f"{PREFIX}{name}").write_bytes(bytes(out))
    lines = ["horizon,parent,material,working,roof,floor,rd,as,ts"]
    for seam in SEAMS:
        stem = f"{PREFIX}{seam.lower()}"
        lines.append(f"{seam},,coal,yes,{stem}.srg,{stem}.sfg,{stem}.rdg,{stem}.asg,{stem}.tsg")
    definition = folder / "stack.csv"
    definition.write_text("\n".join(lines) + "\n")
    print(f"Wrote {len(values)} Vulcan grids of {cols} x {rows} nodes at {CELL:g} m and {definition.name}", flush=True)
    return definition


# The lidar ------------------------------------------------------------------------------------------------------------

SPOIL = (3000.0, 4600.0, 600.0, 1100.0)  # an old dump's footprint: west, east, south, north
SPOIL_HEIGHT = 35.0
TAN37 = math.tan(math.radians(37.0))


def spoil(x, y):
    """An old spoil dump: 37 degree faces benched every 12 m with 8 m berms, a flat top, rounded corners."""
    west, east, south, north = SPOIL
    dx = max(west - x, 0.0, x - east)
    dy = max(south - y, 0.0, y - north)
    inside = min(x - west, east - x, y - south, north - y)
    if dx > 0.0 or dy > 0.0 or inside < 0.0:
        return 0.0
    run = inside  # distance in from the toe
    height, z = 0.0, 0.0
    while height < SPOIL_HEIGHT:
        lift = min(12.0, SPOIL_HEIGHT - height)
        face = lift / TAN37
        if run <= face:
            return z + run * TAN37
        run -= face
        z += lift
        height += lift
        if run <= 8.0:
            return z
        run -= 8.0
    return z


def road(x, y):
    """A haul road on a 5 m embankment, 30 m wide on top, 1 in 2 batters, crossing the pit on a diagonal."""
    d = abs((y - 300.0) - 0.28 * (x - 5000.0)) / math.hypot(1.0, 0.28)
    if d <= 15.0:
        return 5.0
    return max(0.0, 5.0 - (d - 15.0) / 2.0)


def dam(x, y):
    d = math.hypot(x - 7800.0, (y - 900.0) * 1.4)
    return -6.0 * (1.0 - smoothstep(30.0, 80.0, d))


def roughness(i, j):
    """Lidar's own roughness: hashed noise, a few cm to 30 cm, deterministic by node."""
    h1 = math.sin(i * 12.9898 + j * 78.233) * 43758.5453
    h2 = math.sin((i // 3) * 39.3468 + (j // 3) * 11.135) * 24634.6345
    return 0.08 * (h1 - math.floor(h1) - 0.5) + 0.35 * (h2 - math.floor(h2) - 0.5)


def lidar_rows(job):
    """The lidar's heights for a band of rows, as Vulcan node records."""
    west, south, cell, cols, first, last = job
    out = bytearray()
    pack = struct.Struct(">fi").pack
    for r in range(first, last):
        y = south + r * cell
        for c in range(cols):
            x = west + c * cell
            z = ground(x, y) - creek(x, y) - gullies(x, y) + spoil(x, y) + road(x, y) + dam(x, y) + roughness(c, r)
            out += pack(z, 1)
    return bytes(out)


def write_lidar(folder, area, cell):
    west, east, south, north = area
    cols, rows = round((east - west) / cell) + 1, round((north - south) / cell) + 1
    path = folder / "lidar.tpg"
    band = max(1, rows // 400)
    jobs = [(west, south, cell, cols, r, min(rows, r + band)) for r in range(0, rows, band)]
    with path.open("wb") as out, multiprocessing.Pool(max(1, (os.cpu_count() or 2) - 4)) as pool:
        out.write(vulcan_grid((), cols, rows, west, south, cell))
        for part in pool.imap(lidar_rows, jobs):
            out.write(part)
    print(f"Wrote the lidar, {cols:,} x {rows:,} nodes at {cell:g} m ({cols * rows:,} nodes, {path.stat().st_size / 1e6:,.0f} MB)", flush=True)
    return path


# The run --------------------------------------------------------------------------------------------------------------


def peak_gb():
    try:
        import ctypes
        from ctypes import wintypes

        class Counters(ctypes.Structure):
            _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD), ("PeakWorkingSetSize", ctypes.c_size_t),
                        ("WorkingSetSize", ctypes.c_size_t), ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                        ("QuotaPagedPoolUsage", ctypes.c_size_t), ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                        ("QuotaNonPagedPoolUsage", ctypes.c_size_t), ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t)]

        counters = Counters()
        counters.cb = ctypes.sizeof(Counters)
        current = ctypes.windll.kernel32.GetCurrentProcess
        current.restype = wintypes.HANDLE
        info = ctypes.windll.psapi.GetProcessMemoryInfo
        info.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
        info(current(), ctypes.byref(counters), counters.cb)
        return counters.PeakWorkingSetSize / 1e9
    except Exception:
        return float("nan")


class Clock:
    def __init__(self):
        self.started = self.last = time.perf_counter()

    def __call__(self, label):
        now = time.perf_counter()
        print(f"   {label}: {now - self.last:,.1f} s (peak memory {peak_gb():,.1f} GB)", flush=True)
        self.last = now


def main():
    import zd

    parser = argparse.ArgumentParser()
    parser.add_argument("--lidar", type=float, default=1.0)
    parser.add_argument("--strike", type=float, default=10000.0)
    parser.add_argument("--dip", type=float, default=3000.0)
    parser.add_argument("--block", type=float, default=100.0)
    parser.add_argument("--until", choices=["grids", "lidar", "faces", "strips", "seams", "blocks"], default="blocks")
    parser.add_argument("folder", nargs="?", default=str(HERE / "reserves" / "large mine"))
    a = parser.parse_args()
    folder = pathlib.Path(a.folder)
    folder.mkdir(parents=True, exist_ok=True)
    strips_count = round(a.dip / STRIP_WIDTH)
    limit = (50.0, a.strike - 50.0, -60.0, strips_count * STRIP_WIDTH - 40.0)
    area = (-300.0, a.strike + 300.0, -300.0, a.dip + 700.0)
    clock = Clock()
    s = zd.Session()
    path = folder / f"Large mine {a.lidar:g}m.zdp"

    def finish(stage):
        if a.until != stage:
            return False
        for layer in s.layers():
            s.set_layer_visible(layer["name"], layer["name"] in SHOWN)
        s.save(path)
        clock(f"saved {path} ({path.stat().st_size / 1e9:,.2f} GB)")
        print(f"Stopped after '{stage}' in {time.perf_counter() - clock.started:,.0f} s", flush=True)
        return True

    definition = write_grids(folder, area, a.strike)
    clock("grids written")
    g = {}
    for seam in SEAMS + ["bow"]:
        for id in made(s.run("grid.import_vulcan_seam", dir=str(folder), prefix=PREFIX, seam=seam.lower(), layer="grids")):
            g[s.object(id)["name"]] = id
    qualities_ = [id for name, id in g.items() if name.split()[-1] in ("as", "rd", "ts")]
    s.run("object.delete_many", objects=qualities_)
    g = {name: id for name, id in g.items() if id not in qualities_}
    model = made(s.run("seam.build_model", definition=str(definition), layer="geology", name="Seam model"))[0]
    clock(f"{len(g)} grids and the seam model imported")
    if finish("grids"):
        return

    lidar = write_lidar(folder, area, a.lidar)
    clock("lidar written")
    topo = made(s.run("surface.import", path=str(lidar), layer="topography", name="Lidar"))[0]
    clock(f"lidar imported, {s.object(topo)['triangles']:,} triangles")
    if finish("lidar"):
        return

    floor = g["S20 sf"]
    base = [(0.0, 0.0), (a.strike * 0.3, 35.0), (a.strike * 0.7, -20.0), (a.strike, 10.0)]
    toe = made(s.run("string.create", name="Base toe", layer="1 toes", points=[at(x, y) for x, y in base]))[0]
    pit_limit = made(s.run("string.create", name="Pit limit", layer="0 pit limit", closed=True,
                           points=[at(limit[0], limit[2]), at(limit[1], limit[2]), at(limit[1], limit[3]), at(limit[0], limit[3])]))[0]
    toes = [toe, *made(s.run("string.offset", string=toe, distance=STRIP_WIDTH, side="left", times=strips_count - 1, name="Toe", layer="1 toes"))]
    s.run("string.number_across", {"strings": toes, "start": 1, "from": at(a.strike / 2, -150.0), "to": at(a.strike / 2, limit[3] + 200.0)})
    for t in toes:
        s.run("string.drape", string=t, surface=floor, crossings=False)
    toes_layer = next(layer["id"] for layer in s.layers() if layer["name"] == "1 toes")
    steps = [
        {"step": "project", "to": {"kind": "surface", "id": g["BOW sf"]}, "batter": 65.0, "ramps": []},
        {"step": "berm", "width": BERM, "min_bench": MIN_BENCH},
        {"step": "project", "to": {"kind": "surface", "id": topo}, "batter": 45.0, "ramps": []},
        {"step": "project", "to": {"kind": "rl", "rl": 500.0}, "batter": 45.0, "ramps": []},
    ]
    register = {"kind": "surface", "id": floor}
    up = {"name": "Highwall", "direction": "up", "register": register, "condition": 0.1, "steps": steps, "ramps": []}
    down = {"name": "Highwall down", "direction": "down", "register": register, "condition": 0.1, "ramps": [],
            "steps": [{"step": "project", "to": {"kind": "rl", "rl": -900.0}, "batter": 65.0, "ramps": []}]}
    lines = [{"layer": toes_layer, "profile": name, "side": "left", "run_on": [100.0, 100.0]} for name in ("Highwall", "Highwall down")]
    design = made(s.run("face.save_design", name="Highwall", content={"layer": "2 faces", "lines": lines, "profiles": [up, down]}))[0]
    s.run("face.project", design=design)
    faces = [o["id"] for o in s.objects(layer="2 faces", kind="surface")]
    clock(f"{len(toes)} toes, {len(faces)} faces")
    if finish("faces"):
        return

    pit = made(s.run("solid.between_surfaces", top=topo, bottom=floor, string=pit_limit, layer="3 pit", name="Pit"))[0]
    clock(f"the pit, {s.object(pit)['volume']:,.0f} m3, {s.object(pit)['triangles']:,} triangles")
    strips = made(s.run("solid.slice_by_faces", solid=pit, faces=faces, layer="4 strips"))
    s.run("object.delete", object=strips[-1])
    strips = strips[:-1]
    clock(f"{len(strips)} strips")
    if finish("strips"):
        return

    pieces = made(s.run("solid.cut_by_seams", solids=strips, model=model, layer="5 seam pieces"))
    clock(f"{len(pieces):,} seam pieces")
    if finish("seams"):
        return

    first = made(s.run("string.create", name="Block line 0", layer="7 block lines",
                       points=[at(limit[0], limit[2] - 200.0, -900.0), at(limit[0], limit[3] + 200.0, -900.0)]))[0]
    for name, y in (("South closing line", limit[2] - 100.0), ("North closing line", limit[3] + 100.0)):
        s.run("string.create", name=name, layer="7 block lines", points=[at(limit[0] - 100.0, y, -900.0), at(limit[1] + 100.0, y, -900.0)])
    count = round((limit[1] - limit[0]) / a.block)
    s.run("string.offset", string=first, distance=a.block, side="right", times=count, name="Block line", layer="7 block lines")
    block_lines = [o["id"] for o in s.objects(layer="7 block lines", kind="string")]
    polygons = made(s.run("string.polygons_from_lines", strings=block_lines, layer="8 block polygons", name="B#"))
    blocks = made(s.run("solid.cut_by_polygons", solids=pieces, polygons=polygons, layer="9 blocks"))
    clock(f"{len(polygons)} block polygons, {len(blocks):,} blocks")
    s.run("solid.measure", solids=blocks)
    coal = [b for b in blocks if s.object(b)["attributes"].get("material") == "coal"]
    s.run("solid.interrogate", solids=coal, model=model)
    clock(f"{len(blocks):,} blocks measured, {len(coal):,} coal blocks interrogated")
    finish("blocks")


if __name__ == "__main__":
    # On a 128 MB stack, the most Python allows here: Python's main thread has 1 MB, and the engine's work on meshes this
    # size isn't checked on so little.
    import threading

    threading.stack_size(1 << 27)
    worker = threading.Thread(target=main)
    worker.start()
    worker.join()
