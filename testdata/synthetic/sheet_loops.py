"""Where a face of the grid test pit isn't one sheet: each loop of open edges with its size and place, for one design.

    .venv\\Scripts\\python testdata\\synthetic\\sheet_loops.py "Grid walls" [strip ...]

Opens `Grid template.zdp` (run `grid_template.py` first; pass its folder with ZD_GRID_PIT), runs the design and lists
the loops of every face that has more than one, in metres from the template's origin, biggest first. A face that's one
sheet with no holes has one loop, its edge.
"""

import os
import pathlib
import sys

import zd

E, N = 650_000.0, 7_450_000.0


def loops_of(mesh):
    """Each loop of open edges as its vertex indices, in order."""
    uses = {}
    for a, b, c in mesh["triangles"]:
        for u, w in ((a, b), (b, c), (c, a)):
            key = (min(u, w), max(u, w))
            uses[key] = uses.get(key, 0) + 1
    links = {}
    for u, w in (edge for edge, n in uses.items() if n == 1):
        links.setdefault(u, []).append(w)
        links.setdefault(w, []).append(u)
    seen, loops = set(), []
    for start in links:
        if start in seen:
            continue
        loop, stack = [], [start]
        while stack:
            x = stack.pop()
            if x not in seen:
                seen.add(x)
                loop.append(x)
                stack.extend(links[x])
        loops.append(loop)
    return loops, sum(n > 2 for n in (len(v) for v in links.values()))


def main():
    design_name = sys.argv[1] if len(sys.argv) > 1 else "Grid walls"
    strips = set(sys.argv[2:])
    folder = pathlib.Path(os.environ.get("ZD_GRID_PIT", pathlib.Path(__file__).resolve().parent))
    s = zd.open(str(folder / "Grid template.zdp"))
    design = next(o["id"] for o in s.objects() if o["kind"] == "face_design" and o["name"] == design_name)
    record = s.run("face.project", design=design)
    for o in s.objects(layer=f"faces {design_name}"):
        if o["kind"] != "surface" or (strips and not any(o["name"].startswith(f"Strip {k} ") for k in strips)):
            continue
        mesh = s.mesh(o["id"])
        loops, pinched = loops_of(mesh)
        if len(loops) == 1 and not pinched:
            continue
        points = mesh["vertices"]
        rows = []
        for loop in loops:
            xs = [points[i][0] - E for i in loop]
            ys = [points[i][1] - N for i in loop]
            zs = [points[i][2] for i in loop]
            span = max(max(xs) - min(xs), max(ys) - min(ys))
            rows.append((span, len(loop), min(xs), max(xs), min(ys), max(ys), min(zs), max(zs)))
        rows.sort(reverse=True)
        print(f"{o['name']}: {len(loops)} loops, {pinched} pinched vertices")
        for span, n, x0, x1, y0, y1, z0, z1 in rows:
            print(f"  {n:5d} edges, {span:8.1f} m across: x {x0:8.1f} to {x1:8.1f}, y {y0:7.1f} to {y1:7.1f}, RL {z0:6.1f} to {z1:6.1f}")
    for m in record["messages"]:
        if any(m.startswith(f"Strip {k} ") for k in strips) or not strips:
            pass


if __name__ == "__main__":
    main()
