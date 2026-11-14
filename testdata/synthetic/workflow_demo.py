"""A workflow to learn from (decision 0065): the made-up pit taken from a toe to interrogated blocks, captured as a
workflow with a note on every step and a pause to look at the strips, saved as "Workflow demo.zdp".

What you draw by hand stays yours, and the workflow does the rest from it:

- the base toe (on '1 toes'), the pit limit (on '0 pit limit'), the ramp strings (on '0 ramp strings') and the
  first block line with the two closing lines (on '6 block lines') are drawn first and are not steps: edit any of them,
  run the workflow again, and everything after follows;
- the highwall is 65 degrees to the base of weathering, a 30 m berm there, then 45 degrees to the topography, with 50 m
  roads at 10 % starting 100 m inside the pit's west end: A straight up both walls of strips 1 to 5, and B on strip 6,
  the final highwall, turning back on the berm, a switchback;
- each step puts what it makes on a layer numbered in the order of the workflow: '1 toes', '2 faces', '3 pit',
  '4 strips', '5 seam pieces', '6 block lines', '7 block polygons', '8 blocks';
- it's saved with only the toes, the pit limit, the block lines and the blocks on show, so what the run makes is what
  you see; the grids, faces, pit, strips, seam pieces and polygons are a tick away in Layers.

The grids and the seam model are imported first and aren't steps either: they're the geology the workflow reads.

    .venv\\Scripts\\python testdata/synthetic/workflow_demo.py [folder]

Writes the grids and "Workflow demo.zdp" to the folder (testdata/synthetic/reserves/workflow demo by default), and prints
the steps, how long the run took and what it made.
"""

import pathlib
import sys
import time

import zd

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from ramp_template import at, made  # noqa: E402
from reserves_workflow import BASE, LIMIT_EAST, LIMIT_NORTH, LIMIT_SOUTH, LIMIT_WEST, STRIP_WIDTH, STRIPS, import_grids, write_grids  # noqa: E402

BLOCK_LENGTH = 100.0

# A note on each step, by its command (and its name where one command is used for several steps): why it's there.
NOTES = {
    "string.offset Toe": "The base toe offset north a strip width (60 m) five times: one toe a strip.",
    "string.number_across": "Each toe numbered by a line across them, south to north: its strip number.",
    "string.drape": "The toe draped on the L floor, so the face starts on the seam it mines to.",
    "face.save_design": "The highwall profile: 65 degrees to the base of weathering, 45 to the topography, on to RL 400 up and RL -300 down so each face passes right through the pit. A 30 m berm at the base of weathering. 50 m roads at 10 %: A straight up both walls of strips 1 to 5, B on strip 6 turning back on the berm, a switchback.",
    "face.project": "One face a toe, on '2 faces'.",
    "solid.between_surfaces": "The pit: topography to the L floor inside the pit limit, on '3 pit'.",
    "solid.slice_by_faces": "The pit sliced into strips by the faces, each strip what lies in front of its face, on '4 strips'. Checked first: every face must pass right through the pit.",
    "object.delete": "What's behind the last face isn't a strip: deleted.",
    "pause": "Look at the strips: tick '4 strips' in Layers. Delete any you don't want, then Continue.",
    "solid.cut_by_seams": "Each strip cut by the seam model's horizons in one run: overburden, U, U-M, M, M-L, L and below, each piece carrying its seam, material and horizon, on '5 seam pieces'. Checked first: the grids must cover every strip.",
    "string.offset Block line": "The first block line offset east every 100 m to the pit's east end: the block length is this distance.",
    "string.polygons_from_lines": "The block lines and the two closing lines made into block polygons B1 to B15, west to east, on '7 block polygons'.",
    "solid.cut_by_polygons": "Every seam piece cut vertically by the block polygons, each block named strip_block_horizon, on '8 blocks'. Checked first: the polygons must cover every piece.",
    "solid.interrogate": "Each coal block's qualities (ash, sulphur, RD) averaged from the seam grids over it, written onto it.",
}

