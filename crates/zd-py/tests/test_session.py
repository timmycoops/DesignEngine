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
    report = s.save(pathlib.Path(tmp) / "pit.zdp")
    assert report["bytes"] > 0 and not report["unchanged"], report
    assert s.save(pathlib.Path(tmp) / "pit.zdp")["unchanged"], "nothing changed, nothing written"
    return s


def the_gate_open_list_run_read(tmp, original):
    # Open the saved project.
    s = zd.open(os.path.join(tmp, "pit.zdp"))
    assert repr(s) == "<zd.Session: 3 objects on 3 layers>"
    assert s.open_notes == [] and s.links() == [], "no seam models, so nothing to find"

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
    assert block["bounds"] == [[E, N, 30.0], [E + 10.0, N + 20.0, 35.0]], block["bounds"]
    mesh = s.mesh(block["id"])
    assert len(mesh["vertices"]) == 8 and len(mesh["triangles"]) == 12

    # Run a command.
    record = s.run("string.set_elevation", string=toe["id"], rl=42.5)
    assert record["status"] == "ok" and record["outputs"][0]["change"] == "revised"
    assert [n["text"] for n in record["notes"]] == record["messages"], "each line a note, its text as before"
    assert "refused" not in record
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
    for bad in (lambda: s.object("not-an-id"), lambda: zd.open(os.path.join(tmp, "missing.zdp")), lambda: s.run("string.create", name="x", layer="d", points=[[float("nan"), 0, 0], [1, 1, 1]])):
        try:
            bad()
            raise AssertionError("expected an error")
        except (ValueError, zd.Error):
            pass


def a_workflow_is_captured_and_run_again():
    """Decision 0065: a 100 m by 60 m pit from RL 0 to 100 cut at RL 40, captured from its runs and run again: the same
    solids at their next revisions, 240,000 and 360,000 m3 (to 1e-6 m3), undone in one."""
    s = zd.Session()
    e, n = 650_000.0, 7_450_000.0
    corners = [[e - 100, n - 100], [e + 300, n - 100], [e + 300, n + 300], [e - 100, n + 300]]
    top, bottom = (s.run("surface.from_mesh", name=name, layer="surfaces", vertices=[[x, y, z] for x, y in corners], triangles=[[0, 1, 2], [0, 2, 3]])["outputs"][0]["id"]
                   for name, z in (("Top", 100.0), ("Bottom", 0.0)))
    outline = s.run("string.create", name="Outline", layer="outline", closed=True,
                    points=[[e, n, 0.0], [e + 100, n, 0.0], [e + 100, n + 60, 0.0], [e, n + 60, 0.0]])["outputs"][0]["id"]
    solid = s.run("solid.between_surfaces", top=top, bottom=bottom, string=outline, layer="pit")
    cut = s.run("solid.cut_by_surface", solids=[solid["outputs"][0]["id"]], rl=40.0, layer="cut")
    workflow = s.capture_workflow("Box pit", [solid["run"], cut["run"]])["outputs"][0]["id"]
    steps = s.object(workflow)["workflow"]["steps"]
    assert steps[1]["action"]["inputs"]["solids"]["from"] == {"step": 1}, steps[1]
    pieces = [o["id"] for o in cut["outputs"]]
    record = s.run_workflow(workflow)
    assert record["status"] == "finished" and [step["outcome"] for step in record["steps"]] == ["ran", "ran"], record
    volumes = sorted(s.object(p)["volume"] for p in pieces)
    assert [s.object(p)["revision"] for p in pieces] == [2, 2]
    assert abs(volumes[0] - 240_000.0) < 1e-6 and abs(volumes[1] - 360_000.0) < 1e-6, volumes
    assert s.log()[-1]["event"] == "workflow"
    assert s.undo() == record["run"] and [s.object(p)["revision"] for p in pieces] == [1, 1]
    paused = s.run_workflow(workflow, from_step=2)
    assert paused["from"] == 2 and paused["status"] == "finished"
    # A case (decision 0066): cut at RL 60 on its own layers, tagged with its case.
    content = s.object(workflow)["workflow"]
    content["cases"] = [{"name": "60", "settings": {"2": {"rl": 60.0}}}]
    s.run("workflow.save", workflow=workflow, name="Box pit", content=content)
    records = s.run_all_cases(workflow)
    assert [(r["case"], r["status"]) for r in records] == [("Base", "finished"), ("60", "finished")], records
    case = sorted(s.object(o["id"])["volume"] for o in s.objects(layer="cut 60", kind="solid"))
    assert abs(case[0] - 240_000.0) < 1e-6 and abs(case[1] - 360_000.0) < 1e-6, case
    assert all(o["attributes"]["case"] == "60" for o in s.objects(layer="cut 60"))
    assert s.run_workflow(workflow, case="60")["case"] == "60"
    # A layer shown or hidden: display state, not a run.
    runs = len(s.log())
    assert s.set_layer_visible("surfaces", False) and not next(l for l in s.layers() if l["name"] == "surfaces")["visible"]
    assert not s.set_layer_visible("no such layer", False) and len(s.log()) == runs


