//! Python bindings for the Zovlyn Pit Design Engine (decision 0012): `import zd`.
//!
//! A thin layer over [`zd_commands::Session`]: open or start a project, list its objects, run any
//! command by name, read the run log, undo, redo and save. Values cross as plain Python dicts and
//! lists, converted through JSON (Python's `json` module on one side, `serde_json` on the other), so
//! coordinates keep every bit: both sides write the shortest text that reads back to the same f64.
//!
//! ```python
//! import zd
//! s = zd.open("pit.zdp")
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
    /// Each reserves table's case kept at a step (Reserves case plan, decision 9), by table.
    steps: std::collections::HashMap<ObjectId, zd_commands::builtin::CaseAtStep>,
}

#[pymethods]
impl PySession {
    /// A new, empty project.
    #[new]
    fn new() -> Self {
        PySession { inner: Session::with_builtins(), steps: Default::default() }
    }

    /// Opens a `.zdp` project file, or an older `<name>.zdproj` folder. The run log carries on from where it was saved.
    #[staticmethod]
    fn open(py: Python<'_>, path: PathBuf) -> PyResult<Self> {
        let inner = py.detach(|| Session::open(&path)).map_err(|e| ProjectError::new_err(format!("{}: {e}", path.display())))?;
        Ok(PySession { inner, steps: Default::default() })
    }