# On show when it's saved: what you draw and what the workflow ends with.
SHOWN = {"0 pit limit", "0 ramp strings", "1 toes", "6 block lines", "8 blocks", "8 blocks 50m"}

# The roads: 50 m wide at 10 %, Tim's standard (coal ramp norms), each set by a string drawn across the strips.
ROAD = {"width": 50.0, "grade": 10.0}
# The berm at the base of weathering the road turns back on, and the least bench height that stands one.
BERM, MIN_BENCH = 30.0, 3.0
# The roads, each set by a string drawn across the strips it serves, starting where it crosses each toe 100 m inside the
# pit's west end: behind its start the toe moves back by the road, its entrance, and the face going down follows it.
# A climbs straight up both walls of strips 1 to 5; B, on strip 6, the final highwall, climbs east to the berm at the
# base of weathering, turns back on a switchback box there, and climbs west to the topography. A switchback's box is the
# road, the berm and the road again wide (130 m), more than a strip, so only the last strip, with nothing mined behind it,
# takes one. Name, x, south and north ends of its string, how it's held, and whether it turns.
RAMPS = (("A", 150.0, -150.0, 280.0, "start", False), ("B", 150.0, 280.0, 600.0, "start", True))


def drawn_by_hand(s):
    """What the workflow starts from, drawn as you would: the base toe, the pit limit and the block lines' frame."""
    toe = made(s.run("string.create", name="Base toe", layer="1 toes", points=[at(x, y, 0.0) for x, y in BASE]))[0]
    limit = made(s.run("string.create", name="Pit limit", layer="0 pit limit", closed=True,
                       points=[at(LIMIT_WEST, LIMIT_SOUTH), at(LIMIT_EAST, LIMIT_SOUTH), at(LIMIT_EAST, LIMIT_NORTH), at(LIMIT_WEST, LIMIT_NORTH)]))[0]
    first = made(s.run("string.create", name="Block line 0", layer="6 block lines",
                       points=[at(LIMIT_WEST, LIMIT_SOUTH - 200.0, -300.0), at(LIMIT_WEST, LIMIT_NORTH + 200.0, -300.0)]))[0]
    for name, y in (("South closing line", LIMIT_SOUTH - 100.0), ("North closing line", LIMIT_NORTH + 100.0)):
        s.run("string.create", name=name, layer="6 block lines", points=[at(LIMIT_WEST - 100.0, y, -300.0), at(LIMIT_EAST + 100.0, y, -300.0)])
    # A string across the strips for each road, the road starting where it crosses each toe.
    ramps = {name: made(s.run("string.create", name=f"Ramp {name}", layer="0 ramp strings", points=[at(x, south), at(x, north)]))[0] for name, x, south, north, _, _ in RAMPS}
    return toe, limit, first, ramps


