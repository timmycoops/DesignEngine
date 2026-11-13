"""The made-up pit's strips taken to interrogated blocks the new way (decision 0062), saved beside the first as v2.

Run reserves_workflow.py first: this opens what it saved, takes out what the old steps made (a layer for each seam and
waste, blocks cut by near-vertical faces), and from the same strip solids:

1. cuts every strip by the seam model's horizons in one run onto `layers`, each piece carrying strip, seam, material and
   horizon (`solid.cut_by_seams`);
2. closes the block lines across the strips, with a line at each end of the pit and one south and north of it, into
   block polygons B1 to B15 (`string.polygons_from_lines`);
3. cuts every piece by them onto `blocks`, each taking its polygon's name as `block` and named strip_block_horizon
   (`solid.cut_by_polygons`);
4. interrogates the coal blocks by their `seam` (`solid.interrogate`), the column check at mid-seam.

The first file is left as it is.
"""

import pathlib
import sys
import time

import zd

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from ramp_template import at, made  # noqa: E402
from reserves_workflow import LIMIT_EAST, LIMIT_NORTH, LIMIT_SOUTH, LIMIT_WEST  # noqa: E402

FOLDER = HERE / "reserves"
OLD = ["overburden", "U coal", "U-M interburden", "M coal", "M-L interburden", "L coal", "refused at U roof", "block faces"]


def timed(label, record, started):
    print(f"   {label}: {len(record['outputs'])} made in {1000 * (time.perf_counter() - started):.0f} ms")
    for line in record["messages"][:3]:
        print(f"      {line}")
    for note in record["notes"]:
        if note.get("severity") == "warning":
            print(f"      warning: {note['text'][:200]}")


def clear_old(s):
    """What the old steps made, and the design that cut the old blocks."""
    names = OLD + [f"{layer} blocks" for layer in OLD[:6]]
    gone = [o["id"] for name in names for o in s.objects(layer=name)]
    gone += [o["id"] for o in s.objects(kind="face_design") if o["name"] == "Block cuts"]
    if gone:
        s.run("object.delete_many", objects=gone)
    for layer in s.layers():
        if layer["name"] in names:
            s.run("layer.delete", layer=layer["name"])
    print(f"1. Took out {len(gone)} objects the old steps made, on {len(names)} layers")


def main():
    s = zd.open(FOLDER / "Reserves workflow.zdp")
    clear_old(s)
    model = s.objects(kind="seam_model")[0]["id"]

    # The pit sliced into strips again by its faces, so the strips are named as slicing names them now: what's behind
    # the last face is the next strip, deleted here as it isn't one, as Tim does before the seam cut.
    old = [o["id"] for o in s.objects(layer="strips")]
    s.run("object.delete_many", objects=old)
    pit = next(o["id"] for o in s.objects(layer="pit") if o["kind"] == "solid")
    faces = [o["id"] for o in s.objects(layer="faces") if o["kind"] == "surface"]
    started = time.perf_counter()
    record = s.run("solid.slice_by_faces", solid=pit, faces=faces, layer="strips")
    timed(f"   The pit sliced again by its {len(faces)} faces onto 'strips'", record, started)
    sliced = made(record)
    behind = sliced[-1]
    print(f"   Deleted '{s.object(behind)['name']}', behind the last face")
    s.run("object.delete", object=behind)
    strips = sliced[:-1]

    started = time.perf_counter()
    record = s.run("solid.cut_by_seams", solids=strips, model=model, layer="layers")
    timed(f"2. {len(strips)} strips cut by seams onto 'layers'", record, started)
    pieces = [o["id"] for o in s.objects(layer="layers") if o["kind"] == "solid"]

    # The block lines across the strips (every 100 m inside the limit), one at each end of the pit, and one south and
    # north of it crossing them all.
    lines = [o["id"] for o in s.objects(layer="block lines") if o["kind"] == "string"]
    for name, x in (("Block line 0", LIMIT_WEST), ("Block line 15", LIMIT_EAST)):
        lines += made(s.run("string.create", name=name, layer="block lines", points=[at(x, LIMIT_SOUTH - 200.0, -300.0), at(x, LIMIT_NORTH + 200.0, -300.0)]))
    for name, y in (("South closing line", LIMIT_SOUTH - 100.0), ("North closing line", LIMIT_NORTH + 100.0)):
        lines += made(s.run("string.create", name=name, layer="block lines", points=[at(LIMIT_WEST - 100.0, y, -300.0), at(LIMIT_EAST + 100.0, y, -300.0)]))
    started = time.perf_counter()
    record = s.run("string.polygons_from_lines", strings=lines, layer="block polygons", name="B#")
    timed(f"3. {len(lines)} block lines closed into polygons", record, started)
    polygons = made(record)

    started = time.perf_counter()
    record = s.run("solid.cut_by_polygons", solids=pieces, polygons=polygons, layer="blocks")
    timed(f"4. {len(pieces)} pieces cut into blocks onto 'blocks'", record, started)

    coal = [o["id"] for o in s.objects(layer="blocks") if o["kind"] == "solid" and o["attributes"].get("material") == "coal"]
    started = time.perf_counter()
    record = s.run("solid.interrogate", solids=coal, model=model)
    timed(f"5. {len(coal)} coal blocks interrogated", record, started)

    path = FOLDER / (sys.argv[1] if len(sys.argv) > 1 else "Reserves workflow v2.zdp")
    s.save(path)
    print(f"Saved {path}")


if __name__ == "__main__":
    main()