    /// Saves to a `.zdp` project file (a `.zdproj` path saves as a `.zdp` beside it); saving again re-encodes only what
    /// changed. Returns what the save wrote.
    fn save(&mut self, py: Python<'_>, path: PathBuf) -> PyResult<Py<PyAny>> {
        let report = py.detach(|| self.inner.save(&path)).map_err(|e| ProjectError::new_err(format!("{}: {e}", path.display())))?;
        to_py(
            py,
            &json!({
                "path": report.path.display().to_string(),
                "unchanged": report.unchanged,
                "entries": report.entries,
                "encoded": report.encoded,
                "bytes": report.bytes,
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
    /// seam model's horizons and issues. `bounds` is the lowest and highest corner of its geometry.
    fn object(&self, py: Python<'_>, id: &str) -> PyResult<Py<PyAny>> {
        let project = self.inner.project();
        let object = project.object(object_id(id)?).ok_or_else(|| PyValueError::new_err(format!("no live object {id}")))?;
        let mut value = summary(project, object);
        if let (Value::Object(fields), Value::Object(measured)) = (&mut value, measures(&object.body)) {
            fields.extend(measured);
            fields.insert("bounds".into(), json!(bounds(&object.body)));
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

    /// Captures runs from the run log as a new workflow (decision 0065), each run a step and each object id it was given
    /// a rule that finds the same objects again. `runs` are run ids from `log()`, still applied (not refused, not
    /// undone). Returns the `workflow.save` run record; the workflow is its output.
    fn capture_workflow(&mut self, py: Python<'_>, name: &str, runs: Vec<String>) -> PyResult<Py<PyAny>> {
        let runs = runs
            .iter()
            .map(|r| serde_json::from_value(Value::String(r.clone())).map_err(|_| PyValueError::new_err(format!("'{r}' is not a run id"))))
            .collect::<PyResult<Vec<zd_core::RunId>>>()?;
        let record = py.detach(|| self.inner.capture_workflow(name, &runs)).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!(record))
    }

    /// Runs a workflow from step `from_step` (from 1) to its end, a pause or the first refusal, as one undo step. What
    /// each step made last time is made again as the next revisions of the same objects. Returns the workflow run's
    /// record: `status` is `finished`, `paused` or `refused`, with `at` the step it stopped at, and `steps` says what
    /// each step did. A refusal is in the record, not raised.
    /// `case` runs one of the workflow's cases (decision 0066), on its own layers; Base if left out.
    #[pyo3(signature = (workflow, from_step=1, case=None))]
    fn run_workflow(&mut self, py: Python<'_>, workflow: &str, from_step: usize, case: Option<&str>) -> PyResult<Py<PyAny>> {
        let id = object_id(workflow)?;
        let case = case.unwrap_or(zd_core::BASE_CASE).to_string();
        let record = py.detach(|| self.inner.run_case(id, from_step, &case)).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!(record))
    }

    /// Runs every case of a workflow from its first step, Base first, as one undo step (decision 0066); a pause or a
    /// refusal stops it at that case. Returns each case's run record.
    fn run_all_cases(&mut self, py: Python<'_>, workflow: &str) -> PyResult<Py<PyAny>> {
        let id = object_id(workflow)?;
        let records = py.detach(|| self.inner.run_all_cases(id)).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!(records))
    }

    /// The workflow report's figures (decision 0068): every case from its latest run, its blocks by seam and material,
    /// what was left out and the reconciliation to the pit, the steps and the inputs. `utc_offset_minutes` dates it in
    /// local time (`time.localtime().tm_gmtoff // 60`); UTC if left out.
    /// Saves a starter parameter table and a dataflow reading it from a seam model (decision 0073): every step, a field
    /// for the density and each quality grid, and a table with every column the steps read. Two runs, each undone on its
    /// own. Gives back `{"parameters": id, "dataflow": id}`.
    #[pyo3(signature = (model, name=None))]
    fn dataflow_starter(&mut self, py: Python<'_>, model: &str, name: Option<&str>) -> PyResult<Py<PyAny>> {
        let id = object_id(model)?;
        let name = name.unwrap_or("Reserves").to_string();
        let (table, flow) = py.detach(|| self.inner.dataflow_starter(id, &name)).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!({ "parameters": table, "dataflow": flow }))
    }

    /// What fills a reserves table (Table setup plan, decision 1): the setup it holds, or for a table made before setups,
    /// the one the run that made it gives. Save a changed one with `run("table.setup", table=..., setup=...)`.
    fn table_setup(&self, py: Python<'_>, table: &str) -> PyResult<Py<PyAny>> {
        let id = object_id(table)?;
        let setup = zd_commands::builtin::setup_of(self.inner.project(), self.inner.log(), id).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &serde_json::to_value(setup).map_err(|e| CommandError::new_err(e.to_string()))?)
    }

    /// What reads a parameter table (Table setup plan, decision 4): `{"table", "readers": [{kind, name, id, how}],
    /// "columns": [{column, read_by, shadowed_by}]}`.
    fn read_by(&self, py: Python<'_>, table: &str) -> PyResult<Py<PyAny>> {
        let id = object_id(table)?;
        let read = zd_commands::builtin::read_by(self.inner.project(), id).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &serde_json::to_value(read).map_err(|e| CommandError::new_err(e.to_string()))?)
    }