def the_steps(s, g, model, toe, limit, first, ramps):
    """Toe to interrogated blocks, each run a step to capture; returns the runs in order."""
    runs = []

    def run(command, **params):
        record = s.run(command, params)
        runs.append(record["run"])
        return record

    offsets = made(run("string.offset", string=toe, distance=STRIP_WIDTH, side="left", times=STRIPS - 1, name="Toe", layer="1 toes"))
    toes = [toe, *offsets]
    run("string.number_across", strings=toes, start=1, **{"from": at(700.0, -100.0), "to": at(700.0, 600.0)})
    for t in toes:
        run("string.drape", string=t, surface=g["L sf"], crossings=False)
    toes_layer = next(layer["id"] for layer in s.layers() if layer["name"] == "1 toes")
    roads = [r[0] for r in RAMPS]
    turning = [r[0] for r in RAMPS if r[5]]
    # The road climbs the first wall to the base of weathering, turns back on a switchback box on the berm there, and
    # climbs the weathered wall the other way to the topography.
    steps = [
        {"step": "project", "to": {"kind": "surface", "id": g["BOW sf"]}, "batter": 65.0, "ramps": roads},
        {"step": "berm", "width": BERM, "min_bench": MIN_BENCH},
        {"step": "project", "to": {"kind": "surface", "id": g["TOPO sf"]}, "batter": 45.0, "ramps": roads, "turns": turning},
        {"step": "project", "to": {"kind": "rl", "rl": 400.0}, "batter": 45.0, "ramps": []},
    ]
    register = {"kind": "surface", "id": g["L sf"]}
    road_list = [{"name": name, "string": ramps[name], "fixed": fixed, "heading": "along", **ROAD} for name, _, _, _, fixed, _ in RAMPS]
    up = {"name": "Highwall", "direction": "up", "register": register, "condition": 0.1, "steps": steps, "ramps": road_list}
    down = {"name": "Highwall down", "direction": "down", "register": register, "condition": 0.1, "ramps": [],
            "steps": [{"step": "project", "to": {"kind": "rl", "rl": -300.0}, "batter": 65.0, "ramps": []}]}
    lines = [{"layer": toes_layer, "profile": name, "side": "left", "run_on": [100.0, 100.0]} for name in ("Highwall", "Highwall down")]
    design = made(run("face.save_design", name="Highwall", content={"layer": "2 faces", "lines": lines, "profiles": [up, down]}))[0]
    run("face.project", design=design)
    faces = [o["id"] for o in s.objects(layer="2 faces", kind="surface")]
    pit = made(run("solid.between_surfaces", top=g["TOPO sf"], bottom=g["L sf"], string=limit, layer="3 pit", name="Pit"))[0]
    strips = made(run("solid.slice_by_faces", solid=pit, faces=faces, layer="4 strips"))
    run("object.delete", object=strips[-1])
    pieces = made(run("solid.cut_by_seams", solids=strips[:-1], model=model, layer="5 seam pieces"))
    count = round((LIMIT_EAST - LIMIT_WEST) / BLOCK_LENGTH)
    run("string.offset", string=first, distance=BLOCK_LENGTH, side="right", times=count, name="Block line", layer="6 block lines")
    block_lines = [o["id"] for o in s.objects(layer="6 block lines", kind="string")]
    polygons = made(run("string.polygons_from_lines", strings=block_lines, layer="7 block polygons", name="B#"))
    blocks = made(run("solid.cut_by_polygons", solids=pieces, polygons=polygons, layer="8 blocks"))
    coal = [b for b in blocks if s.object(b)["attributes"].get("material") == "coal"]
    run("solid.interrogate", solids=coal, model=model)
    return runs


def noted(steps):
    """Each step's note, and a pause after the next strip is deleted."""
    out = []
    for step in steps:
        action = step["action"]
        name = action["settings"].get("name", "")
        step["note"] = NOTES.get(f"{action['command']} {name}") or NOTES.get(action["command"], "")
        out.append(step)
        if action["command"] == "object.delete":
            out.append({"id": max(s["id"] for s in steps) + 1, "note": NOTES["pause"], "action": {"kind": "pause"}})
    return out


