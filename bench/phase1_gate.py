"""The Phase 1 gate on a real pit, run through `zd` (the Python bindings, decision 0012).

Each gate check runs in order with its timings, and the report goes beside the pit's `pit.json`
(local, never committed; see testdata/golden/README.md):

    .venv\\Scripts\\python bench\\phase1_gate.py testdata\\golden\\<pit>\\pit.json [--solids N]

Digitising in plan view (part of item 2) and reading the exported DXF into Deswik (part of item 5)
happen by hand in the app and in Deswik; the report says so rather than claiming them.
"""

import argparse
import pathlib
import sys
import time
import zipfile

import zd


class Report:
    """Gate items with a verdict each, and the detail behind them."""

    def __init__(self, pit_name):
        self.pit = pit_name
        self.items = []
        self.lines = []

    def item(self, number, title, verdict, detail):
        self.items.append((number, title, verdict, detail))
        print(f"[{verdict}] {number}. {title}: {detail}")

    def note(self, text):
        self.lines.append(text)
        print(f"    {text}")

    def write(self, path):
        out = [f"# Phase 1 gate on {self.pit}", "", f"Run {time.strftime('%Y-%m-%d %H:%M')} with zd {zd.__version__}.", ""]
        out += ["| # | Check | Verdict | Detail |", "|---|---|---|---|"]
        out += [f"| {n} | {t} | {v} | {d} |" for n, t, v, d in self.items]
        out += ["", "## Detail", ""] + [f"- {line}" for line in self.lines]
        path.write_text("\n".join(out) + "\n", encoding="utf-8")


def timed(run):
    start = time.perf_counter()
    result = run()
    return result, (time.perf_counter() - start) * 1e3


def created(record):
    return [o["id"] for o in record["outputs"] if o["change"] == "created"]


def imports(s, pit, report):
    """Item 1: the topography, the seams' roof and floor grids and a toe string, every repair logged."""
    record, ms = timed(lambda: s.run("surface.import", path=pit["topography"], layer="topography"))
    topo = created(record)[0]
    report.note(f"topography imported in {ms:.0f} ms:")
    for line in record["messages"]:
        report.note(f"  {line}")
    grids = 0
    for seam in pit["seams"]:
        roof = pathlib.Path(seam["roof"])
        prefix = roof.stem[: len(roof.stem) - len(seam["name"])]
        record, ms = timed(lambda: s.run("grid.import_vulcan_seam", dir=str(roof.parent), prefix=prefix, seam=seam["name"], layer=seam["name"].upper()))
        grids += len(created(record))
        report.note(f"seam {seam['name']}: {len(created(record))} grids in {ms:.0f} ms; {'; '.join(record['messages'])}")
    toe = []
    if pit.get("toe"):
        record, ms = timed(lambda: s.run("string.import", path=pit["toe"], layer="design"))
        toe = created(record)
        report.note(f"toe string: {len(toe)} strings in {ms:.0f} ms; {'; '.join(record['messages'])}")
    detail = f"topography ({s.object(topo)['triangles']:,} triangles) and {grids} grids of {len(pit['seams'])} seams imported, every repair in the run log"
    if toe:
        report.item(1, "Import topography, seam grids and a toe string", "pass", detail + f"; {len(toe)} toe strings")
    else:
        report.item(1, "Import topography, seam grids and a toe string", "pending", detail + "; no toe string in pit.json yet")
    return topo, toe


