"""A reserves case kept at a step, timed (the Build Plan's "Reserves case plan", decision 9, workstream 3).

Opens a project, keeps one reserves table's case at a step (`rom_loss` by default), then edits the coal roof loss the
way a planner types it (0.15 m, 0.16 m, 0.15 m ...), each edit not saved, and times each rerun from the first step that
reads it; then runs on to `final`. Decision 9 picks from this: the whole case reruns as you type if a rerun from a step
takes under a second and the kept columns fit beside the project; else the block as you type and the case on a press.

    python case_at_step.py "<project>.zdp" [--table NAME] [--step rom_loss] [--counts-only]

With no reserves table in the project, one is made first: the starter dataflow from the project's seam model, its
settings the workflow demo's (made up, a planner's usual numbers), run over every solid carrying a horizon, material,
strip and block. `--counts-only` prints counts, milliseconds and memory and nothing else: use it on client data
(CLAUDE.md, "Client data stays local").
"""

import argparse
import csv
import ctypes
import io
import sys
import time

import zd

# The workflow demo's coal settings (testdata/synthetic/workflow_demo.py): made up, not any site's.
COAL = {"mad": 3.5, "mis": 8.0, "mrom": 10.0, "min_parting": 0.5, "ws_strip_ratio": 1.0, "min_coal_thickness": 0.3, "insitu_ash_cutoff": 35.0,
        "roof_loss": 0.1, "floor_loss": 0.1, "edge_loss": 0.05, "other_loss": 1.0,
        "roof_dilution": 0.05, "floor_dilution": 0.05, "edge_dilution": 0.02, "other_dilution": 0.5, "rom_ash_cutoff": 30.0}
WASTE = {"rd_default": 2.3, "as_default": 85.0, "ts_default": 0.3}


def memory_mb():
    """This process's working set and its peak, in MB (Windows); none elsewhere."""
    if sys.platform != "win32":
        return None, None

    class Counters(ctypes.Structure):
        _fields_ = [("cb", ctypes.c_ulong), ("PageFaultCount", ctypes.c_ulong), ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
                    ("QuotaPeakPagedPoolUsage", ctypes.c_size_t), ("QuotaPagedPoolUsage", ctypes.c_size_t), ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaNonPagedPoolUsage", ctypes.c_size_t), ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t)]

    c = Counters()
    c.cb = ctypes.sizeof(c)
    kernel32 = ctypes.windll.kernel32
    kernel32.GetCurrentProcess.restype = ctypes.c_void_p
    kernel32.K32GetProcessMemoryInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(Counters), ctypes.c_ulong]
    if not kernel32.K32GetProcessMemoryInfo(kernel32.GetCurrentProcess(), ctypes.byref(c), c.cb):
        return None, None
    return c.WorkingSetSize / 1e6, c.PeakWorkingSetSize / 1e6


def reserves_tables(s):
    """The project's tables a reserves run made, as their setups say."""
    found = []
    for o in s.objects(kind="table"):
        try:
            setup = s.table_setup(o["id"])
        except zd.Error:
            continue
        if setup and setup.get("dataflow"):
            found.append(o)
    return found


def make_table(s):
    """The starter dataflow from the seam model, the demo's settings, run over the blocks: a reserves table."""
    models = s.objects(kind="seam_model")
    if not models:
        raise SystemExit("no reserves table and no seam model to make one from")
    made = s.dataflow_starter(models[0]["id"], "Case at a step")
    table = s.object(made["parameters"])["parameters"]
    for row in table["rows"]:
        material = row.get("when", {}).get("material")
        row["values"].update(COAL if material == "coal" else WASTE if material == "waste" else {})
    s.run("parameters.save", table=made["parameters"], name="Case at a step assumptions", content=table)
    blocks, coal, unmeasured = [], [], []
    for o in s.objects(kind="solid"):
        a = s.object(o["id"]).get("attributes", {})
        if all(k in a for k in ("horizon", "material", "strip", "block")):
            blocks.append(o["id"])
            if "volume" not in a:
                unmeasured.append(o["id"])
            if a.get("material") == "coal" and "rd" not in a:
                coal.append(o["id"])
    # Measured and interrogated first where they aren't, as the demo's own steps do.
    if unmeasured:
        s.run("solid.measure", solids=unmeasured)
    if coal:
        s.run("solid.interrogate", solids=coal, model=models[0]["id"])
    record = s.run("reserves.run", dataflow=made["dataflow"], blocks=blocks, model=models[0]["id"], layer="case at a step")
    if not record["outputs"]:
        raise SystemExit(f"the reserves run made no table: {len(blocks)} blocks, none ran")
    return record["outputs"][0]["id"], len(blocks)


def with_roof(text, roof):
    """A settings table's CSV with every roof_loss cell given set to `roof`."""
    rows = list(csv.reader(io.StringIO(text)))
    at = [i for i, h in enumerate(rows[0]) if h.split(" (")[0].strip().lower() == "roof_loss"]
    for row in rows[1:]:
        for i in at:
            if i < len(row) and row[i].strip():
                row[i] = str(roof)
    out = io.StringIO()
    csv.writer(out, lineterminator="\n").writerows(rows)
    return out.getvalue()


