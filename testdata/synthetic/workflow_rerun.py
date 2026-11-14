"""Tim's toe-to-blocks sequence on the made-up pit, captured from its runs as a workflow and run again (decision 0065).

Builds the made-up pit with reserves_workflow.py and reserves_workflow_v2.py into a folder of its own, opens what v2
saved, and captures the runs from the base toe to the interrogated blocks: the toe, its offsets, numbering and drapes,
the face design and its run, the pit limit and the pit solid (from the first script); then the slice into strips, the
next strip deleted, the cut by seams, the block lines, the polygons, the cut into blocks and the interrogation (from the
second). Then runs the workflow again from its first step and checks that it made the same blocks again:

- the same block solids (same ids, each at its next revision), nothing else made;
- each block's volume, and the total, the same to 1e-6 m3;
- each block's attributes the same, interrogated qualities included (to 1e-9, relative);
- one undo takes the whole run back.

It also runs again from the cut into blocks, keeping what the earlier steps made. Prints counts, rules and timings.

    .venv\\Scripts\\python testdata/synthetic/workflow_rerun.py [folder]
"""

import math
import pathlib
import sys
import tempfile
import time

import zd

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import reserves_workflow as v1  # noqa: E402
import reserves_workflow_v2 as v2  # noqa: E402

VOLUME_TOLERANCE = 1e-6  # m3
ATTRIBUTE_TOLERANCE = 1e-9  # relative


def build(folder):
    """The made-up pit, as the two scripts make it, saved in `folder`."""
    argv = sys.argv
    try:
        sys.argv = [argv[0], str(folder)]
        v1.main()
        v2.FOLDER = folder
        sys.argv = [argv[0]]
        v2.main()
    finally:
        sys.argv = argv
    return folder / "Reserves workflow v2.zdp"


def toe_to_blocks(log):
    """The runs from the base toe to the pit solid, then from the second slice to the interrogation."""
    opened = [k for k, e in enumerate(log) if e["event"] == "opened"]
    runs = [(k, e) for k, e in enumerate(log) if e["event"] == "run" and e["status"] == "ok"]
    first = next(k for k, e in runs if e["command"] == "string.create" and e["params"].get("name") == "Base toe")
    pit = next(k for k, e in runs if e["command"] == "solid.between_surfaces")
    # The first open is the second script's; the first script started a new project.
    again = next(k for k, e in runs if k > opened[0] and e["command"] == "solid.slice_by_faces")
    return [e for k, e in runs if first <= k <= pit or k >= again]


def blocks(s):
    return {o["id"]: s.object(o["id"]) for o in s.objects(layer="blocks", kind="solid")}


def same_attributes(a, b):
    if a.keys() != b.keys():
        return False
    for key, x in a.items():
        y = b[key]
        if isinstance(x, float) and isinstance(y, float):
            if not math.isclose(x, y, rel_tol=ATTRIBUTE_TOLERANCE, abs_tol=ATTRIBUTE_TOLERANCE):
                return False
        elif x != y:
            return False
    return True


def check_same(before, after, label):
    assert before.keys() == after.keys(), f"{label}: {len(before)} blocks before, {len(after)} after, {len(before.keys() ^ after.keys())} differ"
    worst = max(abs(after[i]["volume"] - b["volume"]) for i, b in before.items())
    total = (sum(b["volume"] for b in before.values()), sum(a["volume"] for a in after.values()))
    assert worst <= VOLUME_TOLERANCE, f"{label}: a block's volume moved {worst:.3g} m3"
    assert abs(total[0] - total[1]) <= VOLUME_TOLERANCE, f"{label}: the total moved {total[1] - total[0]:.3g} m3"
    changed = sum(not same_attributes(b["attributes"], after[i]["attributes"]) for i, b in before.items())
    assert changed == 0, f"{label}: {changed} blocks' attributes differ"
    assert all(after[i]["name"] == b["name"] for i, b in before.items()), f"{label}: names differ"
    print(f"   {len(after)} blocks the same: worst volume difference {worst:.3g} m3, total {total[1]:,.3f} m3 "
          f"(differs by {abs(total[1] - total[0]):.3g} m3), attributes the same")


def main():
    folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(tempfile.mkdtemp(prefix="zd-workflow-"))
    folder.mkdir(parents=True, exist_ok=True)
    print(f"Building the made-up pit in {folder}")
    path = build(folder)

    s = zd.open(path)
    picked = toe_to_blocks(s.log())
    by_hand = sum(e["duration_ms"] for e in picked)
    started = time.perf_counter()
    record = s.capture_workflow("Toe to blocks", [e["run"] for e in picked])
    workflow = record["outputs"][0]["id"]
    captured_ms = 1000 * (time.perf_counter() - started)
    steps = s.object(workflow)["workflow"]["steps"]
    rules = {}
    for step in steps:
        for rule in step["action"].get("inputs", {}).values():
            kind = next(iter(rule["from"]))
            rules[kind] = rules.get(kind, 0) + 1
    picked_warnings = sum(1 for n in record["notes"] if n.get("code") == "workflow.picked")
    print(f"Captured {len(steps)} steps in {captured_ms:.0f} ms; inputs by rule: "
          + ", ".join(f"{n} {kind}" for kind, n in sorted(rules.items())) + f"; {picked_warnings} kept as picked")
    for k, step in enumerate(steps, 1):
        inputs = step["action"].get("inputs", {})
        said = ", ".join(f"{field} from {next(iter(r['from']))}" + (f" where {r['where']}" if r.get("where") else "") for field, r in inputs.items())
        print(f"   {k:2}. {step['action']['command']}" + (f" ({said})" if said else ""))

    before = blocks(s)
    revisions = {i: b["revision"] for i, b in before.items()}
    objects = len(s.objects())
    started = time.perf_counter()
    run = s.run_workflow(workflow)
    rerun_ms = 1000 * (time.perf_counter() - started)
    assert run["status"] == "finished", f"stopped: {run['status']} at {run.get('at')}"
    print(f"Run again from step 1: {len(run['steps'])} steps in {rerun_ms / 1000:.1f} s (the runs by hand took {by_hand / 1000:.1f} s)")
    slowest = sorted(run["steps"], key=lambda r: -r["ms"])[:4]
    print("   slowest: " + ", ".join(f"{r['position']}. {r['command']} {r['ms'] / 1000:.1f} s" for r in slowest))
    after = blocks(s)
    check_same(before, after, "run again")
    # The cut makes each block again as its next revision; the interrogation then writes the coal blocks' qualities as
    # the revision after that.
    coal = {i for i, b in before.items() if b["attributes"].get("material") == "coal"}
    assert all(after[i]["revision"] == revisions[i] + (2 if i in coal else 1) for i in after), "each block made again as revisions"
    assert len(s.objects()) == objects, f"{len(s.objects()) - objects} objects more than before"
    print(f"   every block made again as its next revision ({len(coal)} coal blocks then interrogated); {objects} objects before and after")

    assert s.undo() == run["run"]
    assert all(s.object(i)["revision"] == revisions[i] for i in before), "one undo takes the whole run back"
    print("   one undo took the whole run back")
    s.redo()

    cut = next(k for k, step in enumerate(steps, 1) if step["action"]["command"] == "solid.cut_by_polygons")
    started = time.perf_counter()
    run = s.run_workflow(workflow, from_step=cut)
    assert run["status"] == "finished"
    print(f"Run again from step {cut} (the cut into blocks): {len(run['steps'])} steps in {time.perf_counter() - started:.1f} s")
    check_same(before, blocks(s), f"run from step {cut}")
    print("All pass.")


if __name__ == "__main__":
    main()
