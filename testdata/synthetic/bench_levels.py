"""Benches on the made-up demo pit (decision 0071): the workflow demo's pit taken to its seam pieces, then benched and cut
into blocks in both orders Tim asked about, timed, and checked to give the same ground.

- seams then benches: the strips cut by the seam model, the seam pieces cut at bench levels, then into blocks;
- benches then seams: the strips cut at bench levels, the benches cut by the seam model, then into blocks.

Every piece is benched from RL -300 to 500 every 10 m. Prints counts, volumes and timings only.

    .venv\\Scripts\\python testdata/synthetic/bench_levels.py [folder]
"""

import pathlib
import sys
import tempfile
import time

import zd

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from ramp_template import made  # noqa: E402
from reserves_workflow import import_grids, write_grids  # noqa: E402
from workflow_demo import drawn_by_hand, the_steps  # noqa: E402

RULES = [{"from": -300.0, "to": 500.0, "every": 10.0}]


def timed(s, label, command, **params):
    started = time.perf_counter()
    record = s.run(command, params)
    seconds = time.perf_counter() - started
    out = made(record)
    print(f"   {label}: {len(out)} solids in {seconds * 1000:.0f} ms")
    return out, seconds


def volume(s, ids):
    return sum(s.object(i)["volume"] for i in ids)


def key(s, i):
    a = s.object(i)["attributes"]
    return (a.get("strip"), a.get("block"), a.get("horizon"), a.get("bench"))


def main():
    folder = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(tempfile.mkdtemp(prefix="zd-bench-levels-"))
    folder.mkdir(parents=True, exist_ok=True)
    s = zd.Session()
    g, model = import_grids(s, folder, write_grids(folder))
    toe, limit, first, ramps = drawn_by_hand(s)
    the_steps(s, g, model, toe, limit, first, ramps)
    strips = [o["id"] for o in s.objects(layer="4 strips", kind="solid")]
    pieces = [o["id"] for o in s.objects(layer="5 seam pieces", kind="solid")]
    polygons = [o["id"] for o in s.objects(layer="7 block polygons", kind="string")]
    print(f"The demo pit: {len(strips)} strips, {volume(s, strips):,.3f} m3; {len(pieces)} seam pieces; {len(polygons)} block polygons")

    print("Without benches:")
    plain, t_plain = timed(s, "blocks from the seam pieces", "solid.cut_by_polygons", solids=pieces, polygons=polygons, layer="blocks plain")

    print("Seams then benches:")
    pieces_a, t_a0 = timed(s, "strips cut by seams", "solid.cut_by_seams", solids=strips, model=model, layer="pieces A")
    benched_a, t_a1 = timed(s, "seam pieces benched", "solid.cut_by_benches", solids=pieces_a, layer="benches A", rules=RULES)
    blocks_a, t_a2 = timed(s, "benches into blocks", "solid.cut_by_polygons", solids=benched_a, polygons=polygons, layer="blocks A")

    print("Benches then seams:")
    benched_b, t_b1 = timed(s, "strips benched", "solid.cut_by_benches", solids=strips, layer="benches B", rules=RULES)
    pieces_b, t_b2 = timed(s, "benches cut by seams", "solid.cut_by_seams", solids=benched_b, model=model, layer="pieces B")
    blocks_b, t_b3 = timed(s, "pieces into blocks", "solid.cut_by_polygons", solids=pieces_b, polygons=polygons, layer="blocks B")

    va, vb, vp = volume(s, blocks_a), volume(s, blocks_b), volume(s, plain)
    print(f"Seams then benches: {len(blocks_a)} blocks, {va:,.3f} m3, seams, benching and blocks {t_a0 + t_a1 + t_a2:.2f} s")
    print(f"Benches then seams: {len(blocks_b)} blocks, {vb:,.3f} m3, benching, seams and blocks {t_b1 + t_b2 + t_b3:.2f} s")
    print(f"Without benches: {len(plain)} blocks, {vp:,.3f} m3, blocks {t_plain:.2f} s")
    print(f"Blocks differ from the unbenched blocks by {va - vp:+.4f} m3 (seams then benches) and {vb - vp:+.4f} m3 (benches then seams): flaps at the levels")

    # The same ground block by block, bench by bench: each (strip, block, horizon, bench) in both orders.
    by_a, by_b = {}, {}
    for ids, into in ((blocks_a, by_a), (blocks_b, by_b)):
        for i in ids:
            k = key(s, i)
            into[k] = into.get(k, 0.0) + s.object(i)["volume"]
    shared = set(by_a) & set(by_b)
    worst = max((abs(by_a[k] - by_b[k]) for k in shared), default=0.0)
    print(f"{len(shared)} strip/block/horizon/bench units in both orders ({len(by_a)} and {len(by_b)}); the most any differs: {worst:.4f} m3")
    print(f"Only seams then benches: {len(set(by_a) - shared)} units, {sum(by_a[k] for k in set(by_a) - shared):.4f} m3; "
          f"only benches then seams: {len(set(by_b) - shared)} units, {sum(by_b[k] for k in set(by_b) - shared):.4f} m3")


if __name__ == "__main__":
    main()