    /// A reserves table's setup as the app's Setup tab shows it (Table setup plan, decision 5): its blocks, dataflows and
    /// steps, each field's source (`own` where the table gives its own), its parameter tables in order with the columns
    /// read from each, and any problems.
    fn setup_view(&self, py: Python<'_>, table: &str) -> PyResult<Py<PyAny>> {
        let id = object_id(table)?;
        let view = zd_commands::builtin::setup_view(self.inner.project(), self.inner.log(), id).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &serde_json::to_value(view).map_err(|e| CommandError::new_err(e.to_string()))?)
    }

    /// Runs a reserves table again from its setup (decisions 0085, and the Table setup plan's 1 and 6): every block of
    /// its case the setup picks, or those under `only` (`["Pit", "3", "B1"]`), into the table in place. The run's record.
    #[pyo3(signature = (table, only=None))]
    fn run_table(&mut self, py: Python<'_>, table: &str, only: Option<Vec<String>>) -> PyResult<Py<PyAny>> {
        let id = object_id(table)?;
        let p = zd_commands::builtin::rerun_params(self.inner.project(), self.inner.log(), id, &only.unwrap_or_default()).map_err(|e| CommandError::new_err(e.to_string()))?;
        let params = serde_json::to_value(&p).map_err(|e| CommandError::new_err(e.to_string()))?;
        let record = py.detach(|| self.inner.run("reserves.run", params)).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &serde_json::to_value(&record).map_err(|e| CommandError::new_err(e.to_string()))?)
    }

    /// Runs a case's dataflows (Reserves case plan, decisions 6 and 14): with `dataflow` that one alone, its reserves
    /// dataflow again from its setup (with `only` a block or strip) or one of its row dataflows on the table's rows;
    /// without, all of them in order, the reserves dataflow first. Running one never runs another. A list of the runs'
    /// records, one a dataflow, each the table's next revision.
    ///
    /// With `out_of_date`, only those out of date (decision 6), in order, each judged after the one before it ran, so a
    /// row dataflow below a reserves dataflow just run is run too.
    #[pyo3(signature = (table, dataflow=None, only=None, out_of_date=false))]
    fn run_case(&mut self, py: Python<'_>, table: &str, dataflow: Option<&str>, only: Option<Vec<String>>, out_of_date: bool) -> PyResult<Py<PyAny>> {
        let id = object_id(table)?;
        let setup = zd_commands::builtin::setup_of(self.inner.project(), self.inner.log(), id).map_err(|e| CommandError::new_err(e.to_string()))?;
        let which: Vec<zd_core::ObjectId> = match dataflow {
            Some(d) => vec![object_id(d)?],
            None => std::iter::once(setup.dataflow).chain(setup.row_dataflows.iter().map(|r| r.dataflow)).collect(),
        };
        let mut records = Vec::with_capacity(which.len());
        for flow in which {
            if out_of_date {
                let state = zd_commands::builtin::case_state(self.inner.project(), self.inner.log(), id).map_err(|e| CommandError::new_err(e.to_string()))?;
                if state.dataflows.iter().any(|d| d.dataflow == flow && d.up_to_date) {
                    continue;
                }
            }
            let record = if flow == setup.dataflow {
                let p = zd_commands::builtin::rerun_params(self.inner.project(), self.inner.log(), id, &only.clone().unwrap_or_default()).map_err(|e| CommandError::new_err(e.to_string()))?;
                let params = serde_json::to_value(&p).map_err(|e| CommandError::new_err(e.to_string()))?;
                py.detach(|| self.inner.run("reserves.run", params))
            } else {
                py.detach(|| self.inner.run("table.run_dataflow", json!({ "table": id, "dataflow": flow })))
            }
            .map_err(|e| CommandError::new_err(e.to_string()))?;
            records.push(serde_json::to_value(&record).map_err(|e| CommandError::new_err(e.to_string()))?);
        }
        to_py(py, &serde_json::Value::Array(records))
    }

    /// Each of a case's dataflows in run order, up to date or why not (Reserves case plan, decision 6): `{"table", "name",
    /// "dataflows": [{dataflow, name, reserves, range, run, ran, up_to_date, why}]}`, read from the run log.
    fn case_state(&self, py: Python<'_>, table: &str) -> PyResult<Py<PyAny>> {
        let id = object_id(table)?;
        let state = zd_commands::builtin::case_state(self.inner.project(), self.inner.log(), id).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &serde_json::to_value(state).map_err(|e| CommandError::new_err(e.to_string()))?)
    }

    /// A reserves table's case at a step (Reserves case plan, decisions 8 and 9): every column of its case run to `step`
    /// (a step's name, or its number, 0 for the blocks as read) and kept, on what's saved, with `tables` as edited
    /// (`{parameter table name or id: CSV text}`, read in place of those tables and never saved). Kept from the last call
    /// on the same table: an edit runs again from the first step that reads it, the steps before it reused, and a later
    /// step runs on from where it is. The case's totals at the step: `{step, name, steps, columns, stopped, fields, units,
    /// written, said, balance, ran_from, ran, kept_mb}`; units by material and what became of them, written by folder,
    /// each `{group, count, values}`.
    #[pyo3(signature = (table, step, tables=None))]
    fn step_case(&mut self, py: Python<'_>, table: &str, step: &Bound<'_, PyAny>, tables: Option<std::collections::BTreeMap<String, String>>) -> PyResult<Py<PyAny>> {
        let id = object_id(table)?;
        let project = self.inner.project();
        let unsaved = zd_commands::builtin::Unsaved { dataflow: None, tables: self.edited_tables(tables)? };
        let refused = |e: zd_commands::CommandError| CommandError::new_err(e.to_string());
        let mut case = match self.steps.remove(&id) {
            Some(mut case) => {
                case.rerun(project, self.inner.log(), &unsaved).map_err(refused)?;
                case
            }
            None => zd_commands::builtin::CaseAtStep::open(project, self.inner.log(), id, 0, &unsaved).map_err(refused)?,
        };
        let n = match step.extract::<usize>() {
            Ok(n) => n,
            Err(_) => case.step_number(&step.extract::<String>()?).map_err(refused)?,
        };
        case.go_to(n).map_err(refused)?;
        let totals = case.totals();
        self.steps.insert(id, case);
        to_py(py, &serde_json::to_value(totals).map_err(|e| CommandError::new_err(e.to_string()))?)
    }

    /// The units of a table's case at the step `step_case` keeps it at, a row a unit (Tables and scripts plan, decision
    /// 10): `{step, upper, columns, stopped, table: {rows, units, columns}}`.
    fn step_units(&self, py: Python<'_>, table: &str) -> PyResult<Py<PyAny>> {
        let case = self.kept_case(table)?;
        let units = case.units();
        to_py(py, &json!({ "step": units.step, "upper": units.upper, "columns": units.columns, "stopped": units.stopped, "table": table_json(&units.table) }))
    }

    /// The table a table's case kept by `step_case` would make from its step: run on to the dataflow's last step and
    /// written as a reserves run writes a new table, with the edits it was last given. Nothing is saved.
    fn step_table(&self, py: Python<'_>, table: &str) -> PyResult<Py<PyAny>> {
        let case = self.kept_case(table)?;
        let made = case.table(self.inner.project()).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &table_json(&made))
    }

    /// The flow of a table's case kept by `step_case` (Reserves case plan, decision 10): the case after every step of its
    /// dataflow, 0 for its blocks as read, its units by material and what became of them, `[{step, name, groups:
    /// [{material, became, detail, units, volume, tonnes}], stopped}]`. The steps after the one kept run on and aren't kept.
    fn step_flow(&mut self, py: Python<'_>, table: &str) -> PyResult<Py<PyAny>> {
        let id = object_id(table)?;
        let case = self.steps.get_mut(&id).ok_or_else(|| PyValueError::new_err(format!("no case kept for {table}: open it at a step with step_case first")))?;
        to_py(py, &serde_json::to_value(case.flow()).map_err(|e| CommandError::new_err(e.to_string()))?)
    }

    /// One of a case's row dataflows at step `step` (Reserves case plan, workstream 6; 0 before its first step), on what's
    /// saved or with `tables` as edited (`{parameter table name or id: CSV text}`, never saved): `{name, step, steps, range,
    /// rows_in_range, outside, columns: [{field, column, unit, before, after, last_run}], rows: [{row, label, before,
    /// after}], tables, stopped, said, ms}`. `dataflow` is its id or name. Nothing is kept or saved.
    #[pyo3(signature = (table, dataflow, step, tables=None))]
    fn row_step(&self, py: Python<'_>, table: &str, dataflow: &str, step: usize, tables: Option<std::collections::BTreeMap<String, String>>) -> PyResult<Py<PyAny>> {
        let id = object_id(table)?;
        let project = self.inner.project();
        let flow = object_id(dataflow)
            .ok()
            .filter(|i| project.object(*i).is_some())
            .or_else(|| project.objects().find(|o| o.name == dataflow && matches!(o.body, Body::Dataflow(_))).map(|o| o.id))
            .ok_or_else(|| PyValueError::new_err(format!("no dataflow {dataflow}")))?;
        let unsaved = zd_commands::builtin::Unsaved { dataflow: None, tables: self.edited_tables(tables)? };
        let view = zd_commands::builtin::row_dataflow_at_step(project, id, flow, step, &unsaved).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &serde_json::to_value(view).map_err(|e| CommandError::new_err(e.to_string()))?)
    }

    /// Adds waste allocation to a dataflow (the Waste allocation plan, decision 7): its four parameter tables, where the
    /// project hasn't one of that name, then the dataflow saved with the schedule fields, allocation steps and checks
    /// added as the Waste allocation add-on the dataflow lists (Table setup plan, decision 2). Gives back
    /// `{"tables": [ids, settings first], "add_on": id}`.
    fn add_waste_allocation(&mut self, py: Python<'_>, dataflow: &str) -> PyResult<Py<PyAny>> {
        let id = object_id(dataflow)?;
        let (tables, add_on) = py.detach(|| self.inner.add_waste_allocation(id)).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!({ "tables": tables.iter().map(|t| t.to_string()).collect::<Vec<_>>(), "add_on": add_on.to_string() }))
    }

    /// One block's column after each of a dataflow's steps (decision 0072): `params` as `reserves.run` takes them, the block
    /// by case and its values of the dataflow's levels above the seam (`["Pit", "3", "B1"]` by default, decision 0074). A
    /// list of steps, each with its units and their values by field.
    #[pyo3(signature = (params, path, case=None))]
    fn reserves_preview(&self, py: Python<'_>, params: &Bound<'_, PyDict>, path: Vec<String>, case: Option<&str>) -> PyResult<Py<PyAny>> {
        let p: zd_commands::builtin::RunReservesParams = serde_json::from_value(from_py(params.as_any())?).map_err(|e| CommandError::new_err(e.to_string()))?;
        let project = self.inner.project();
        let get = |id| project.object(id).cloned().ok_or(zd_commands::CommandError::MissingObject(id));
        let views = zd_commands::builtin::reserves_preview(&get, &p, case.unwrap_or(zd_core::BASE_CASE), &path).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!(views))
    }

    /// The brief an AI is handed to write a script right (decision 0104): how scripts are written, what this kind may
    /// set, every name it can read with its meaning, the parameter tables and scripts, the script and the ask, as
    /// Markdown. Keywords as the app's: `kind` (formula, allocation, calculated, object, table, library), `dataflow`,
    /// `table`, `objects`, `source`, `name`, `words` (the words columns hold, off unless asked), `ask`, `problems`.
    #[pyo3(signature = (**brief))]
    fn brief(&self, brief: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
        let b = brief_of(brief)?;
        zd_commands::builtin::brief(self.inner.project(), &b).map_err(|e| CommandError::new_err(e.to_string()))
    }

    /// A script checked against the project as its editor checks it, nothing kept: a list of `{message, line}`, empty
    /// where it holds. Keywords as `brief`.
    #[pyo3(signature = (**brief))]
    fn check_script(&self, py: Python<'_>, brief: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
        let b = brief_of(brief)?;
        let problems = zd_commands::builtin::check_script(self.inner.project(), &b).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!(problems))
    }

    /// A script tried on the project as it stands, nothing kept: `{said, problems}`. A calculated column on row `row`
    /// line by line, an object script's sets on the first blocks, a table script's table and first rows.
    #[pyo3(signature = (row=0, **brief))]
    fn try_script(&self, py: Python<'_>, row: usize, brief: Option<&Bound<'_, PyDict>>) -> PyResult<Py<PyAny>> {
        let b = brief_of(brief)?;
        let tried = zd_commands::builtin::try_script(self.inner.project(), &b, row).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!(tried))
    }

    #[pyo3(signature = (workflow, utc_offset_minutes=None))]
    fn workflow_report(&self, py: Python<'_>, workflow: &str, utc_offset_minutes: Option<i32>) -> PyResult<Py<PyAny>> {
        let id = object_id(workflow)?;
        let report = self.inner.workflow_report(id, utc_offset_minutes.unwrap_or(0)).map_err(|e| CommandError::new_err(e.to_string()))?;
        to_py(py, &json!(report))
    }

    /// Writes the workflow report as one HTML page that opens offline (decision 0068), into `folder`, or the project's
    /// `<project> reports` folder beside its file, dated as `workflow_report` is. Returns the page's path.
    #[pyo3(signature = (workflow, folder=None, utc_offset_minutes=None))]
    fn write_workflow_report(&self, workflow: &str, folder: Option<&str>, utc_offset_minutes: Option<i32>) -> PyResult<String> {
        let id = object_id(workflow)?;
        let path = self.inner.write_workflow_report(id, utc_offset_minutes.unwrap_or(0), folder.map(std::path::Path::new)).map_err(|e| CommandError::new_err(e.to_string()))?;
        Ok(path.display().to_string())
    }

    /// Every run, undo, redo, save and open, oldest first, as saved in the project.
    fn log(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_py(py, &json!(self.inner.log()))
    }

    /// Every seam model's grid files checked where they are now (decision 0058, stage 4): for each model its id,
    /// name, the folders it reads from with how many files each holds and how many are missing, and the files whose
    /// content has changed since it was built. `seam.relink` points a folder at where it is now.
    fn links(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let links = py.detach(|| self.inner.links());
        to_py(py, &json!(links))
    }

    /// What opening did beyond reading: seam models relinked to grid folders found beside the project, and folders
    /// found neither where they were nor beside it.
    #[getter]
    fn open_notes(&self) -> Vec<String> {
        self.inner.open_notes().to_vec()
    }

    /// Shows or hides a layer by name, as its tick in the app does: display state, saved with the project, not a run
    /// and not undone. False if no layer has that name.
    fn set_layer_visible(&mut self, layer: &str, visible: bool) -> bool {
        match self.inner.project().layer_named(layer).map(|l| l.id) {
            Some(id) => {
                self.inner.set_layer_visible(id, visible);
                true
            }
            None => false,
        }
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

/// Opens a `.zdp` project file, or an older `<name>.zdproj` folder: the same as `zd.Session.open(path)`.
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
    m.add_function(wrap_pyfunction!(licence, m)?)?;
    #[cfg(not(feature = "every-module"))]
    apply_licence(&zd_licence::check_this_machine());
    Ok(())
}

