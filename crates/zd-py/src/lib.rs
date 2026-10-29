//! Python bindings for the Zovlyn Pit Design Engine (decision 0012): `import zd`.
//!
//! A thin layer over [`zd_commands::Session`]: open or start a project, list its objects, run any
//! command by name, read the run log, undo, redo and save. Values cross as plain Python dicts and
//! lists, converted through JSON (Python's `json` module on one side, `serde_json` on the other), so
//! coordinates keep every bit: both sides write the shortest text that reads back to the same f64.
//!
//! ```python
//! import zd
//! s = zd.open("pit.zdproj")
//! for o in s.objects(kind="solid"):
//!     print(o["name"], s.object(o["id"])["volume"])
//! record = s.run("surface.import", path="topo.00t", layer="topo")
//! print(s.log()[-1]["messages"])
//! ```

use std::path::PathBuf;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use zd_commands::Session;
use zd_core::{Body, Object, ObjectId, Project};

create_exception!(zd, Error, PyException, "Something the engine refused or could not do.");
create_exception!(zd, CommandError, Error, "A command was refused or failed. The failed run is in the run log.");
create_exception!(zd, ProjectError, Error, "A project could not be opened or saved.");

/// A project, the engine's commands, and the log of what they did.
#[pyclass(module = "zd", name = "Session")]
struct PySession {
    inner: Session,
}

#[pymethods]
impl PySession {
    /// A new, empty project.
    #[new]
    fn new() -> Self {
        PySession { inner: Session::with_builtins() }
    }

    /// Opens a `<name>.zdproj` folder. The run log carries on from where it was saved.
    #[staticmethod]
    fn open(py: Python<'_>, path: PathBuf) -> PyResult<Self> {
        let inner = py.detach(|| Session::open(&path)).map_err(|e| ProjectError::new_err(format!("{}: {e}", path.display())))?;
        Ok(PySession { inner })
    }

    /// Saves to a `<name>.zdproj` folder; saving again to the same folder rewrites only what
    /// changed. Returns what the save wrote.
    fn save(&mut self, py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
        let report = py.detach(|| self.inner.save(&path)).map_err(|e| ProjectError::new_err(format!("{}: {e}", path.display())))?;
        to_py(
            py,
            &json!({
                "object_files_written": report.object_files_written,
                "object_files_reused": report.object_files_reused,
                "files_deleted": report.files_deleted,
                "bytes_written": report.bytes_written,
                "ms": report.ms,
            }),
        )
    }

    /// The live objects, optionally of one kind (`string`, `surface`, `solid`, `grid`,
    /// `seam_model`) or on one layer: each a dict of id, name, layer, kind, revision and attributes.
    #[pyo3(signature = (kind=None, layer=None))]
    fn objects(&self, py: Python<'_>, kind: Option<&str>, layer: Option<&str>) -> PyResult<Py<PyAny>> {
        let project = self.inner.project();
        let listed: Vec<Value> = project
            .objects()
            .map(|o| summary(project, o))
            .filter(|v| kind.is_none_or(|k| v["kind"] == k) && layer.is_none_or(|l| v["layer"] == l))
            .collect();
        to_py(py, &Value::Array(listed))
    }

    /// One object with its measurements: a string's points, lengths and area; a surface's counts,
    /// area, validity flags and crossing check; a solid's too, with its volume; a grid's lattice; a
    /// seam model's horizons and issues.
    fn object(&self, py: Python<'_>, id: &str) -> PyResult<Py<PyAny>> {
        let project = self.inner.project();
        let object = project.object(object_id(id)?).ok_or_else(|| PyValueError::new_err(format!("no live object {id}")))?;
        let mut value = summary(project, object);
        if let (Value::Object(fields), Value::Object(measured)) = (&mut value, measures(&object.body)) {
            fields.extend(measured);
        }
        to_py(py, &value)
    }

    /// A surface's or solid's mesh: `vertices` as `[x, y, z]` and `triangles` as vertex numbers from
    /// 0, counter-clockwise about the outward normal.
    fn mesh(&self, py: Python<'_>, id: &str) -> PyResult<Py<PyAny>> {
        let object = self.inner.project().object(object_id(id)?).ok_or_else(|| PyValueError::new_err(format!("no live object {id}")))?;
        let mesh = match &object.body {
            Body::Surface(s) => s.mesh(),
            Body::Solid(s) => s.mesh(),
            other => return Err(PyValueError::new_err(format!("'{}' is a {:?}, not a surface or solid", object.name, other.kind()))),
        };
        to_py(py, &json!({ "vertices": mesh.vertices, "triangles": mesh.triangles }))
    }

    /// The layers: id, name, colour and whether shown.
    fn layers(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_py(py, &json!(self.inner.project().layers()))
    }