def roof_table(s, table):
    """The first settings table the case's dataflow reads that gives roof_loss, by id, and its CSV."""
    setup = s.table_setup(table)
    ids = setup.get("tables") or s.object(setup["dataflow"])["dataflow"].get("parameters", [])
    for t in ids:
        text = s.object(t)["csv"]
        if "roof_loss" in text.splitlines()[0]:
            return t, text
    raise SystemExit("no settings table the dataflow reads gives roof_loss")


def written(totals, folder):
    """A folder's volume and tonnes as the steps so far have written them."""
    group = next((g for g in totals["written"] if g["group"] == folder), None)
    return (group["values"].get("volume"), group["tonnes"]) if group else (None, None)


def said(totals, what):
    return next(((x["units"], x["volume"]) for x in totals["said"] if x["what"] == what), (0, 0.0))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("project")
    parser.add_argument("--table")
    parser.add_argument("--step", default="rom_loss")
    parser.add_argument("--counts-only", action="store_true")
    args = parser.parse_args()
    quiet = args.counts_only

    started = time.perf_counter()
    s = zd.Session.open(args.project)
    opened_ms = (time.perf_counter() - started) * 1000
    project_mb, _ = memory_mb()
    print(f"project open: {opened_ms:.0f} ms, {project_mb:.0f} MB working set" if project_mb else f"project open: {opened_ms:.0f} ms")

    tables = reserves_tables(s)
    if args.table:
        tables = [t for t in tables if t["name"] == args.table]
    if tables:
        table = tables[0]["id"]
    else:
        started = time.perf_counter()
        table, n = make_table(s)
        print(f"reserves table made: {n} blocks, {(time.perf_counter() - started) * 1000:.0f} ms")

    started = time.perf_counter()
    totals = s.step_case(table, args.step)
    open_ms = (time.perf_counter() - started) * 1000
    units = sum(g["count"] for g in totals["units"])
    print(f"opened at {totals['name']} (step {totals['step']} of {len(totals['steps'])}): {totals['columns']} columns, {len(totals['stopped'])} stopped, "
          f"{units} units, {open_ms:.0f} ms; balanced {totals['balance']['balanced']} (worst {totals['balance']['worst']:.1e})")
    print("   " + ", ".join(f"{r['name']} {r['ms']:.0f}" for r in totals["ran"]) + " ms")
    now_mb, peak_mb = memory_mb()
    if now_mb:
        print(f"kept: {totals['kept_mb']:.0f} MB by count; working set {now_mb:.0f} MB ({now_mb - project_mb:+.0f} MB on the project's {project_mb:.0f}), peak {peak_mb:.0f} MB")
    if not quiet:
        print(f"   roof loss {said(totals, 'roof loss')[1]:,.0f} m3")

    settings, text = roof_table(s, table)
    times = []
    for roof in (0.15, 0.16, 0.15, 0.16, 0.15):
        started = time.perf_counter()
        totals = s.step_case(table, args.step, tables={settings: with_roof(text, roof)})
        ms = (time.perf_counter() - started) * 1000
        times.append(ms)
        ran = ", ".join(f"{r['name']} {r['ms']:.0f}" for r in totals["ran"])
        line = f"roof loss {roof}: from step {totals['ran_from']}, {ms:.0f} ms in all ({ran} ms); balanced {totals['balance']['balanced']}"
        if not quiet:
            line += f"; roof loss {said(totals, 'roof loss')[1]:,.0f} m3"
        print(line)
    times.sort()
    print(f"rerun from {args.step}: median {times[len(times) // 2]:.0f} ms, worst {times[-1]:.0f} ms (decision 9's bar: 1,000 ms)")

    started = time.perf_counter()
    totals = s.step_case(table, "final", tables={settings: with_roof(text, 0.15)})
    print(f"on to final: {(time.perf_counter() - started) * 1000:.0f} ms ({', '.join(f'{r['name']} {r['ms']:.0f}' for r in totals['ran'])} ms); balanced {totals['balance']['balanced']}")
    if not quiet:
        volume, tonnes = written(totals, "ROM\\Coal")
        print(f"   ROM coal at 0.15 m: {volume:,.1f} m3, {tonnes:,.1f} t")
    now_mb, peak_mb = memory_mb()
    if now_mb:
        print(f"working set at the end {now_mb:.0f} MB, peak {peak_mb:.0f} MB; kept {totals['kept_mb']:.0f} MB by count")


if __name__ == "__main__":
    if "--counts-only" in sys.argv:
        # On client data an error's words may name blocks: say only what kind it was.
        try:
            main()
        except (zd.Error, KeyError, IndexError, ValueError) as e:
            raise SystemExit(f"stopped: {type(e).__name__}") from None
    else:
        main()