/// The licence this machine holds (decision 0098): `import zd` runs the modules a licence turns on only if it lists
/// Python; otherwise only the Viewer's commands run.
#[cfg(not(feature = "every-module"))]
fn apply_licence(status: &zd_licence::Status) {
    use zd_commands::modules::{set_licence_stamp, set_modules_on};
    let on: Vec<zd_commands::Module> = if status.has("python") { status.modules.iter().filter_map(|m| zd_commands::Module::from_name(m)).collect() } else { Vec::new() };
    set_modules_on(on);
    set_licence_stamp(status.stamp());
}

impl PySession {
    /// Parameter tables as edited, `{name or id: CSV text}`, each read and checked, by id.
    fn edited_tables(&self, tables: Option<std::collections::BTreeMap<String, String>>) -> PyResult<Vec<(zd_core::ObjectId, std::sync::Arc<zd_core::ParameterTable>)>> {
        let project = self.inner.project();
        let mut edited = Vec::new();
        for (key, csv) in tables.unwrap_or_default() {
            let found = object_id(&key).ok().and_then(|i| project.object(i)).or_else(|| project.objects().find(|o| o.name == key && matches!(o.body, Body::ParameterTable(_))));
            let object = found.ok_or_else(|| PyValueError::new_err(format!("no parameter table {key}")))?;
            let parsed = zd_core::ParameterTable::from_csv(&csv).map_err(|e| PyValueError::new_err(format!("{key}: {e}")))?;
            edited.push((object.id, std::sync::Arc::new(parsed)));
        }
        Ok(edited)
    }