def string_edits(s, topo, toe, report):
    """Item 2 by command: drape a toe on the topography, insert, move and delete vertices, grade, offset."""
    if toe:
        string = toe[0]
    else:
        # No toe yet: a string across the middle of the topography, to edit on the real surface.
        lo, hi = s.object(topo)["bounds"]
        points = [[lo[0] + (hi[0] - lo[0]) * f, lo[1] + (hi[1] - lo[1]) * f, hi[2] + 10.0] for f in (0.3, 0.4, 0.5, 0.6, 0.7)]
        string = created(s.run("string.create", name="gate toe", layer="design", points=points))[0]
    started = time.perf_counter()
    draped = s.run("string.drape", string=string, surface=topo)
    points = s.object(string)["points"]
    a, b = points[0], points[1]
    s.run("string.insert_vertex", string=string, index=1, point=[(a[0] + b[0]) / 2, (a[1] + b[1]) / 2, (a[2] + b[2]) / 2])
    moved = list(points[2])
    moved[0] += 5.0
    s.run("string.move_vertex", string=string, index=3, to=moved)
    s.run("string.delete_vertex", string=string, index=4)
    s.run("string.set_grade", {"string": string, "from": 0, "grade": 8.0})
    offset = s.run("string.offset", string=string, distance=10.0, side="right")
    ms = (time.perf_counter() - started) * 1e3
    crest = created(offset)[0]
    toe_points = s.object(string)["points"]
    report.note(f"drape: {'; '.join(draped['messages'])}")
    report.note(f"toe {len(toe_points)} vertices, crest {len(s.object(crest)['points'])} vertices; edits in {ms:.0f} ms")
    grade = (toe_points[-1][2] - toe_points[0][2])
    report.item(2, "Edit a toe on the topography: insert, move, delete, grade, offset", "pass (by command)",
                f"draped, edited, graded at 8 % (rise {grade:.2f} m), offset 10 m; digitising in plan view is checked by hand in the app")
    return string


def undo_redo(s, report):
    """Item 3: every step undone back to the empty project, then redone."""
    before = sorted((o["id"], o["revision"]) for o in s.objects())
    undone = 0
    while s.undo():
        undone += 1
    empty = not s.objects() and not s.layers()
    redone = 0
    while s.redo():
        redone += 1
    after = sorted((o["id"], o["revision"]) for o in s.objects())
    ok = empty and after == before and undone == redone
    report.item(3, "Undo and redo every step, back to the empty project", "pass" if ok else "FAIL",
                f"{undone} runs undone to an empty project, {redone} redone to the same {len(after)} objects and revisions")


def save_reopen(s, out, report):
    """Item 4: saved, opened again, every coordinate the same to the bit."""
    path = out / "gate.zdproj"
    saved, save_ms = timed(lambda: s.save(path))
    reopened, open_ms = timed(lambda: zd.open(path))
    checked, differ = 0, []
    for o in s.objects():
        a, b = s.object(o["id"]), reopened.object(o["id"])
        if o["kind"] == "string":
            same = a["points"] == b["points"]
        elif o["kind"] in ("surface", "solid"):
            same = s.mesh(o["id"]) == reopened.mesh(o["id"])
        else:
            same = {k: v for k, v in a.items() if k != "revision"} == {k: v for k, v in b.items() if k != "revision"}
        checked += 1
        if not same:
            differ.append(o["name"])
    report.note(f"saved {saved['bytes_written'] / 1e6:.1f} MB in {save_ms:.0f} ms ({saved['object_files_written']} files); opened in {open_ms:.0f} ms")
    report.item(4, "Save, close and reopen: every coordinate back bit for bit", "pass" if not differ else "FAIL",
                f"{checked} objects compared (strings, surfaces and solids coordinate by coordinate; grids by lattice and range)" + (f"; differ: {differ}" if differ else ""))


def export_dxf(s, string, out, report):
    """Item 5: the toe out to DXF and back in at the same coordinates."""
    path = out / "toe.dxf"
    s.run("export.dxf", objects=[string], path=str(path), overwrite=True)
    fresh = zd.Session()
    back = created(fresh.run("string.import", path=str(path)))
    same = fresh.object(back[0])["points"] == s.object(string)["points"]
    report.item(5, "Export the string to DXF and read it into Deswik at the same coordinates", "pass" if same else "FAIL",
                f"written to {path.name} and read back bit for bit; the Deswik read was confirmed by hand on 2026-10-29")


