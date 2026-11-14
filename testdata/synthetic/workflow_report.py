"""The workflow report (decision 0068) on the made-up demo pit: the demo built and run as Base and 50m (workflow_demo.py),
opened again from its file, and its report written beside it. Checks each case's blocks plus what was left out against
the pit, and that both cases hold the same coal and waste; prints counts, totals and timings.

    python testdata/synthetic/workflow_report.py [folder]
"""

import pathlib
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import zd  # noqa: E402
import workflow_demo  # noqa: E402


def main():
    folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else HERE / "reserves" / "workflow report"
    sys.argv = [sys.argv[0], str(folder)]
    workflow_demo.main()
    s = zd.Session.open(folder / "Workflow demo.zdp")
    workflow = next(o["id"] for o in s.objects(kind="workflow"))
    offset = time.localtime().tm_gmtoff // 60

    started = time.perf_counter()
    report = s.workflow_report(workflow, utc_offset_minutes=offset)
    figures_ms = (time.perf_counter() - started) * 1e3
    started = time.perf_counter()
    path = s.write_workflow_report(workflow, utc_offset_minutes=offset)
    page_ms = (time.perf_counter() - started) * 1e3
    print(f"Report figures in {figures_ms:.0f} ms, page in {page_ms:.0f} ms: {path} ({pathlib.Path(path).stat().st_size:,} bytes)")

    for case in report["cases"]:
        pit = case["pit"]
        left = sum(l["volume"] for l in case["left_out"])
        print(f"   {case['name']}: {len(case['blocks'])} blocks, coal {case['coal']['tonnes']:,.0f} t in {case['coal']['volume']:,.0f} m3 "
              f"(RD {case['coal']['rd']:.3f}), waste {case['waste']['volume']:,.0f} bcm, SR {case['strip_ratio']:.2f}; "
              f"left out {left:,.3f} m3; pit {pit['volume']:,.3f} m3, difference {pit['difference']:.3g} m3, within: {pit['within']}; "
              f"{len(case['warnings'])} warnings")
        assert pit["within"], case["name"]
    base, half = report["cases"]
    for key in ("coal", "waste"):
        assert abs(base[key]["volume"] - half[key]["volume"]) <= 1e-9 * base[key]["volume"], key
    assert len(half["blocks"]) > len(base["blocks"])
    page = pathlib.Path(path).read_text(encoding="utf-8")
    assert "http://" not in page and "https://" not in page
    print("   both cases hold the same coal and waste; the page fetches nothing")


if __name__ == "__main__":
    main()