    /// The case `step_case` keeps for a table, or why there's none.
    fn kept_case(&self, table: &str) -> PyResult<&zd_commands::builtin::CaseAtStep> {
        let id = object_id(table)?;
        self.steps.get(&id).ok_or_else(|| PyValueError::new_err(format!("no case kept for {table}: open it at a step with step_case first")))
    }
}

/// What this machine's licence says, where it stands, the modules on and this machine's code, as a dict.
#[pyfunction]
fn licence(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let status = serde_json::to_value(zd_licence::check_this_machine()).map_err(|e| PyValueError::new_err(e.to_string()))?;
    to_py(py, &status)
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

/// The lowest and highest corner of the object's geometry, or None (a seam model, or no geometry).
fn bounds(body: &Body) -> Option<[[f64; 3]; 2]> {
    let corners = |points: &[[f64; 3]]| {
        points.iter().fold(None, |acc: Option<[[f64; 3]; 2]>, p| match acc {
            None => Some([*p, *p]),
            Some([lo, hi]) => Some([[0, 1, 2].map(|i| lo[i].min(p[i])), [0, 1, 2].map(|i| hi[i].max(p[i]))]),
        })
    };
    match body {
        Body::String(line) => corners(&line.vertices),
        Body::Surface(s) => s.mesh().bounds().map(|b| [b.min, b.max]),
        Body::Solid(s) => s.mesh().bounds().map(|b| [b.min, b.max]),
        Body::Grid(g) => {
            let [lo, hi] = g.z_range()?;
            let far = [g.origin[0] + (g.cols - 1) as f64 * g.cell_size[0], g.origin[1] + (g.rows - 1) as f64 * g.cell_size[1]];
            Some([[g.origin[0], g.origin[1], lo], [far[0], far[1], hi]])
        }
        Body::SeamModel(_) | Body::FaceDesign(_) | Body::Workflow(_) | Body::ParameterTable(_) | Body::Dataflow(_) | Body::Table(_) | Body::Script(_) => None,
    }
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
            "pieces": s.pieces(),
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
        Body::FaceDesign(d) => json!({ "design": **d }),
        Body::Workflow(w) => json!({ "workflow": **w }),
        Body::ParameterTable(table) => json!({ "parameters": **table, "csv": table.to_csv() }),
        Body::Dataflow(d) => json!({ "dataflow": **d }),
        Body::Script(s) => json!({ "script": **s }),
        Body::Table(table) => table_json(table),
    }
}

/// A table's columns by name, each a list of its values (None where there's none), and their units.
fn table_json(table: &zd_core::Table) -> Value {
    json!({
        "rows": table.rows(),
        "units": table.columns.iter().map(|c| (c.name.clone(), json!(c.unit))).collect::<serde_json::Map<String, Value>>(),
        "columns": table.columns.iter().map(|c| (c.name.clone(), match &c.values {
            zd_core::ColumnValues::Number(v) => json!(v),
            zd_core::ColumnValues::Text(v) => json!(v),
        })).collect::<serde_json::Map<String, Value>>(),
    })
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

/// A brief's keywords read as the engine's `BriefFor`.
fn brief_of(brief: Option<&Bound<'_, PyDict>>) -> PyResult<zd_commands::builtin::BriefFor> {
    let value = match brief {
        Some(d) => from_py(d.as_any())?,
        None => json!({}),
    };
    serde_json::from_value(value).map_err(|e| CommandError::new_err(e.to_string()))
}