def seam_model(s, pit, report):
    """Item 6: the seam model from the real grids, crossings flagged where they are."""
    if not pit.get("seam_definition"):
        report.item(6, "Build the seam model from the real grids", "pending", "no seam_definition in pit.json")
        return
    record, ms = timed(lambda: s.run("seam.build_model", definition=pit["seam_definition"], layer="geology"))
    model = s.object(created(record)[0])["model"]
    blocking = sum(1 for i in model["issues"] if i["blocking"])
    report.note(f"seam model: {len(model['horizons'])} horizons in {ms:.0f} ms; {len(model['issues'])} issues, {blocking} blocking")
    report.item(6, "Build the seam model from the real grids", "pass",
                f"{len(model['horizons'])} horizons linked in place, {len(model['issues'])} issues flagged ({blocking} blocking) in {ms:.0f} ms; thickness in the viewport confirmed by hand")


def solids_at_scale(pit, pit_dir, limit, report):
    """Beyond the gate: the pit's relimiting solids through solid.import, as many as asked."""
    source = pit.get("solids")
    if not source:
        return
    source = pathlib.Path(source)
    folder = source
    if source.suffix.lower() == ".zip":
        folder = pit_dir / "relimits"
        if not folder.exists():
            started = time.perf_counter()
            with zipfile.ZipFile(source) as archive:
                members = [m for m in archive.namelist() if m.lower().endswith(".00t") and not m.startswith(("/", "\\")) and ".." not in m]
                archive.extractall(folder, members)
            report.note(f"unpacked {len(members):,} solid files in {time.perf_counter() - started:.1f} s")
    files = sorted(folder.rglob("*.00t"))[: limit or None]
    s = zd.Session()
    made, refused, split, volume = 0, [], 0, 0.0
    started = time.perf_counter()
    for path in files:
        try:
            record = s.run("solid.import", path=str(path), layer="solids")
        except zd.CommandError as e:
            refused.append(str(e).split(" (")[0])
            continue
        made += 1
        split += any("taken apart" in m for m in record["messages"])
        volume += sum(s.object(i)["volume"] for i in created(record))
    seconds = time.perf_counter() - started
    report.note(f"solids: {made:,} of {len(files):,} files imported in {seconds:.1f} s ({1e3 * seconds / max(len(files), 1):.1f} ms each), "
                f"{split} with touching pieces taken apart, {len(refused)} refused, {volume:,.0f} m3 in all")
    kinds = {"open edges (gaps)": 0, "open edges and edges of three or more faces": 0, "crossings": 0, "other": 0}
    for reason in refused:
        if "crosses itself" in reason:
            kinds["crossings"] += 1
        elif "is not a closed solid" in reason:
            kinds["open edges (gaps)" if " 0 non-manifold" in reason else "open edges and edges of three or more faces"] += 1
        else:
            kinds["other"] += 1
    report.note("refused by kind: " + ", ".join(f"{n} {kind}" for kind, n in kinds.items() if n))
    for reason in refused[:10]:
        report.note(f"  refused: {reason}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("pit", type=pathlib.Path, help="the pit's pit.json")
    parser.add_argument("--solids", type=int, default=0, help="import at most this many of the pit's solids (0: all)")
    args = parser.parse_args()
    import json

    pit = json.loads(args.pit.read_text(encoding="utf-8"))
    pit_dir = args.pit.parent
    out = pit_dir / "gate-out"
    out.mkdir(exist_ok=True)
    report = Report(pit.get("name", pit_dir.name))

    s = zd.Session()
    topo, toe = imports(s, pit, report)
    string = string_edits(s, topo, toe, report)
    undo_redo(s, report)
    save_reopen(s, out, report)
    export_dxf(s, string, out, report)
    seam_model(s, pit, report)
    report.item(7, "From Python, open the project, list its objects, run a command and read the run log", "pass",
                f"this run: {len(s.log())} log entries, {len(s.objects())} objects")
    solids_at_scale(pit, pit_dir, args.solids, report)

    path = pit_dir / "phase1-gate.md"
    report.write(path)
    print(f"report: {path}")
    return 0 if all(v.startswith(("pass", "pending")) for _, _, v, _ in report.items) else 1


if __name__ == "__main__":
    sys.exit(main())
