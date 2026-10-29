"""The Phase 1 gate's Python item, end to end: open a project, list its objects, run a command and
read the run log (decision 0012). Plain asserts, so it runs with nothing but the built `zd` module:

    python crates/zd-py/tests/test_session.py
"""

import math
import os
import pathlib
import struct
import tempfile

import zd

E, N = 650_000.1, 7_450_000.2


def bits(points):
    return [struct.pack("<d", c) for p in points for c in p]


def a_project_is_made_saved_and_opened(tmp):
    s = zd.Session()
    assert repr(s) == "<zd.Session: 0 objects on 0 layers>"
    toe = s.run("string.create", name="toe", layer="design", points=[[E, N, 50.0], [E + 100.3, N, 50.0], [E + 100.3, N + 100.7, 50.0]])
    toe_id = toe["outputs"][0]["id"]
    # 'from' is reserved in Python: pass it in a dict.
    s.run("string.set_grade", {"string": toe_id, "from": 0}, grade=10.0)
    s.run("string.offset", string=toe_id, distance=10.0, side="right", layer="crests")
    box = [[E + i % 2 * 10.0, N + i // 2 % 2 * 20.0, 30.0 + i // 4 * 5.0] for i in range(8)]
    faces = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]]
    s.run("solid.from_mesh", name="block", layer="solids", vertices=box, triangles=faces)
    report = s.save(pathlib.Path(tmp) / "pit.zdproj")
    assert report["object_files_written"] >= 1
    return s


def the_gate_open_list_run_read(tmp, original):
    # Open the saved project.
    s = zd.open(os.path.join(tmp, "pit.zdproj"))
    assert repr(s) == "<zd.Session: 3 objects on 3 layers>"

    # List its objects.
    names = sorted(o["name"] for o in s.objects())
    assert names == ["block", "toe", "toe offset 10 m right"], names
    assert [o["name"] for o in s.objects(kind="solid")] == ["block"]
    assert [o["name"] for o in s.objects(layer="crests")] == ["toe offset 10 m right"]
    assert sorted(layer["name"] for layer in s.layers()) == ["crests", "design", "solids"]

    # Measurements, and every coordinate exactly as saved.
    toe = next(o for o in s.objects() if o["name"] == "toe")
    detail = s.object(toe["id"])
    assert detail["kind"] == "string" and detail["revision"] == 2
    assert bits(detail["points"]) == bits(original.object(toe["id"])["points"])
    assert detail["points"][0] == [E, N, 50.0]
    assert math.isclose(detail["points"][2][2], 50.0 + 0.1 * (100.3 + 100.7), abs_tol=1e-9), detail["points"]
    block = s.object(s.objects(kind="solid")[0]["id"])
    assert abs(block["volume"] - 1000.0) < 1e-6, block["volume"]
    assert block["validity"]["closed"] and block["crossings"]["pairs"] == 0
    mesh = s.mesh(block["id"])
    assert len(mesh["vertices"]) == 8 and len(mesh["triangles"]) == 12

    # Run a command.
    record = s.run("string.set_elevation", string=toe["id"], rl=42.5)
    assert record["status"] == "ok" and record["outputs"][0]["change"] == "revised"
    assert all(p[2] == 42.5 for p in s.object(toe["id"])["points"])

    # Read the run log: it carries on from the saved project's.
    log = s.log()
    assert [e["event"] for e in log][-3:] == ["saved", "opened", "run"], [e["event"] for e in log]
    assert log[-1]["command"] == "string.set_elevation"
    assert sum(1 for e in log if e["event"] == "run") == 5

    # A refused run raises, and is logged as failed.
    try:
        s.run("string.offset", string=toe["id"], distance=10.0, side="up")
        raise AssertionError("expected zd.CommandError")
    except zd.CommandError as e:
        assert "side" in str(e) or "variant" in str(e), str(e)
    assert s.log()[-1]["status"] != "ok"
    assert issubclass(zd.CommandError, zd.Error)

    # Undo and redo.
    assert s.can_undo
    assert s.undo() == record["run"]
    assert all(p[2] != 42.5 for p in s.object(toe["id"])["points"])
    assert s.redo() == record["run"]

    # The command list.
    assert "solid.import" in [c["name"] for c in s.commands()]

    # Bad input is a Python error, not a crash.
    for bad in (lambda: s.object("not-an-id"), lambda: zd.open(os.path.join(tmp, "missing.zdproj")), lambda: s.run("string.create", name="x", layer="d", points=[[float("nan"), 0, 0], [1, 1, 1]])):
        try:
            bad()
            raise AssertionError("expected an error")
        except (ValueError, zd.Error):
            pass


def main():
    with tempfile.TemporaryDirectory() as tmp:
        original = a_project_is_made_saved_and_opened(tmp)
        the_gate_open_list_run_read(tmp, original)
    print(f"zd {zd.__version__}: open, list, run and read the log all pass")


if __name__ == "__main__":
    main()
