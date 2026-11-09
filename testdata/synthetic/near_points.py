"""Points a hair apart in a face: pairs of vertices further apart than the 1 um weld and closer than 1 cm, which leave
sliver triangles (decision 0034, 2026-11-09). Counts them for every face a run makes.

    .venv\\Scripts\\python testdata\\synthetic\\near_points.py grid "Grid walls"     # a design of the grid test pit
    .venv\\Scripts\\python testdata\\synthetic\\near_points.py made-up              # bench/made_up_face.py's face on RLs
"""

import math
import os
import pathlib
import sys

import zd

E, N = 650_000.0, 7_450_000.0


def near_pairs(points, low=1e-6, high=0.01):
    cells = {}
    for i, p in enumerate(points):
        cells.setdefault((math.floor(p[0] / high), math.floor(p[1] / high)), []).append(i)
    found = []
    for (cx, cy), ids in cells.items():
        around = [j for dx in (-1, 0, 1) for dy in (-1, 0, 1) for j in cells.get((cx + dx, cy + dy), [])]
        for i in ids:
            for j in around:
                if j > i and low < math.dist(points[i], points[j]) < high:
                    found.append((math.dist(points[i], points[j]), i, j))
    return sorted(found)


def report(s, faces, show=4):
    total = 0
    for face in faces:
        points = s.mesh(face["id"])["vertices"]
        pairs = near_pairs(points)
        total += len(pairs)
        if pairs:
            print(f"{face['name']}: {len(pairs)} pairs")
            for d, i, j in pairs[:show]:
                p, q = points[i], points[j]
                print(f"   {d * 1e6:9.1f} um at ({p[0] - E:.3f}, {p[1] - N:.3f}, {p[2]:.4f}), apart by ({(q[0] - p[0]) * 1e6:.1f}, {(q[1] - p[1]) * 1e6:.1f}, {(q[2] - p[2]) * 1e6:.1f}) um")
    print(f"{total} pairs")
    return total


def main():
    which = sys.argv[1] if len(sys.argv) > 1 else "made-up"
    if which == "grid":
        folder = pathlib.Path(os.environ.get("ZD_GRID_PIT", pathlib.Path(__file__).resolve().parent))
        s = zd.open(str(folder / "Grid template.zdproj"))
        name = sys.argv[2] if len(sys.argv) > 2 else "Grid walls"
        design = next(o["id"] for o in s.objects() if o["kind"] == "face_design" and o["name"] == name)
        s.run("face.project", design=design)
        report(s, [o for o in s.objects(layer=f"faces {name}") if o["kind"] == "surface"])
        return
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[2] / "bench"))
    import made_up_face as m

    s = zd.Session()
    toe = s.run("string.create", name="Made-up toe", layer="toe", points=m.TOE)["outputs"][0]["id"]
    content = {
        "layer": "made-up",
        "lines": [{"string": toe, "profile": "Highwall", "side": "left", "run_on": [0.0, 0.0]}],
        "profiles": [{"name": "Highwall", "direction": "up", "register": {"kind": "rl", "rl": m.TOE_RL}, "condition": 0.1, "steps": m.steps(False), "ramps": []}],
        "rules": [],
    }
    design = s.run("face.save_design", name="made-up", content=content)["outputs"][0]["id"]
    record = s.run("face.project", design=design)
    report(s, [o for o in s.objects(layer="made-up") if o["kind"] == "surface"])


if __name__ == "__main__":
    main()