def several_strings_are_one_run():
    """Decision 0078: three toes set to RL 120 as `strings` are one run and one undo; one that can't be done refuses
    the run with every string as it was."""
    s = zd.Session()
    e, n = 650_000.0, 7_450_000.0
    toes = [s.run("string.create", name=f"Toe {k + 1}", layer="toes", points=[[e, n - 50 * k, 100.0 + k], [e + 200, n - 50 * k, 100.0 + k], [e + 400, n - 50 * k + 30, 100.0 + k]])["outputs"][0]["id"] for k in range(3)]
    record = s.run("string.set_elevation", strings=toes, rl=120.0)
    assert len(record["outputs"]) == 3 and record["messages"][0] == "set to RL 120.000: 3 of 3 strings"
    assert all(p[2] == 120.0 for toe in toes for p in s.object(toe)["points"])
    short = s.run("string.create", name="Short", layer="toes", points=[[e, n + 500, 0.0], [e + 100, n + 500, 0.0]])["outputs"][0]["id"]
    try:
        s.run("string.set_closed", strings=toes + [short], closed=True)
        raise AssertionError("expected zd.CommandError")
    except zd.CommandError as refused:
        assert "1 of 4 strings can't be done, so nothing was changed" in str(refused), refused
    assert not any(s.object(toe)["closed"] for toe in toes)
    s.undo()
    s.undo()
    assert [s.object(toe)["points"][0][2] for toe in toes] == [100.0, 101.0, 102.0], "one undo for the three"


def pieces_are_joined_into_one():
    """Decision 0078, workstream 3: three pieces of a toe given as `strings` are joined end to end into the first, a
    3-4-5 gap bridged and said as 5 m, in one run."""
    s = zd.Session()
    e, n = 650_000.0, 7_450_000.0
    pieces = [[[e + 10, n, 50.0], [e + 20, n, 50.0]], [[e + 10, n, 50.0], [e, n, 50.0]], [[e + 23, n + 4, 50.0], [e + 40, n + 4, 50.0]]]
    ids = [s.run("string.create", name=f"Piece {k + 1}", layer="pieces", points=points)["outputs"][0]["id"] for k, points in enumerate(pieces)]
    record = s.run("string.join", strings=ids)
    assert record["messages"][0] == "joined 3 strings into 'Piece 1': 5 vertices; 1 join met, 1 gap bridged, 5.000 m in all", record["messages"]
    assert [p[0] - e for p in s.object(ids[0])["points"]] == [0.0, 10.0, 20.0, 23.0, 40.0]
    assert sum(1 for o in record["outputs"] if o["change"] == "deleted") == 2


