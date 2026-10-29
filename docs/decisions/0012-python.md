# 0012 — Python access (`import zd`)

Date: 2026-10-29 · Status: accepted (Tim, 2026-10-29)

## Decision

Workstream 8 meets the last Phase 1 gate item: from Python, open the project, list its objects, run a
command and read the run log. `crates/zd-py` wraps the engine's `Session` with PyO3 (0.30) and maturin
builds it, both in the locked stack (decision 0004). Nothing new is added to the geometry: Python is
another way into the same commands, project files and run log as the app.

1. **One module, `zd`.** `zd.Session()` starts a project, `zd.open(path)` opens a `.zdproj` folder,
   `s.save(path)` saves it.
2. **The same calls the app makes.** `s.objects(kind=None, layer=None)` lists objects (id, name,
   layer, kind, revision, attributes); `s.object(id)` adds what Properties shows (a string's points
   and lengths; a surface's counts, area, validity flags and crossing check; a solid's volume; a
   grid's lattice; a seam model); `s.mesh(id)` gives a surface's or solid's vertices and triangles;
   `s.run(command, params, **keywords)` runs any command and returns its run record; `s.log()`,
   `s.commands()`, `s.layers()`, `s.undo()`, `s.redo()`. A refused run raises `zd.CommandError`
   (and is logged as failed); a project that cannot be opened or saved raises `zd.ProjectError`;
   both are `zd.Error`. Ids are strings, as in the saved project.
3. **Plain Python values, no NumPy yet.** Values cross as dicts and lists, through Python's `json`
   module and `serde_json`, so there is no new dependency, and coordinates keep every bit: both sides
   write the shortest text that reads back to the same f64. Paths may be `pathlib.Path`. NumPy arrays
   would need the `numpy` crate, outside the locked stack; that waits for a script that needs it.
4. **One wheel for CPython 3.10 and later** (the stable ABI). maturin (1.9.4 or later) tells PyO3 it
   is building an extension module, so `cargo test --workspace` still links. Commands run with other
   Python threads free, so a notebook stays responsive.
5. **CI**: a job on Windows and Linux builds the module with `maturin develop` in a fresh virtual
   environment and runs `crates/zd-py/tests/test_session.py`, plain asserts with no pytest. The engine
   job leaves `zd-py` out, since linking Python on Linux needs development libraries the runner may
   not have.

## Use

```python
import zd

s = zd.open(r"D:\Projects\pit.zdproj")
for o in s.objects(kind="solid"):
    print(o["name"], s.object(o["id"])["volume"])

record = s.run("surface.import", path=r"D:\data\topo.00t", layer="topo")
print(record["messages"])                # what the reader and the repair step did
s.run("string.set_grade", {"string": toe_id, "from": 0}, grade=10.0)   # 'from' is reserved in Python
s.save(r"D:\Projects\pit.zdproj")
```

Building it, into a project virtual environment:

```powershell
py -m venv .venv; .venv\Scripts\activate; python -m pip install "maturin>=1.9.4,<2"
maturin develop --release -m crates/zd-py/Cargo.toml
python crates/zd-py/tests/test_session.py
```

## Evidence

`crates/zd-py/tests/test_session.py`: a project with a graded toe, its offset crest and a box solid is
saved; opened again from Python it lists three objects on three layers, filters by kind and layer,
returns the toe's points bit for bit and the box's 1,000 m³ with its validity and crossing check;
a command run from Python revises the toe and lands in the log after the saved and opened entries; a
refused command raises `zd.CommandError` and is logged as failed; undo and redo return the run's id;
a bad id, a missing project and a NaN coordinate are Python errors. It passes against the module
built locally and in CI.
