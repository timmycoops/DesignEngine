"""A project to try ramps on a toe that lies flat on an RL (decision 0033), at made-up coordinates.

Five strips offset 80 m from one bent base toe and numbered by a line across, two ramp strings across them,
and a face design that runs every strip on the toes layer: the toe flat on RL -100, three walls to RL -60,
-20 and 20 with 20 m berms between, and two 30 m roads at 10 % up all three walls, each crossing the berms
level after them (decision 0035). Ramp A starts where its string crosses each toe; ramp B comes out on the top
crest where its string crosses.

    .venv\\Scripts\\python testdata/synthetic/ramp_template.py [folder]

Writes "Ramp template.zdp" beside this script, or in the folder given. Then it runs the design once
without saving the run, and prints what each strip made, so a template that doesn't build says so here.
"""

import math
import pathlib
import sys

import zd

E, N = 650_000.0, 7_450_000.0
TOE_RL = -100.0
WALLS = [(-60.0, 70.0), (-20.0, 70.0), (20.0, 65.0)]
BERM = 20.0
STRIP_WIDTH = 80.0
STRIPS = 5
ROAD = {"width": 30.0, "grade": 10.0}

# A base toe 3.5 km long running east, bent a few degrees twice, and the strips offset north from it.
BASE = [(0.0, 0.0), (1200.0, 60.0), (2400.0, 40.0), (3500.0, 150.0)]


def at(x, y, z=TOE_RL):
    return [E + x, N + y, z]


def made(record):
    return [o["id"] for o in record["outputs"]]


def strips(s, count=STRIPS, z=TOE_RL):
    base = made(s.run("string.create", name="Base toe", layer="toes", points=[at(x, y, z) for x, y in BASE]))[0]
    offsets = made(s.run("string.offset", string=base, distance=STRIP_WIDTH, side="left", times=count - 1, name="Toe"))
    # Numbered south to north by a line across them all.
    north = STRIP_WIDTH * count + 200.0
    s.run("string.number_across", {"strings": [base, *offsets], "from": at(1750.0, -100.0), "to": at(1750.0, north), "start": 1})
    return [base, *offsets]


def ramp_string(s, name, x, north=500.0):
    return made(s.run("string.create", name=name, layer="ramp strings", points=[at(x, -100.0), at(x, north)]))[0]


def design(s, toes_layer, ramp_a, ramp_b):
    steps = []
    for k, (rl, batter) in enumerate(WALLS):
        if k > 0:
            steps.append({"step": "berm", "width": BERM})
        steps.append({"step": "project", "to": {"kind": "rl", "rl": rl}, "batter": batter, "ramps": ["A", "B"]})
    profile = {
        "name": "Highwall",
        "direction": "up",
        "register": {"kind": "rl", "rl": TOE_RL},
        "steps": steps,
        "ramps": [
            {"name": "A", "string": ramp_a, "fixed": "start", "heading": "along", **ROAD},
            {"name": "B", "string": ramp_b, "fixed": "end", "heading": "along", **ROAD},
        ],
    }
    content = {"layer": "faces", "lines": [{"layer": toes_layer, "profile": "Highwall", "side": "left", "run_on": [100.0, 100.0]}], "profiles": [profile]}
    return made(s.run("face.save_design", name="Ramps on RLs", content=content))[0]


def climb_and_landings(points):
    """How far a road climbs in plan, and how far it runs level across berms."""
    steps = [(math.hypot(b[0] - a[0], b[1] - a[1]), b[2] - a[2]) for a, b in zip(points, points[1:])]
    return sum(d for d, dz in steps if abs(dz) > 1e-9), sum(d for d, dz in steps if abs(dz) <= 1e-9)


def check(s, design_id):
    """Runs the design and says what each strip made: its face, and each road's climb, landings and heights."""
    record = s.run("face.project", design=design_id)
    print(f"face.project: {record['duration_ms']:.0f} ms, {len(record['outputs'])} objects")
    for message in record["messages"]:
        print("  " + message)
    top = WALLS[-1][0]
    rise_over_grade = (top - TOE_RL) / (ROAD["grade"] / 100.0)
    faults = 0
    made_by = {o["name"]: o["id"] for o in s.objects(layer="faces")}
    for k in range(1, STRIPS + 1):
        face = made_by.get(f"Strip {k} Highwall face")
        crossings = s.object(face)["crossings"]["pairs"] if face else None
        line = f"Strip {k}: face {'missing' if face is None else 'clean' if crossings == 0 else f'{crossings} crossing pairs'}"
        faults += face is None or crossings != 0
        for ramp in "AB":
            edge = made_by.get(f"Strip {k} Highwall road {ramp} pit edge")
            if edge is None:
                line += f"; road {ramp} missing"
                faults += 1
                continue
            points = s.object(edge)["points"]
            (climb, landings), low, high = climb_and_landings(points), points[0][2], points[-1][2]
            line += f"; road {ramp} {climb:,.1f} m up and {landings:,.1f} m across berms, from RL {low:.1f} to {high:.1f}"
            faults += abs(high - top) > 1e-6 or abs(low - TOE_RL) > 1e-6
        print(line)
    print(f"Each road up RL {TOE_RL:g} to {top:g} at {ROAD['grade']:g} % is {rise_over_grade:,.0f} m on its walls.")
    return faults


def main():
    folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent
    s = zd.Session()
    toes = strips(s)
    ramp_a, ramp_b = ramp_string(s, "Ramp A", 400.0), ramp_string(s, "Ramp B", 3100.0)
    toes_layer = next(layer["id"] for layer in s.layers() if layer["name"] == "toes")
    design_id = design(s, toes_layer, ramp_a, ramp_b)
    path = folder / "Ramp template.zdp"
    s.save(path)
    print(f"Saved {path}: {len(toes)} strips, 2 ramp strings, the design Ramps on RLs")
    faults = check(s, design_id)
    sys.exit(1 if faults else 0)


if __name__ == "__main__":
    main()
