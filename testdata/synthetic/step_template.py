"""A project to try step design on (decision 0045): a face built by hand from a toe, a step at a time, at made-up
coordinates, every step to an RL so each one shows as it goes in.

The toe is a bent string 1.5 km long, flat on RL 100. The design "Steps from Toe" is what the Face design panel's
Step design button makes from it: one line, one profile called Steps, the toe on its own RL. It was built the way
the panel builds it, one face.step a row:

    1. a wall to RL 115 at 70 degrees
    2. a 10 m berm (its line shows as "Toe Steps next toe")
    3. ramp A placed on that berm line, as Place ramp places it, and a wall to RL 130 at 70 degrees climbing it
    4. a 10 m berm, which the road crosses level after it
    5. a wall to RL 145 at 65 degrees carrying the road on

A second toe, "Your toe", 400 m south, has no design, to start one from scratch.

    .venv\\Scripts\\python testdata/synthetic/step_template.py [folder]

Writes "Step template.zdproj" beside this script, or in the folder given. Then it checks the steps: the face is
clean, the road climbs from RL 115 to 145, and running the design whole gives the same face, vertex for vertex.
"""

import math
import pathlib
import sys

import zd

E, N = 650_000.0, 7_450_000.0
TOE_RL = 100.0
# A toe running east, bent a few degrees twice. The walls build north, on its left.
TOE = [(0.0, 0.0), (600.0, 30.0), (1100.0, 10.0), (1500.0, 70.0)]
YOUR_TOE = [(0.0, -400.0), (600.0, -370.0), (1100.0, -390.0), (1500.0, -330.0)]
SIDE = "left"
# Where ramp A is placed on the first berm's line, east of the toe's start (m), and how far its string reaches
# either side of the line, as the panel's Place ramp does.
RAMP_AT = 150.0
RAMP_REACH = 5.0
ROAD = {"width": 30.0, "grade": 10.0, "berm": "after", "turn_join": 50.0}


def at(x, y, z=TOE_RL):
    return [E + x, N + y, z]


def made(record):
    return [o["id"] for o in record["outputs"]]


def wall(rl, batter, ramps=()):
    return {"step": "project", "to": {"kind": "rl", "rl": rl}, "batter": batter, "condition": 0.1, **({"ramps": list(ramps)} if ramps else {})}


def berm(width=10.0):
    return {"step": "berm", "on": None, "width": width, "register": True, "condition": 0.1, "min_bench": 1.0}


def content(toe, steps, ramps=()):
    """A step design as the panel's Step design button saves it, with the rows added so far."""
    profile = {"name": "Steps", "direction": "up", "register": {"kind": "rl", "rl": TOE_RL}, "condition": 0.1, "steps": steps}
    if ramps:
        profile["ramps"] = list(ramps)
    return {"layer": "faces", "lines": [{"string": toe, "profile": "Steps", "side": SIDE, "run_on": [0.0, 0.0]}], "profiles": [profile], "rules": []}


def place_ramp(s, name, line, x):
    """A ramp as Place ramp makes one: started where the line is clicked, east along it, its string a short one
    square across the line there."""
    points = s.object(line)["points"]
    k = next(i for i in range(1, len(points)) if points[i][0] - E >= x)
    a, b = points[k - 1], points[k]
    t = (E + x - a[0]) / (b[0] - a[0])
    p = [a[j] + t * (b[j] - a[j]) for j in range(3)]
    length = math.hypot(b[0] - a[0], b[1] - a[1])
    way = ((b[0] - a[0]) / length, (b[1] - a[1]) / length)
    string = made(s.run("string.create", name=f"Ramp {name}", layer="ramps", points=[[p[0] - way[1] * d, p[1] + way[0] * d, p[2]] for d in (-RAMP_REACH, RAMP_REACH)]))[0]
    return {"name": name, "string": string, "fixed": "start", "heading": "along", **ROAD}, p


def by_name(s, layer):
    return {o["name"]: o["id"] for o in s.objects(layer=layer)}


def step_design(s, toe):
    """Builds the design a row at a time, each a face.step, and says what each step took."""
    name = "Steps from Toe"
    design = made(s.run("face.save_design", name=name, content=content(toe, [])))[0]
    rows = [wall(115.0, 70.0), berm()]
    times = []
    for k in range(1, len(rows) + 1):
        times.append(s.run("face.step", design=design, name=name, content=content(toe, rows[:k]))["duration_ms"])
    # Ramp A goes on the berm's line, and the next wall climbs it, as the panel carries a placed ramp onto it.
    ramp, start = place_ramp(s, "A", by_name(s, "faces")["Toe Steps next toe"], RAMP_AT)
    for row in (wall(130.0, 70.0, ["A"]), berm(), wall(145.0, 65.0, ["A"])):
        rows.append(row)
        record = s.run("face.step", design=design, name=name, content=content(toe, rows, [ramp]))
        times.append(record["duration_ms"])
    print(f"Steps: {', '.join(f'{t:.0f}' for t in times)} ms")
    for message in record["messages"]:
        print("  " + message)
    return design, rows, ramp, start


def check(s, toe, design, rows, ramp, start):
    faults = 0
    outputs = by_name(s, "faces")
    face = outputs.get("Toe Steps face")
    crossings = s.object(face)["crossings"]["pairs"] if face else None
    print(f"Face: {'missing' if face is None else 'clean' if crossings == 0 else f'{crossings} crossing pairs'}")
    faults += face is None or crossings != 0
    edge = outputs.get("Toe Steps road A pit edge")
    if edge is None:
        print("Road A: missing")
        return faults + 1
    points = s.object(edge)["points"]
    gap = math.hypot(points[0][0] - start[0], points[0][1] - start[1])
    print(f"Road A: from RL {points[0][2]:.3f} to {points[-1][2]:.3f}, starting {gap:.3f} m from where it was placed")
    faults += abs(points[0][2] - 115.0) > 1e-6 or abs(points[-1][2] - 145.0) > 1e-6
    # The same design run whole, on its own layer, gives the same face.
    whole = made(s.run("face.save_design", name="Whole", content={**content(toe, rows, [ramp]), "layer": "whole"}))[0]
    s.run("face.project", design=whole)
    other = by_name(s, "whole").get("Toe Steps face")
    stepped, once = s.mesh(face), s.mesh(other)
    same = stepped["vertices"] == once["vertices"] and stepped["triangles"] == once["triangles"]
    print(f"Run whole, the same face: {same} ({len(stepped['triangles']):,} triangles)")
    faults += not same
    return faults


def main():
    folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parent
    s = zd.Session()
    toe = made(s.run("string.create", name="Toe", layer="toes", points=[at(x, y) for x, y in TOE]))[0]
    s.run("string.create", name="Your toe", layer="toes", points=[at(x, y) for x, y in YOUR_TOE])
    design, rows, ramp, start = step_design(s, toe)
    path = folder / "Step template.zdproj"
    s.save(path)
    print(f"Saved {path}: the toe, your toe, ramp A's string, and the design Steps from Toe with {len(rows)} rows run")
    # The check runs the design whole too, after the save, so that stays out of the project.
    faults = check(s, toe, design, rows, ramp, start)
    sys.exit(1 if faults else 0)


if __name__ == "__main__":
    main()