def a_case_runs_its_dataflows_in_order():
    """Reserves case plan, decisions 2, 4 and 14: two strips, each a block of OB (10 m) over 3 m of coal at RD 1.5,
    100 m by 50 m: 22,500 t of coal a block. Haulage, a row dataflow on strip 1 only, gives 2 km and 45,000 t.km there
    and nothing on strip 2. `run_case` runs the reserves dataflow, then Haulage: two records, in order."""
    s = zd.Session()
    e, n = 650_000.0, 7_450_000.0
    faces = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]]
    cells = []
    for strip in (1, 2):
        x = (strip - 1) * 100.0
        for name, z0, z1, a in (("OB", 20.0, 30.0, {"horizon": "OB", "material": "waste", "seam": "L"}),
                                ("L", 12.0, 15.0, {"horizon": "L", "material": "coal", "seam": "L", "rd": 1.5})):
            box = [[e + x + i % 2 * 100.0, n + i // 2 % 2 * 50.0, z0 if i < 4 else z1] for i in range(8)]
            solid = s.run("solid.from_mesh", name=f"{strip}_{name}", layer="blocks", vertices=box, triangles=faces)["outputs"][0]["id"]
            for k, v in dict(a, strip=float(strip), block="B1").items():
                cells.append({"object": solid, "attribute": k, "value": v})
    s.run("object.set_attributes", cells=cells)
    blocks = [o["id"] for o in s.objects(layer="blocks")]
    assumptions = s.run("parameters.save", name="assumptions", csv="when material,rd (t/m3)\nwaste,2.3\n")["outputs"][0]["id"]
    fields = [{"name": "volume", "kind": "volume", "add_up": {"by": "sum"}, "from": "volume"},
              {"name": "thickness", "kind": "thickness", "add_up": {"by": "sum"}, "from": "thickness"},
              {"name": "rd", "kind": "density", "add_up": {"by": "volume"}, "from": "attribute", "attribute": "rd"}]
    reserves = s.run("dataflow.save", name="Reserves", content={"fields": fields, "steps": [{"step": "initialise"}, {"step": "final"}], "parameters": [assumptions]})["outputs"][0]["id"]
    table = s.run("reserves.run", dataflow=reserves, blocks=blocks, stack=["OB", "L"], layer="reserves")["outputs"][0]["id"]
    distances = s.run("parameters.save", name="haul distances", csv="when strip,distance (km)\n1,2\n,5\n")["outputs"][0]["id"]
    haul = "def formula(row, p):\n    row.distance = p.distance\n    row.tkm = row[\"Imported\\\\Coal\\\\tonnes\"] * p.distance\n"
    haulage = s.run("dataflow.save", name="Haulage", content={
        "rows": True, "units": {"distance": "km", "tkm": "t.km"}, "parameters": [distances],
        "fields": [{"name": "distance", "kind": "other", "add_up": {"by": "max"}, "from": "worked"}, {"name": "tkm", "kind": "other", "add_up": {"by": "sum"}, "from": "worked"}],
        "steps": [{"step": "haul", "formula": {"script": haul}}]})["outputs"][0]["id"]
    setup = s.table_setup(table)
    setup["row_dataflows"] = [{"dataflow": haulage, "range": {"strip": "1"}}]
    s.run("table.setup", table=table, setup=setup)
    records = s.run_case(table)
    assert [r["command"] for r in records] == ["reserves.run", "table.run_dataflow"], [r["command"] for r in records]
    assert "Haulage ran on 1 of 2 rows (strip 1)" in records[1]["messages"][0], records[1]["messages"]
    t = s.object(table)
    assert t["units"]["Haulage\\tkm"] == "t.km"
    by_strip = dict(zip(t["columns"]["strip"], t["columns"]["Haulage\\tkm"]))
    assert abs(by_strip["1"] - 45_000.0) < 1e-6 and by_strip["2"] is None, by_strip
    only = s.run_case(table, dataflow=haulage)
    assert [r["command"] for r in only] == ["table.run_dataflow"], "that one alone"
    # Out of date (decision 6): both up to date; nothing to run. Haulage's distances revised: it alone is out of date,
    # said why, and run alone. The reserves dataflow run again: Haulage after it, run too.
    state = s.case_state(table)
    assert [(d["name"], d["up_to_date"], d["why"]) for d in state["dataflows"]] == [("Reserves", True, []), ("Haulage", True, [])], state
    assert s.run_case(table, out_of_date=True) == []
    s.run("parameters.save", table=distances, name="haul distances", csv="when strip,distance (km)\n1,3\n,5\n")
    assert s.case_state(table)["dataflows"][1]["why"] == ["haul distances changed"]
    assert [r["command"] for r in s.run_case(table, out_of_date=True)] == ["table.run_dataflow"]
    s.run_case(table, dataflow=reserves)
    assert s.case_state(table)["dataflows"][1]["why"] == ["Reserves ran after it"]
    assert [r["command"] for r in s.run_case(table, out_of_date=True)] == ["table.run_dataflow"]
    assert all(d["up_to_date"] for d in s.case_state(table)["dataflows"])
    assert [d["tables"] for d in s.case_state(table)["dataflows"]] == [["assumptions"], ["haul distances"]]
    heads = s.case_headlines()
    assert [(h["table"], h["base"]) for h in heads] == [(table, True)], heads
    # Haulage at its step (workstream 6): strip 1 at 3 km as saved, 67,500 t.km; at 4 km as edited, 90,000, not saved.
    at = s.row_step(table, "Haulage", 1)
    tkm = {c["field"]: c for c in at["columns"]}["tkm"]
    assert at["rows_in_range"] + at["outside"] == s.object(table)["rows"] and tkm["before"] is None, at
    assert abs(tkm["after"] - 67_500.0) < 1e-6 and abs(tkm["last_run"] - 67_500.0) < 1e-6, tkm
    edited = s.row_step(table, haulage, 1, tables={"haul distances": "when strip,distance (km)\n1,4\n,5\n"})
    assert abs({c["field"]: c for c in edited["columns"]}["tkm"]["after"] - 90_000.0) < 1e-6, edited["columns"]
    assert "1,3" in s.object(distances)["csv"], "the edit wasn't saved"
    a_case_is_kept_at_a_step(s, table, assumptions)


def a_case_is_kept_at_a_step(s, table, assumptions):
    """Reserves case plan, decisions 8 and 9: the case of the two strips kept at `small_volumes`, then at `final`. Each
    strip's coal, 15,000 m3, is over a minimum volume of 10 m3; edited to 20,000 m3 (not saved), `small_volumes` runs
    again from the columns kept before it and sets both aside, 30,000 m3; the table from there has the table's rows, and
    nothing is saved."""
    edit = "when material,rd (t/m3),min_volume (m3)\nwaste,2.3,\ncoal,,{}\n"
    s.run("parameters.save", table=assumptions, name="assumptions", csv=edit.format(10))
    flow = s.table_setup(table)["dataflow"]
    content = s.object(flow)["dataflow"]
    content["steps"] = [{"step": "small_volumes"}] + content["steps"]
    s.run("dataflow.save", dataflow=flow, name="Reserves", content=content)
    s.run_case(table, dataflow=flow)
    totals = s.step_case(table, "small_volumes")
    assert (totals["step"], totals["name"], totals["steps"]) == (1, "small_volumes", ["small_volumes", "initialise", "final"]), totals
    assert totals["balance"]["balanced"] and abs(totals["balance"]["volume_in"] - 130_000.0) < 1e-6, totals["balance"]
    assert totals["said"] == [], totals["said"]
    totals = s.step_case(table, "small_volumes", tables={"assumptions": edit.format(20_000)})
    assert totals["ran_from"] == 1 and [r["name"] for r in totals["ran"]] == ["small_volumes"], totals["ran"]
    said = {x["what"]: (x["units"], x["volume"]) for x in totals["said"]}
    assert said["set aside as small volumes"][0] == 2 and abs(said["set aside as small volumes"][1] - 30_000.0) < 1e-6, said
    units = s.step_units(table)
    assert units["table"]["rows"] == 4 and units["step"] == "small_volumes", units
    totals = s.step_case(table, "final", tables={"assumptions": edit.format(20_000)})
    assert totals["ran_from"] is None and [r["name"] for r in totals["ran"]] == ["initialise", "final"], totals["ran"]
    # The flow (decision 10): the case after every step, the blocks first, every step adding back to the 130,000 m3 in.
    flow = s.step_flow(table)
    assert [f["name"] for f in flow] == ["as input", "small_volumes", "initialise", "final"], [f["name"] for f in flow]
    assert all(abs(sum(g["volume"] for g in f["groups"]) - 130_000.0) < 1e-6 for f in flow), flow
    made = s.step_table(table)
    assert made["rows"] == s.object(table)["rows"], made["rows"]
    assert "coal,,10" in s.object(assumptions)["csv"], "the edit wasn't saved"


def main():
    with tempfile.TemporaryDirectory() as tmp:
        original = a_project_is_made_saved_and_opened(tmp)
        the_gate_open_list_run_read(tmp, original)
    a_workflow_is_captured_and_run_again()
    several_strings_are_one_run()
    pieces_are_joined_into_one()
    a_case_runs_its_dataflows_in_order()
    print(f"zd {zd.__version__}: open, list, run and read the log all pass")


if __name__ == "__main__":
    main()