def main():
    folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "reserves" / "workflow demo"
    folder.mkdir(parents=True, exist_ok=True)
    definition = write_grids(folder)
    s = zd.Session()
    g, model = import_grids(s, folder, definition)
    toe, limit, first, ramps = drawn_by_hand(s)
    started = time.perf_counter()
    runs = the_steps(s, g, model, toe, limit, first, ramps)
    by_hand = time.perf_counter() - started

    record = s.capture_workflow("Toe to blocks", runs)
    workflow = record["outputs"][0]["id"]
    picked = [n for n in record["notes"] if n.get("code") == "workflow.picked"]
    content = s.object(workflow)["workflow"]
    content["steps"] = noted(content["steps"])
    s.run("workflow.save", workflow=workflow, name="Toe to blocks", content=content)
    print(f"Captured {len(runs)} runs as 'Toe to blocks', {len(content['steps'])} steps with the pause; {len(picked)} inputs kept as picked")
    for k, step in enumerate(content["steps"], 1):
        action = step["action"]
        what = action["command"] if action["kind"] == "command" else "PAUSE"
        rules = ", ".join(f"{field} from {next(iter(r['from']))}" for field, r in action.get("inputs", {}).items())
        print(f"   {k:2}. {what}" + (f" ({rules})" if rules else ""))

    # A case (decision 0066): 50 m blocks, the block line offset 50 m 30 times instead of 100 m 15 times.
    content = s.object(workflow)["workflow"]
    blocks_step = next(st["id"] for st in content["steps"] if st["action"]["kind"] == "command" and st["action"]["command"] == "string.offset" and st["action"]["settings"].get("name") == "Block line")
    count = round((LIMIT_EAST - LIMIT_WEST) / BLOCK_LENGTH)
    content["cases"] = [{"name": "50m", "settings": {str(blocks_step): {"distance": BLOCK_LENGTH / 2, "times": 2 * count}}}]
    s.run("workflow.save", workflow=workflow, name="Toe to blocks", content=content)

    # Each case run as a workflow, so each step shows what its last run made: to the pause, then on from it.
    for case in ("Base", "50m"):
        started = time.perf_counter()
        first_part = s.run_workflow(workflow, case=case)
        pause = first_part.get("at")
        assert first_part["status"] == "paused", [(r["position"], r["outcome"], r.get("refused"), [n["text"] for n in r.get("checked", []) if n["severity"] != "info"][:3]) for r in first_part["steps"] if r["outcome"] != "ran"]
        rest = s.run_workflow(workflow, from_step=pause + 1, case=case)
        assert rest["status"] == "finished", [(r["position"], r.get("refused")) for r in rest["steps"] if r["outcome"] == "refused"]
        print(f"{case}: steps 1 to {pause - 1}, paused at {pause}, then {pause + 1} to the end, in {time.perf_counter() - started:.1f} s"
              + (f" (the runs by hand took {by_hand:.1f} s)" if case == "Base" else ""))

    # Each case's blocks add up to its seam pieces, and the two cases to the same total, to one part in a million million of
    # the total (75 m3 in 75 million): the solid booleans' rounding, which the cut by polygons itself says is within
    # 1.1e-10 of each piece's volume (0.0005 m3 on 4.6 million).
    tolerance = lambda total: 1e-12 * total
    totals = {}
    for case, suffix in (("Base", ""), ("50m", " 50m")):
        blocks = s.objects(layer=f"8 blocks{suffix}", kind="solid")
        pieces = s.objects(layer=f"5 seam pieces{suffix}", kind="solid")
        coal = [b for b in blocks if b["attributes"].get("material") == "coal"]
        tagged = all(b["attributes"].get("case") == case for b in blocks)
        total = sum(s.object(b["id"])["volume"] for b in blocks)
        within = sum(s.object(p["id"])["volume"] for p in pieces)
        totals[case] = total
        print(f"   {case}: {len(blocks)} blocks on '8 blocks{suffix}' ({len(coal)} coal), {total:,.6f} m3; seam pieces {within:,.6f} m3; "
              f"differ by {abs(total - within):.3g} m3; every block tagged case={case}: {tagged}")
        assert tagged and abs(total - within) <= tolerance(total), case
    assert abs(totals["Base"] - totals["50m"]) <= tolerance(totals["Base"]), totals
    print(f"   the two cases' totals differ by {abs(totals['Base'] - totals['50m']):.3g} m3")

    for layer in s.layers():
        s.set_layer_visible(layer["name"], layer["name"] in SHOWN)
    path = folder / "Workflow demo.zdp"
    s.save(path)
    print(f"Saved {path}, with {', '.join(sorted(SHOWN))} on show")


if __name__ == "__main__":
    main()