    /// Runs a command by name, its parameters given as a dict, as keywords, or both (keywords win;
    /// a dict also takes names Python reserves, such as `from`). Returns the run record. A refused
    /// or failed run raises `zd.CommandError` and is logged all the same. Other Python threads run
    /// while the command does.
    #[pyo3(signature = (command, params=None, **keywords))]
    fn run(&mut self, py: Python<'_>, command: &str, params: Option<&Bound<'_, PyDict>>, keywords: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
        let merged = PyDict::new(py);
        for given in [params, keywords].into_iter().flatten() {
            merged.update(given.as_mapping())?;
        }
        let value = from_py(merged.as_any())?;
        let command = command.to_string();
        let record = py.detach(|| self.inner.run(&command, value)).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!(record))
    }

    /// Every run, undo, redo, save and open, oldest first, as saved in the project.
    fn log(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_py(py, &json!(self.inner.log()))
    }

    /// Every command: its name, summary, inputs, parameters and outputs.
    fn commands(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_py(py, &json!(self.inner.registry().specs()))
    }

    /// Takes the last run off the project; returns its id, or None if there is nothing to undo.
    fn undo(&mut self) -> Option<String> {
        self.inner.undo().map(|run| json!(run).as_str().unwrap_or_default().to_string())
    }

    /// Puts the last undone run back; returns its id, or None if there is nothing to redo.
    fn redo(&mut self) -> Option<String> {
        self.inner.redo().map(|run| json!(run).as_str().unwrap_or_default().to_string())
    }

    #[getter]
    fn can_undo(&self) -> bool {
        self.inner.project().can_undo()
    }

    #[getter]
    fn can_redo(&self) -> bool {
        self.inner.project().can_redo()
    }

    fn __repr__(&self) -> String {
        let project = self.inner.project();
        format!("<zd.Session: {} objects on {} layers>", project.objects().count(), project.layers().len())
    }
}

/// Opens a `<name>.zdproj` folder: the same as `zd.Session.open(path)`.
#[pyfunction]
fn open(py: Python<'_>, path: PathBuf) -> PyResult<PySession> {
    PySession::open(py, path)
}

/// Python access to the Zovlyn Pit Design Engine.
#[pymodule]
fn zd(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySession>()?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    let py = m.py();
    m.add("Error", py.get_type::<Error>())?;
    m.add("CommandError", py.get_type::<CommandError>())?;
    m.add("ProjectError", py.get_type::<ProjectError>())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

fn object_id(id: &str) -> PyResult<ObjectId> {
    serde_json::from_value(Value::String(id.to_string())).map_err(|_| PyValueError::new_err(format!("'{id}' is not an object id")))
}

fn summary(project: &Project, object: &Object) -> Value {
    let layer = project.layer(object.layer).map(|l| l.name.clone()).unwrap_or_default();
    json!({
        "id": object.id,
        "name": object.name,
        "layer": layer,
        "kind": object.kind(),
        "revision": object.revision,
        "attributes": object.attributes,
    })
}

/// The measurements Properties shows in the app.
fn measures(body: &Body) -> Value {
    match body {
        Body::String(line) => json!({
            "points": line.vertices,
            "closed": line.closed,
            "length": line.length(),
            "plan_length": line.plan_length(),
            "plan_area": line.plan_area(),
        }),
        Body::Surface(s) => json!({
            "vertices": s.mesh().vertices.len(),
            "triangles": s.mesh().triangle_count(),
            "area": s.mesh().area(),
            "validity": s.validity(),
            "crossings": s.crossings(),
        }),
        Body::Solid(s) => json!({
            "vertices": s.mesh().vertices.len(),
            "triangles": s.mesh().triangle_count(),
            "area": s.mesh().area(),
            "volume": s.volume(),
            "validity": s.validity(),
            "crossings": s.crossings(),
        }),
        Body::Grid(g) => json!({
            "origin": g.origin,
            "cell_size": g.cell_size,
            "cols": g.cols,
            "rows": g.rows,
            "nulls": g.null_count(),
            "z_range": g.z_range(),
        }),
        Body::SeamModel(m) => json!({ "model": **m }),
    }
}

/// A JSON value as Python objects, through Python's `json` module.
fn to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    let text = serde_json::to_string(value).map_err(|e| Error::new_err(e.to_string()))?;
    Ok(py.import("json")?.call_method1("loads", (text,))?.unbind())
}

/// Python objects as a JSON value. Paths (`pathlib.Path`) become their text; anything else JSON
/// cannot hold is a `TypeError`.
fn from_py(object: &Bound<'_, PyAny>) -> PyResult<Value> {
    let py = object.py();
    let options = PyDict::new(py);
    options.set_item("default", py.import("os")?.getattr("fspath")?)?;
    options.set_item("allow_nan", false)?;
    let text: String = py.import("json")?.call_method("dumps", (object,), Some(&options))?.extract()?;
    serde_json::from_str(&text).map_err(|e| PyValueError::new_err(format!("parameters: {e}")))
}
