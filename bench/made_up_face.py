"""A made-up highwall on RLs, exported to DXF for Tim to read into Deswik and build again there (Phase 3 gate: "exports
to Deswik by DXF", and a face to set beside Deswik's). Two designs from one toe: plain, and with one road.

    .venv\\Scripts\\python bench\\made_up_face.py [out folder]

Writes `made-up-face.dxf`, `made-up-face-road.dxf`, `made-up-toe.dxf` and `made-up-face.md` (the profile, to build the
same in Deswik) to the folder, `bench/out/deswik-check` by default.
"""

import json
import pathlib
import sys

import zd

E, N = 650_000.0, 7_450_000.0
TOE_RL = 100.0
# A toe bent three times, running east; the wall rises on its left, to the north.
TOE = [[E, N, TOE_RL], [E + 400.0, N + 50.0, TOE_RL], [E + 800.0, N + 20.0, TOE_RL], [E + 1200.0, N + 120.0, TOE_RL]]
# Three 15 m benches at 65 degrees with 10 m berms, then 45 degrees to RL 160 (the made-up topsoil and weathering).
BENCHES = [(115.0, 65.0), (130.0, 65.0), (145.0, 65.0), (160.0, 45.0)]
BERM = 10.0
ROAD = {"width": 50.0, "grade": 10.0}


def steps(ramp):
    rows = []
    for k, (rl, batter) in enumerate(BENCHES):
        if k > 0:
            rows.append({"step": "berm", "width": BERM, "register": False})
        row = {"step": "project", "to": {"kind": "rl", "rl": rl}, "batter": batter}
        if ramp:
            row["ramps"] = ["A"]
        rows.append(row)
    return rows


def main():
    out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else pathlib.Path(__file__).parent / "out" / "deswik-check")
    out.mkdir(parents=True, exist_ok=True)
    s = zd.Session()
    toe = s.run("string.create", name="Made-up toe", layer="made-up toe", points=TOE)["outputs"][0]["id"]
    # Across the toe 150 m from its west end: the road starts where this crosses the toe and climbs east along it.
    across = s.run("string.create", name="Ramp A", layer="made-up ramp", points=[[E + 150.0, N - 30.0, TOE_RL], [E + 150.0, N + 60.0, TOE_RL]])["outputs"][0]["id"]

    written = []
    for name, ramp in [("made-up-face", False), ("made-up-face-road", True)]:
        ramps = [{"name": "A", "string": across, "fixed": "start", "heading": "along", **ROAD}] if ramp else []
        content = {
            "layer": name,
            "lines": [{"string": toe, "profile": "Highwall", "side": "left", "run_on": [0.0, 0.0]}],
            "profiles": [{"name": "Highwall", "direction": "up", "register": {"kind": "rl", "rl": TOE_RL}, "condition": 0.1, "steps": steps(ramp), "ramps": ramps}],
            "rules": [],
        }
        design = s.run("face.save_design", name=name, content=content)["outputs"][0]["id"]
        record = s.run("face.project", design=design)
        made = [o["id"] for o in record["outputs"] if o["change"] == "created"]
        print(f"{name}: {len(made)} objects")
        for message in record["messages"]:
            print(f"  {message}")
        path = out / f"{name}.dxf"
        s.run("export.dxf", objects=made, path=str(path), overwrite=True)
        written.append(path)
    toe_path = out / "made-up-toe.dxf"
    s.run("export.dxf", objects=[toe, across], path=str(toe_path), overwrite=True)
    written.append(toe_path)

    rows = "\n".join(f"| {k + 1} | {TOE_RL if k == 0 else BENCHES[k - 1][0]:.0f} to {rl:.0f} | {batter:.0f}° | {'' if k == len(BENCHES) - 1 else f'{BERM:.0f} m'} |" for k, (rl, batter) in enumerate(BENCHES))
    toe_text = "\n".join(f"| {k + 1} | {p[0]:,.1f} | {p[1]:,.1f} | {p[2]:.1f} |" for k, p in enumerate(TOE))
    (out / "made-up-face.md").write_text(
        f"""# A made-up highwall, to build again in Deswik

Made by `bench/made_up_face.py` from main. Every RL is a flat target, so Deswik can build the same face from the toe
alone. Coordinates are made up, in the MGA range.

## Toe (`made-up-toe.dxf`, layer `made-up toe`), RL {TOE_RL:.0f}, running east

| Vertex | Easting | Northing | RL |
|---:|---:|---:|---:|
{toe_text}

The wall rises on the toe's left, to the north. No run-on at the ends.

## Profile, up from the toe

| Wall | RL | Batter | Berm after it |
|---:|---|---:|---|
{rows}

Batters are measured square to the toe in plan; mitred corners; berm lines conditioned to 0.1 m.

## Files

- `made-up-face.dxf`: the face as one surface (3DFACE), with its toe and crest strings, on layer `made-up-face`.
- `made-up-face-road.dxf`: the same with ramp A: {ROAD['width']:.0f} m wide at {ROAD['grade']:.0f} % on the inside edge (the pit
  side), starting where the string `Ramp A` (in `made-up-toe.dxf`) crosses the toe, 150 m from its west end, and climbing
  east along the wall; past each berm it crosses level and climbs on from the next toe (decision 0035).
- `made-up-toe.dxf`: the toe and the ramp's string, to start from in Deswik.

## What to check in Deswik

1. Each DXF reads in at the same coordinates (DXF R12).
2. Build the same face from the toe with the profile above; set it beside ours: crests within 0.5 m, and the volume
   between the faces.
3. The road: same width, grade on the inside edge, and where it reaches each crest.
""",
        encoding="utf-8",
    )
    written.append(out / "made-up-face.md")
    print(json.dumps([str(p) for p in written], indent=1))


if __name__ == "__main__":
    main()
