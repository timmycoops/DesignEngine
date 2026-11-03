"""Design examples in a Deswik drawing (decision 0036): every string that carries its projection
settings, read through `deswik.import` and listed by layer with the settings in our terms.

Deswik writes on each string it projects how it was made, e.g. "Inside/Right at 45.Up to level 140"
or "Outside/Left at 37. Up by 200 to surface", and on a projection to a surface which surface. This
lists them so a clean case can be picked to run through our walls beside Deswik's own result.

The drawing is client data, so the report goes under testdata/golden (kept out of git) unless told
otherwise:

    .venv\\Scripts\\python bench\\deswik_examples.py <drawing.dcf> [--layers "A\\B;C"] [--out <folder>]

Writes `examples.csv` (one row per string with projection settings) and `summary.md`.
"""

import argparse
import csv
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path

import zd

# "<Inside/Right|Outside/Left> at <angle>.<Up|Down> <to level RL | by D [to surface] | to surface>"
DETAILS = re.compile(
    r"^\s*(?P<side>Inside/Right|Outside/Left)\s+at\s+(?P<angle>-?\d+(?:\.\d+)?)\.\s*(?P<way>Up|Down)\s*"
    r"(?:to level\s+(?P<level>-?\d+(?:\.\d+)?)|by\s+(?P<by>-?\d+(?:\.\d+)?)(?P<surface>\s+to surface)?|(?P<only>to surface))\s*$",
    re.IGNORECASE,
)


def parse(details):
    """The settings in our terms, or None where the wording is not one seen before."""
    m = DETAILS.match(details)
    if not m:
        return None
    side = "right" if m["side"].lower().startswith("inside") else "left"
    if m["level"] is not None:
        to, value = "rl", float(m["level"])
    elif m["by"] is not None:
        to, value = ("surface, at most" if m["surface"] else "distance"), float(m["by"])
    else:
        to, value = "surface", None
    return {"side": side, "batter_deg": float(m["angle"]), "way": m["way"].lower(), "to": to, "value": value}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("dcf")
    ap.add_argument("--layers", default="", help="layers to read, ';' between them, as Deswik names them; all by default")
    ap.add_argument("--out", default=None, help="folder for the report; testdata/golden/deswik-examples/<drawing> by default")
    args = ap.parse_args()

    dcf = Path(args.dcf)
    out = Path(args.out) if args.out else Path(__file__).resolve().parent.parent / "testdata" / "golden" / "deswik-examples" / dcf.stem
    out.mkdir(parents=True, exist_ok=True)
    layers = [l.strip() for l in args.layers.split(";") if l.strip()]

    s = zd.Session()
    record = s.run("deswik.import", path=str(dcf), layers=layers, meshes="none")
    print(record["messages"][0])

    rows, unknown = [], Counter()
    for o in s.objects(kind="string"):
        details = o["attributes"].get("Projection Details")
        if not details:
            continue
        detail = s.object(o["id"])
        lo, hi = detail["bounds"]
        parsed = parse(details)
        if parsed is None:
            unknown[details] += 1
            parsed = {"side": "", "batter_deg": "", "way": "", "to": "", "value": ""}
        rows.append(
            {
                "layer": o["layer"],
                "name": o["name"],
                "vertices": len(detail["points"]),
                "closed": detail["closed"],
                "rl_low": round(lo[2], 3),
                "rl_high": round(hi[2], 3),
                **parsed,
                "projection_surface": o["attributes"].get("Projection Surface", ""),
                "line_name": o["attributes"].get("LineName", ""),
                "details": details,
            }
        )

    with open(out / "examples.csv", "w", newline="", encoding="utf-8") as f:
        fields = ["layer", "name", "vertices", "closed", "rl_low", "rl_high", "side", "batter_deg", "way", "to", "value", "projection_surface", "line_name", "details"]
        w = csv.DictWriter(f, fieldnames=fields)
        w.writeheader()
        w.writerows(rows)

    by_layer = defaultdict(list)
    for r in rows:
        by_layer[r["layer"]].append(r)
    kinds = Counter((r["way"], r["to"], r["batter_deg"]) for r in rows if r["to"])
    strings = len(s.objects(kind="string"))
    lines = [
        f"# Design examples in {dcf.name}",
        "",
        f"{strings} strings read, {len(rows)} carrying projection settings, on {len(by_layer)} layers.",
        "",
        "## Settings, most used first",
        "",
        "| Way | To | Batter (deg) | Strings |",
        "|---|---|---|---|",
        *[f"| {way} | {to} | {batter} | {n} |" for (way, to, batter), n in kinds.most_common(30)],
        "",
        "## Layers with the most projected strings",
        "",
        "| Layer | Strings | Settings used |",
        "|---|---|---|",
    ]
    for layer, rs in sorted(by_layer.items(), key=lambda kv: -len(kv[1]))[:40]:
        used = Counter(r["details"] for r in rs)
        lines.append(f"| {layer} | {len(rs)} | {'; '.join(f'{d} ({n})' for d, n in used.most_common(4))} |")
    if unknown:
        lines += ["", "## Wording not read", "", *[f"- {d} ({n})" for d, n in unknown.most_common()]]
    (out / "summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"{len(rows)} strings with projection settings on {len(by_layer)} layers; {sum(unknown.values())} with wording not read")
    print(f"wrote {out / 'examples.csv'} and {out / 'summary.md'}")


if __name__ == "__main__":
    sys.exit(main())
