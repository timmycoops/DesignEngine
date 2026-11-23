//! Commands the viewport calls. Every edit to the project goes through the engine session as a
//! typed command (`run_command`); the viewport is sent what changed in the project after each
//! change (`changes.rs`), and fetches geometry as binary render buffers (WebGPU shared buffer or raw IPC
//! bytes), never as JSON.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Response;
use tauri::{AppHandle, State, Webview};
use zd_commands::builtin::ModelLinks;
use zd_commands::{LogEntry, RunRecord, Session};
use zd_core::{Attributes, Body, Colouring, Contours, Crossings, Display, FaceDesign, LayerId, MeshValidity, ObjectId, RunId, SeamModel, TriMesh};
use zd_geom::{SectionPlane, StackHorizon, SurfaceRayCaster, stack_issue_nodes, triangulate_grid};
use zd_io::{check_links as check_grid_links, list_deswik_layers, list_spry, read_stack_grids};

use crate::changes::{ProjectChanges, changes, changes_after_hiding};
use crate::render::{NodeValues, encode_grid, encode_grid_with, encode_levels, encode_section, encode_solid_levels, encode_solids, encode_triangulation};
use crate::selection;
use crate::section::{SectionCut, add_selected, pick_on_section, section_groups, snap_on_section};
use crate::shared::post_shared_buffer;
use crate::state::{AppInner, AppState, Running, SolidCaster, TileId, bounds, geometry_key, is_drawn_surface, tile_solids};
use crate::synthetic::synthetic_topography;

/// The whole project, in place of what the app holds: after a reload, or when a change didn't follow on from what the
/// app holds.
#[tauri::command]
pub async fn project_summary(state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    Ok(changes(&mut state.app.lock().unwrap(), true))
}

/// Each block of one seam with its value of an attribute, to colour it by against the seam's grid (decision 0061,
/// workstream 6): the solids whose `seam_attribute` names `horizon` of `model`, as the interrogation matches them, and
/// their `attribute` where it's a number.
#[tauri::command]
pub async fn block_values(model: ObjectId, horizon: String, seam_attribute: String, attribute: String, state: State<'_, AppState>) -> Result<Vec<(ObjectId, f64)>, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let object = project.object(model).ok_or("no such seam model")?;
        let Body::SeamModel(m) = &object.body else { return Err(format!("'{}' is not a seam model", object.name)) };
        let h = m.horizon(&horizon).ok_or_else(|| format!("no horizon '{horizon}' in '{}'", object.name))?;
        let text = |v: &zd_core::AttrValue| match v {
            zd_core::AttrValue::Text(t) => t.trim().to_string(),
            zd_core::AttrValue::Number(n) => format!("{n}"),
            zd_core::AttrValue::Bool(b) => b.to_string(),
        };
        Ok(project
            .objects()
            .filter(|o| matches!(o.body, Body::Solid(_)))
            .filter(|o| o.attributes.get(&seam_attribute).is_some_and(|v| zd_commands::builtin::names_seam(h, &text(v))))
            .filter_map(|o| match o.attributes.get(&attribute) {
                Some(zd_core::AttrValue::Number(n)) if n.is_finite() => Some((o.id, *n)),
                _ => None,
            })
            .collect())
    })
    .await?
}

/// One revision of an object and the run that wrote it.
#[derive(Serialize)]
pub struct RevisionSummary {
    revision: u32,
    run: RunId,
    /// `None` when the run is not in this session's log (a project saved before run logs).
    command: Option<String>,
    at_unix_ms: Option<u64>,
}

/// What the properties panel shows for one object, in world coordinates.
#[derive(Serialize)]
pub struct ObjectDetails {
    id: ObjectId,
    name: String,
    layer: String,
    attributes: Attributes,
    bounds: Option<[[f64; 3]; 2]>,
    /// Oldest first; the last entry is the current revision.
    history: Vec<RevisionSummary>,
    measures: Measures,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Measures {
    String { points: Vec<[f64; 3]>, closed: bool, length: f64, plan_length: f64, plan_area: Option<f64> },
    /// `crossings`: the crossing check, in world coordinates; `None` if none has run.
    Surface { vertices: usize, triangles: usize, area: f64, validity: MeshValidity, crossings: Option<Crossings> },
    Solid { vertices: usize, triangles: usize, area: f64, volume: f64, pieces: zd_core::Pieces, validity: MeshValidity, crossings: Option<Crossings> },
    Grid { origin: [f64; 2], cell_size: [f64; 2], cols: usize, rows: usize, nulls: usize, z_range: Option<[f64; 2]> },
    SeamModel { model: SeamModel },
    FaceDesign { design: FaceDesign },
    Workflow { workflow: zd_core::Workflow },
    /// A parameter table and the same as CSV (decision 0072).
    ParameterTable { parameters: zd_core::ParameterTable, csv: String },
    Dataflow { dataflow: zd_core::Dataflow },
    /// A script's Starlark (Tables and scripts plan, decision 9).
    Script { script: zd_core::Script },
    /// A results table's size, its columns, and its rows added up by write point, horizon and bench (decision 0072).
    Table { rows: usize, columns: Vec<zd_core::ColumnHeader>, figures: zd_commands::report::ReservesFigures },
}

fn measure(body: &Body) -> Measures {
    match body {
        Body::String(line) => Measures::String {
            points: line.vertices.clone(),
            closed: line.closed,
            length: line.length(),
            plan_length: line.plan_length(),
            plan_area: line.plan_area(),
        },
        Body::Surface(s) => Measures::Surface {
            vertices: s.mesh().vertices.len(),
            triangles: s.mesh().triangle_count(),
            area: s.mesh().area(),
            validity: s.validity(),
            crossings: s.crossings().cloned(),
        },
        Body::Solid(s) => Measures::Solid {
            vertices: s.mesh().vertices.len(),
            triangles: s.mesh().triangle_count(),
            area: s.mesh().area(),
            volume: s.volume(),
            pieces: s.pieces(),
            validity: s.validity(),
            crossings: s.crossings().cloned(),
        },
        Body::Grid(g) => Measures::Grid {
            origin: g.origin,
            cell_size: g.cell_size,
            cols: g.cols,
            rows: g.rows,
            nulls: g.null_count(),
            z_range: g.z_range(),
        },
        Body::SeamModel(m) => Measures::SeamModel { model: (**m).clone() },
        Body::FaceDesign(d) => Measures::FaceDesign { design: (**d).clone() },
        Body::Workflow(w) => Measures::Workflow { workflow: (**w).clone() },
        Body::ParameterTable(t) => Measures::ParameterTable { parameters: (**t).clone(), csv: t.to_csv() },
        Body::Dataflow(d) => Measures::Dataflow { dataflow: (**d).clone() },
        Body::Script(s) => Measures::Script { script: (**s).clone() },
        Body::Table(t) => Measures::Table { rows: t.rows(), columns: t.headers(), figures: zd_commands::report::reserves_figures("", t) },
    }
}

/// Details of the current revision of `id`; areas and ranges are computed off the main thread
/// because surfaces can run to millions of triangles.
#[tauri::command]
pub async fn object_details(id: ObjectId, state: State<'_, AppState>) -> Result<ObjectDetails, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let object = project.object(id).ok_or("no such object")?.clone();
        let layer = project.layer(object.layer).map(|l| l.name.clone()).unwrap_or_default();
        let runs: HashMap<RunId, (&str, u64)> = app
            .session
            .log()
            .iter()
            .filter_map(|entry| match entry {
                LogEntry::Run(r) => Some((r.run, (r.command.as_str(), r.started_unix_ms))),
                _ => None,
            })
            .collect();
        let history = (1..=object.revision)
            .filter_map(|r| project.revision(id, r))
            .map(|rev| {
                let run = runs.get(&rev.provenance);
                RevisionSummary {
                    revision: rev.revision,
                    run: rev.provenance,
                    command: run.map(|(c, _)| c.to_string()),
                    at_unix_ms: run.map(|&(_, t)| t),
                }
            })
            .collect();
        Ok(ObjectDetails {
            id,
            name: object.name.clone(),
            layer,
            attributes: object.attributes.clone(),
            bounds: bounds(&object).map(|b| [b.min, b.max]),
            history,
            measures: measure(&object.body),
        })
    })
    .await?
}

#[derive(Serialize)]
pub struct RunReply {
    record: RunRecord,
    changes: ProjectChanges,
}

/// Runs an engine command by name with JSON parameters. A failure comes back as the error text
/// (and is in the run log either way). The run reads the project off the lock and takes it again only to apply what it
/// made, so the view carries on while it goes; one runs at a time (decision 0060).
#[tauri::command]
pub async fn run_command(name: String, params: Value, state: State<'_, AppState>) -> Result<RunReply, String> {
    let app = state.app.clone();
    blocking(move || {
        let job = {
            let mut app = app.lock().unwrap();
            app.idle()?;
            let job = app.session.begin_run(&name, params);
            app.running = Some(Running { command: name.clone(), started: Instant::now(), control: job.control() });
            job
        };
        let done = job.run();
        let mut app = app.lock().unwrap();
        app.running = None;
        let record = app.session.end_run(done).map_err(|e| e.to_string())?;
        Ok(RunReply { record, changes: changes(&mut app, false) })
    })
    .await?
}

/// A workflow run's record and what it changed.
#[derive(Serialize)]
pub struct WorkflowReply {
    record: zd_commands::WorkflowRecord,
    changes: ProjectChanges,
}

/// Runs a workflow, or one of its cases (decision 0066), from the step at `from` (from 1) to its end, a pause or the first
/// refusal (decision 0065). Each step's command runs off the lock as any run does (0060), the run going named for it so
/// the status line says what it's doing and Cancel stops it; nothing else changes the project until it's done. One undo.
#[tauri::command]
pub async fn run_workflow(workflow: ObjectId, from: usize, case: Option<String>, state: State<'_, AppState>) -> Result<WorkflowReply, String> {
    let app = state.app.clone();
    blocking(move || {
        let record = run_one_case(&app, workflow, from, case.as_deref().unwrap_or(zd_core::BASE_CASE))?;
        let mut app = app.lock().unwrap();
        Ok(WorkflowReply { record, changes: changes(&mut app, false) })
    })
    .await?
}

/// Every case's run, Base first.
#[derive(Serialize)]
pub struct CasesReply {
    records: Vec<zd_commands::WorkflowRecord>,
    changes: ProjectChanges,
}

/// Runs every case of a workflow from its first step, Base first, as one undo (decision 0066); a pause or a refusal
/// stops it at that case.
#[tauri::command]
pub async fn run_all_cases(workflow: ObjectId, from: Option<CasesFrom>, state: State<'_, AppState>) -> Result<CasesReply, String> {
    let app = state.app.clone();
    blocking(move || {
        let (names, since) = {
            let app = app.lock().unwrap();
            app.idle()?;
            (app.session.case_names(workflow).map_err(|e| e.to_string())?, app.session.undo_steps())
        };
        let names = cases_from(names, from.as_ref())?;
        let mut records = Vec::new();
        for (name, step) in names {
            let record = run_one_case(&app, workflow, step, &name)?;
            let stop = record.status != zd_commands::WorkflowStatus::Finished;
            records.push(record);
            if stop {
                break;
            }
        }
        let mut app = app.lock().unwrap();
        if let Some(first) = records.first() {
            app.session.join_undo(since, first.run);
        }
        Ok(CasesReply { records, changes: changes(&mut app, false) })
    })
    .await?
}

/// Where Continue picks Run all cases up after a pause (app review brief, 17 Nov, item 3): the case that paused, and
/// the step after the pause.
#[derive(serde::Deserialize)]
pub struct CasesFrom {
    case: String,
    step: usize,
}

/// The cases a Run all cases runs, each with the step it starts from: every case from step 1, or, carrying on after a
/// pause, the case that paused from the step after it and the cases after it from step 1.
fn cases_from(names: Vec<String>, from: Option<&CasesFrom>) -> Result<Vec<(String, usize)>, String> {
    let Some(from) = from else { return Ok(names.into_iter().map(|n| (n, 1)).collect()) };
    let at = names.iter().position(|n| *n == from.case).ok_or_else(|| format!("the workflow has no case {}", from.case))?;
    Ok(names.into_iter().skip(at).enumerate().map(|(k, n)| (n, if k == 0 { from.step } else { 1 })).collect())
}

/// One case of a workflow run step by step, each step's command off the lock.
fn run_one_case(app: &std::sync::Arc<std::sync::Mutex<AppInner>>, workflow: ObjectId, from: usize, case: &str) -> Result<zd_commands::WorkflowRecord, String> {
    let mut run = {
        let app = app.lock().unwrap();
        app.idle()?;
        app.session.begin_case(workflow, from, case).map_err(|e| e.to_string())?
    };
    loop {
        let job = {
            let mut app = app.lock().unwrap();
            let Some(job) = app.session.next_step(&mut run) else { break };
            app.running = Some(Running { command: job.command().to_string(), started: Instant::now(), control: job.control() });
            job
        };
        let done = job.run();
        let mut app = app.lock().unwrap();
        app.session.end_step(&mut run, done);
    }
    let mut app = app.lock().unwrap();
    app.running = None;
    Ok(app.session.end_workflow(run))
}

/// Captures runs from the run log as a new workflow (decision 0065), saved by a `workflow.save` run.
#[tauri::command]
pub async fn capture_workflow(name: String, runs: Vec<RunId>, state: State<'_, AppState>) -> Result<RunReply, String> {
    let app = state.app.clone();
    blocking(move || {
        let mut app = app.lock().unwrap();
        app.idle()?;
        let record = app.session.capture_workflow(&name, &runs).map_err(|e| e.to_string())?;
        Ok(RunReply { record, changes: changes(&mut app, false) })
    })
    .await?
}

/// Adds a tool panel's command and settings to a workflow as a step, at `at` (from 0) or at the end, its objects turned
/// into rules (decision 0065, Add to workflow).
#[tauri::command]
pub async fn add_workflow_step(workflow: ObjectId, at: Option<usize>, command: String, params: Value, note: Option<String>, state: State<'_, AppState>) -> Result<RunReply, String> {
    let app = state.app.clone();
    blocking(move || {
        let mut app = app.lock().unwrap();
        app.idle()?;
        let record = app.session.add_workflow_step(workflow, at, &command, &params, note.as_deref().unwrap_or("")).map_err(|e| e.to_string())?;
        Ok(RunReply { record, changes: changes(&mut app, false) })
    })
    .await?
}

/// A workflow report written beside the project (decision 0068).
#[derive(Serialize)]
pub struct ReportFile {
    path: String,
    name: String,
    modified_unix_ms: u64,
}

fn report_file(path: &std::path::Path) -> Option<ReportFile> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(ReportFile {
        path: path.display().to_string(),
        name: path.file_stem()?.to_string_lossy().into_owned(),
        modified_unix_ms: modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
    })
}

/// Writes a workflow's report, every case from its latest run, as one HTML page in the reports folder beside the project,
/// and opens it in the default browser (decision 0068). `utc_offset_minutes` dates it in the machine's local time.
#[tauri::command]
pub async fn write_workflow_report(workflow: ObjectId, utc_offset_minutes: i32, state: State<'_, AppState>) -> Result<ReportFile, String> {
    let app = state.app.clone();
    let path = blocking(move || {
        let app = app.lock().unwrap();
        app.session.write_workflow_report(workflow, utc_offset_minutes, None).map_err(|e| e.to_string())
    })
    .await??;
    open_in_browser(&path)?;
    report_file(&path).ok_or_else(|| format!("the report was written but can't be read back: {}", path.display()))
}

/// The reports beside the project for a workflow, newest first.
#[tauri::command]
pub fn workflow_reports(workflow: ObjectId, state: State<'_, AppState>) -> Result<Vec<ReportFile>, String> {
    let app = state.app.lock().unwrap();
    let Some(file) = app.session.file() else { return Ok(Vec::new()) };
    let name = app.session.project().object(workflow).map(|o| o.name.clone()).unwrap_or_default();
    let folder = zd_commands::report::reports_folder(&file);
    let Ok(entries) = std::fs::read_dir(&folder) else { return Ok(Vec::new()) };
    let mut files: Vec<ReportFile> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("html")))
        .filter(|p| p.file_stem().is_some_and(|s| s.to_string_lossy().starts_with(&format!("{name} "))))
        .filter_map(|p| report_file(&p))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.modified_unix_ms));
    Ok(files)
}

/// Opens a report in the default browser: only a page in the project's reports folder.
#[tauri::command]
pub fn open_workflow_report(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let app = state.app.lock().unwrap();
    let file = app.session.file().ok_or("the project isn't saved, so it has no reports")?;
    let path = std::path::PathBuf::from(path);
    let folder = std::fs::canonicalize(zd_commands::report::reports_folder(&file)).ok();
    let inside = folder.is_some() && path.parent().and_then(|p| std::fs::canonicalize(p).ok()) == folder;
    if !inside || !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("html")) {
        return Err("only a report in the project's reports folder opens from here".into());
    }
    open_in_browser(&path)
}

/// The page in the machine's default browser.
fn open_in_browser(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    let opened = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let opened = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let opened = std::process::Command::new("xdg-open").arg(path).spawn();
    opened.map(|_| ()).map_err(|e| format!("the report is at {}, but it couldn't be opened: {e}", path.display()))
}

/// The run going, if one is: what it is, how long it's been going and how far it has got.
#[derive(Serialize)]
pub struct RunProgress {
    command: String,
    seconds: f64,
    done: u64,
    /// 0 until the run says what it's counting.
    total: u64,
    of: &'static str,
    cancelled: bool,
}

#[tauri::command]
pub async fn run_progress(state: State<'_, AppState>) -> Result<Option<RunProgress>, String> {
    let app = state.app.lock().unwrap();
    Ok(app.running.as_ref().map(|r| {
        let p = r.control.progress();
        RunProgress { command: r.command.clone(), seconds: r.started.elapsed().as_secs_f64(), done: p.done, total: p.total, of: p.of, cancelled: r.control.is_cancelled() }
    }))
}

/// Asks the run going to stop: it does at the next solid, strip or file, and applies nothing. False if none is going.
#[tauri::command]
pub async fn cancel_run(state: State<'_, AppState>) -> Result<bool, String> {
    let app = state.app.lock().unwrap();
    Ok(app.running.as_ref().map(|r| r.control.cancel()).is_some())
}

#[tauri::command]
pub async fn undo(state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    app.session.undo();
    Ok(changes(&mut app, false))
}

#[tauri::command]
pub async fn redo(state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    app.session.redo();
    Ok(changes(&mut app, false))
}

#[tauri::command]
pub async fn new_project(state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    *app = AppInner::new(Session::with_builtins(), None);
    Ok(changes(&mut app, true))
}

#[tauri::command]
pub async fn open_project(path: String, state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    let app = state.app.clone();
    blocking(move || {
        app.lock().unwrap().idle()?;
        let session = Session::open(&path).map_err(|e| e.to_string())?;
        let mut app = app.lock().unwrap();
        app.idle()?;
        *app = AppInner::new(session, Some(PathBuf::from(path)));
        Ok(changes(&mut app, true))
    })
    .await?
}

/// Where every seam model's grids are now, and what opening did about any that had moved (decision 0058, stage 4).
#[derive(Serialize)]
pub struct LinksReply {
    /// Seam models relinked to grid folders found beside the project when it opened, and folders found neither way.
    notes: Vec<String>,
    models: Vec<ModelLinks>,
}

/// Checks each seam model's grid files where they are now: missing, or changed since the model was built. The files
/// are looked at off the app's lock, as a network share can be slow to answer.
#[tauri::command]
pub async fn check_links(state: State<'_, AppState>) -> Result<LinksReply, String> {
    let app = state.app.clone();
    blocking(move || {
        let (notes, models) = {
            let app = app.lock().unwrap();
            let models: Vec<(ObjectId, String, Arc<SeamModel>)> = app
                .session
                .project()
                .objects()
                .filter_map(|o| match &o.body {
                    Body::SeamModel(m) => Some((o.id, o.name.clone(), m.clone())),
                    _ => None,
                })
                .collect();
            (app.session.open_notes().to_vec(), models)
        };
        let models = models.into_iter().map(|(object, name, model)| ModelLinks { object, name, check: check_grid_links(&model) }).collect();
        Ok(LinksReply { notes, models })
    })
    .await?
}

#[derive(Serialize)]
pub struct SaveReply {
    /// The `.zdp` file written: a project opened from an older `.zdproj` folder saves beside it.
    path: String,
    /// Nothing had changed since the last save, so nothing was written.
    unchanged: bool,
    entries: usize,
    /// Entries encoded this time; the rest were kept from the last save.
    encoded: usize,
    /// The file's size.
    bytes: u64,
    ms: f64,
}

/// Saves to `path`, or to the file last opened or saved when `path` is not given. The project is taken under the
/// lock in an instant and written off it, so the viewport and commands carry on while a large project saves
/// (decision 0058).
#[tauri::command]
pub async fn save_project(path: Option<String>, state: State<'_, AppState>) -> Result<SaveReply, String> {
    let app = state.app.clone();
    blocking(move || {
        let job = {
            let mut app = app.lock().unwrap();
            let path = match path.map(PathBuf::from).or_else(|| app.path.clone()) {
                Some(p) => p,
                None => return Err("this project has not been saved yet; choose a file".to_string()),
            };
            app.session.begin_save(&path).map_err(|e| e.to_string())?
        };
        let result = job.run();
        let mut app = app.lock().unwrap();
        // A project opened while this one saved keeps its own log and path.
        let same = match &result {
            Ok(saved) => app.session.file().as_deref() == Some(saved.path.as_path()),
            Err(_) => true,
        };
        let saved = if same { app.session.end_save(job, result) } else { result }.map_err(|e| e.to_string())?;
        if same {
            app.path = Some(saved.path.clone());
            app.locked_by = None;
        }
        Ok(SaveReply {
            path: saved.path.display().to_string(),
            unchanged: saved.unchanged,
            entries: saved.entries,
            encoded: saved.encoded,
            bytes: saved.bytes,
            ms: saved.ms,
        })
    })
    .await?
}

/// Display state only: not a command, not undone.
/// Shows or hides objects one by one, under their layer's tick. Display state: not a run, not undone.
#[tauri::command]
pub async fn set_objects_visible(objects: Vec<ObjectId>, visible: bool, state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    app.session.set_objects_visible(&objects, visible);
    Ok(changes_after_hiding(&mut app))
}

/// A layer's filter by attributes and measures (decision 0088, workstream 3): what doesn't fit, on it and the layers
/// under it, is drawn, snapped to and cut as hidden. Display state, as hiding is: not a run, not undone. Only the rows
/// whose flags changed come back.
#[tauri::command]
pub async fn set_layer_filter(layer: LayerId, when: std::collections::BTreeMap<String, String>, state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    app.session.set_layer_filter(layer, when);
    Ok(changes_after_hiding(&mut app))
}

/// A reserves table's leaves filter carried to the view (decision 0090), or taken away (None): the solids on the leaves
/// it leaves out, and with `coal_only` the waste on those it keeps, drawn, snapped to and cut as hidden. Display state.
#[tauri::command]
pub async fn set_reserves_filter(filter: Option<zd_core::ReservesFilter>, state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    app.session.set_reserves_filter(filter);
    Ok(changes_after_hiding(&mut app))
}

/// Every attribute of what's on a layer and the layers under it, an object a row: what the filter box lists and counts
/// to tick (decision 0088). Only what the layer tree speaks for: no face designs, workflows, tables or dataflows.
#[tauri::command]
pub async fn layer_attributes(layer: LayerId, state: State<'_, AppState>) -> Result<Vec<zd_core::Attributes>, String> {
    let app = state.app.lock().unwrap();
    let project = app.session.project();
    let Some(name) = project.layer(layer).map(|l| l.name.clone()) else { return Err("no such layer".into()) };
    let under: std::collections::HashSet<LayerId> = project.layers().iter().filter(|l| l.name == name || l.name.strip_prefix(name.as_str()).is_some_and(|rest| rest.starts_with('\\'))).map(|l| l.id).collect();
    Ok(project
        .objects()
        .filter(|o| under.contains(&o.layer) && !matches!(o.kind(), zd_core::ObjectKind::FaceDesign | zd_core::ObjectKind::Workflow | zd_core::ObjectKind::ParameterTable | zd_core::ObjectKind::Dataflow | zd_core::ObjectKind::Table | zd_core::ObjectKind::Script))
        .map(|o| o.attributes.clone())
        .collect())
}

/// What one input of a workflow step finds before its filter, an object a row: its attributes and the measures it has
/// that no attribute names (`volume`, `length`, `area`, `rl_min`, `rl_max`), for the Workflow panel's filter box to list
/// and count (decision 0088, workstream 5). Seen as the case picked sees the project.
#[tauri::command]
pub async fn workflow_input_rows(workflow: ObjectId, step: u32, input: String, case: Option<String>, state: State<'_, AppState>) -> Result<Vec<zd_core::Attributes>, String> {
    let app = state.app.lock().unwrap();
    let pool = app.session.input_pool(workflow, step, &input, case.as_deref().unwrap_or(zd_core::BASE_CASE)).map_err(|e| e.to_string())?;
    let project = app.session.project();
    Ok(pool
        .iter()
        .filter_map(|&id| project.object(id))
        .map(|o| {
            let mut row = o.attributes.clone();
            for name in zd_core::MEASURES {
                if zd_core::attribute_ci(&o.attributes, name).is_none()
                    && let Some(value) = zd_core::measure(o, name)
                {
                    row.insert(name.to_string(), zd_core::AttrValue::Number(value));
                }
            }
            row
        })
        .collect())
}

#[tauri::command]
pub async fn set_layer_visible(layer: LayerId, visible: bool, state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    app.session.set_layer_visible(layer, visible);
    Ok(changes_after_hiding(&mut app))
}

/// Colours objects by RL or by their layer, sets how see-through they are, or both. Display state: not a
/// run, not undone. Only the looks come back, not the whole summary: nothing else changes, and a slider
/// shouldn't wait on a pit's worth of summary.
#[tauri::command]
pub async fn set_looks(objects: Vec<ObjectId>, colouring: Option<Colouring>, opacity: Option<f64>, state: State<'_, AppState>) -> Result<Display, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    app.session.set_looks(&objects, colouring, opacity);
    Ok(app.session.project().display().clone())
}

/// How see-through a layer is. Display state, as above.
#[tauri::command]
pub async fn set_layer_opacity(layer: LayerId, opacity: f64, state: State<'_, AppState>) -> Result<Display, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    app.session.set_layer_opacity(layer, opacity);
    Ok(app.session.project().display().clone())
}

/// The bands of colour by RL. Display state, as above.
#[tauri::command]
pub async fn set_contours(contours: Contours, state: State<'_, AppState>) -> Result<Display, String> {
    let mut app = state.app.lock().unwrap();
    app.idle()?;
    app.session.set_contours(contours);
    Ok(app.session.project().display().clone())
}

/// The run log from entry `from` on, when the app holds the first `from` entries of `session`'s log; the whole log when
/// it holds another session's (a project opened since), as `from` says.
#[derive(Serialize)]
pub struct LogPart {
    session: u64,
    from: usize,
    entries: Vec<LogEntry>,
}

#[tauri::command]
pub async fn run_log(session: Option<u64>, from: Option<usize>, state: State<'_, AppState>) -> Result<LogPart, String> {
    let app = state.app.lock().unwrap();
    let log = app.session.log();
    let from = if session == Some(app.log_session) { from.unwrap_or(0).min(log.len()) } else { 0 };
    Ok(LogPart { session: app.log_session, from, entries: log[from..].to_vec() })
}

/// Every attribute of each object, in the order asked, for what needs more than a row carries (naming solids' files
/// by attributes); an object gone has none.
#[tauri::command]
pub async fn objects_attributes(objects: Vec<ObjectId>, state: State<'_, AppState>) -> Result<Vec<Attributes>, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        Ok(objects.iter().map(|&id| project.object(id).map(|o| o.attributes.clone()).unwrap_or_default()).collect())
    })
    .await?
}

/// A solid as the block table shows it: its own volume, the volume of record, and every attribute.
#[derive(Serialize)]
pub struct SolidRow {
    volume: Option<f64>,
    attributes: Attributes,
}

/// Each solid's volume and attributes, in the order asked, for the block table (Workflow plan, stage 4); anything gone
/// or not a solid has neither.
#[tauri::command]
pub async fn solids_table(objects: Vec<ObjectId>, state: State<'_, AppState>) -> Result<Vec<SolidRow>, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        Ok(objects
            .iter()
            .map(|&id| match project.object(id) {
                Some(o) => match &o.body {
                    zd_core::Body::Solid(s) => SolidRow { volume: Some(s.volume()), attributes: o.attributes.clone() },
                    _ => SolidRow { volume: None, attributes: Attributes::default() },
                },
                None => SolidRow { volume: None, attributes: Attributes::default() },
            })
            .collect())
    })
    .await?
}

/// What a grid is of (Tables and scripts plan, decision 1): a table object, or objects as a table, a row an object.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "of")]
pub enum GridSource {
    Table { table: ObjectId },
    Objects { objects: Vec<ObjectId>, density: Option<String> },
}

/// Any table as a grid: grouped by its word columns, each group added up by every column's own rule, the leaves of the
/// groups opened.
#[tauri::command]
pub async fn table_grid(source: GridSource, ask: zd_commands::table_grid::GridAsk, state: State<'_, AppState>) -> Result<zd_commands::table_grid::Grid, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        match source {
            GridSource::Table { table } => match &project.object(table).ok_or("no such object")?.body {
                zd_core::Body::Table(t) => zd_commands::table_grid::grid_with(t, &ask, &|id| project.object(id).cloned()),
                _ => Err("not a table".to_string()),
            },
            GridSource::Objects { objects, density } => zd_commands::table_grid::grid(&zd_commands::table_grid::objects_table(project, &objects, density.as_deref()), &ask),
        }
    })
    .await?
}

/// A CSV matched to objects or a table (Tables and scripts plan, decision 5): what matches and what would change, before
/// anything is written. The run (`table.update_records`) matches the same way.
#[tauri::command]
pub async fn records_preview(params: zd_commands::builtin::UpdateRecordsParams, state: State<'_, AppState>) -> Result<zd_commands::builtin::RecordsReport, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        zd_commands::builtin::match_records(app.session.project(), &params).map(|(report, _)| report).map_err(|e| e.to_string())
    })
    .await?
}

/// A calculated column as edited, its formula's loads bound to the project's scripts by name (decision 0099).
fn bound_column(project: &zd_core::Project, mut column: zd_core::CalculatedColumn) -> Result<zd_core::CalculatedColumn, String> {
    column.scripts = zd_commands::builtin::bind_loads(project.objects(), &[column.script.as_str()])?.0;
    column.lookups = zd_commands::builtin::bind_lookups(project.objects(), &[column.script.as_str()])?.0;
    Ok(column)
}

/// A dataflow as edited, its formulas' loads bound to the project's scripts by name (decision 0099), so an edit can
/// load a script before it's saved; and those scripts, for the loader.
fn bound(project: &zd_core::Project, mut d: zd_core::Dataflow) -> Result<(zd_core::Dataflow, zd_commands::builtin::ScriptLibraries), String> {
    let sources: Vec<String> = d.steps.iter().filter_map(|s| s.script().map(String::from)).collect();
    let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
    let (ids, libraries) = zd_commands::builtin::bind_loads(project.objects(), &refs)?;
    d.scripts = ids;
    d.lookups = zd_commands::builtin::bind_lookups(project.objects(), &refs)?.0;
    Ok((d, libraries))
}

/// A script as edited, checked as a library against the project's other scripts (decision 0099): its problems, each on
/// its line. Nothing is kept.
#[tauri::command]
pub async fn script_check(name: String, source: String, script: Option<ObjectId>, state: State<'_, AppState>) -> Result<Vec<zd_commands::builtin::FormulaProblem>, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let others: Vec<(String, String)> = app
            .session
            .project()
            .objects()
            .filter(|o| Some(o.id) != script)
            .filter_map(|o| match &o.body {
                zd_core::Body::Script(s) => Some((o.name.clone(), s.source.clone())),
                _ => None,
            })
            .collect();
        let others = Arc::new(others);
        Ok(match zd_commands::builtin::check_library(name.trim(), &source, &others) {
            // A table script (decision 0099, workstream 7) is checked as one too: `def run(t, p):` and what it calls.
            Ok(()) => zd_commands::builtin::check_table_script(&source, others),
            Err(problems) => problems.into_iter().map(|p| zd_commands::builtin::FormulaProblem { message: p.message, line: p.line }).collect(),
        })
    })
    .await?
}

fn results_table(project: &zd_core::Project, id: ObjectId) -> Result<Arc<zd_core::Table>, String> {
    match &project.object(id).ok_or("no such object")?.body {
        zd_core::Body::Table(t) => Ok(t.clone()),
        _ => Err("not a table".to_string()),
    }
}

/// A calculated column as edited, checked against its table (Tables and scripts plan, decision 6): each problem on its
/// line, and the names it may use. Nothing is kept.
#[tauri::command]
pub async fn calc_check(object: ObjectId, column: zd_core::CalculatedColumn, state: State<'_, AppState>) -> Result<zd_commands::table_calc::CalcCheck, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let table = results_table(project, object)?;
        let column = bound_column(project, column)?;
        Ok(zd_commands::table_calc::check(&table, &column, &|id| project.object(id).cloned()))
    })
    .await?
}

/// A calculated column as edited, traced on one row of its table, line by line. Nothing is kept.
#[tauri::command]
pub async fn calc_trace(object: ObjectId, column: zd_core::CalculatedColumn, row: usize, label: String, state: State<'_, AppState>) -> Result<zd_commands::builtin::FormulaTraceView, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let table = results_table(project, object)?;
        if row >= table.rows() {
            return Err(format!("the table has {} rows", table.rows()));
        }
        let column = bound_column(project, column)?;
        let lookup = |id: ObjectId| project.object(id).cloned();
        let calc = zd_commands::table_calc::compile(&table, &column, &lookup).map_err(|p| p.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("; "))?;
        Ok(zd_commands::table_calc::trace_row(&table, &calc, row, &label))
    })
    .await?
}

/// The parameter tables a reserves table's dataflow reads, by id and name: what a new calculated column's `p` reads.
#[tauri::command]
pub async fn calc_parameters(object: ObjectId, state: State<'_, AppState>) -> Result<Vec<(ObjectId, String)>, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let Ok(p) = zd_commands::builtin::rerun_params(project, app.session.log(), object, &[]) else { return Ok(Vec::new()) };
        let Some(zd_core::Body::Dataflow(flow)) = project.object(p.dataflow).map(|o| &o.body) else { return Ok(Vec::new()) };
        Ok(flow.parameters.iter().filter_map(|&id| project.object(id).map(|o| (id, o.name.clone()))).collect())
    })
    .await?
}

/// What a reserves table was made from that has changed since its run (Tables and scripts plan, decision 4): blocks
/// revised (an attribute typed in a table), and anything else it read, by name. Nothing runs again by itself.
#[tauri::command]
pub async fn reserves_changed(object: ObjectId, state: State<'_, AppState>) -> Result<zd_commands::builtin::ChangedSince, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        zd_commands::builtin::changed_since(app.session.project(), app.session.log(), object).map_err(|e| e.to_string())
    })
    .await?
}

/// A reserves table's tree of levels (decision 0075): every node with how many leaves are under it.
#[tauri::command]
pub async fn reserves_tree(object: ObjectId, when: Option<std::collections::BTreeMap<String, String>>, state: State<'_, AppState>) -> Result<zd_commands::reserves_view::TreeNode, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        match &app.session.project().object(object).ok_or("no such object")?.body {
            zd_core::Body::Table(t) => Ok(zd_commands::reserves_view::tree_where(t, &when.unwrap_or_default())),
            _ => Err("not a table".to_string()),
        }
    })
    .await?
}

/// A reserves table's node broken down (decision 0075): its fields down the side, its total and children across, each
/// child in `open` followed by its own.
#[tauri::command]
pub async fn reserves_breakdown(object: ObjectId, node: Vec<String>, open: Vec<Vec<String>>, leaves: Option<bool>, when: Option<std::collections::BTreeMap<String, String>>, state: State<'_, AppState>) -> Result<zd_commands::reserves_view::Breakdown, String> {
    let app = state.app.clone();
    let when = when.unwrap_or_default();
    blocking(move || {
        let app = app.lock().unwrap();
        match &app.session.project().object(object).ok_or("no such object")?.body {
            zd_core::Body::Table(t) if leaves == Some(true) => zd_commands::reserves_view::breakdown_leaves_where(t, &node, &when),
            zd_core::Body::Table(t) => zd_commands::reserves_view::breakdown_where(t, &node, &open, &when),
            _ => Err("not a table".to_string()),
        }
    })
    .await?
}

/// A reserves table's leaves as attributes, a leaf a row: their levels' words, what the filter box counts to tick
/// (decision 0088, workstream 4).
#[tauri::command]
pub async fn reserves_filter_rows(object: ObjectId, state: State<'_, AppState>) -> Result<Vec<zd_core::Attributes>, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        match &app.session.project().object(object).ok_or("no such object")?.body {
            zd_core::Body::Table(t) => Ok(zd_commands::reserves_view::filter_rows(t)),
            _ => Err("not a table".to_string()),
        }
    })
    .await?
}

/// Open its column (decision 0075, 5b): the block column a reserves table's node sits in, run again through its dataflow
/// with the settings of the run that made the table, every unit after each step. Nothing is kept.
///
/// The notebook (decision 0081): each step with what it is, the units it took and the parameters it read; with
/// `dataflow`, run on that dataflow as edited, not the one saved; with `tables`, on those parameter tables as edited, each
/// in place of the saved one with its id (decision 0082, the Dataflow tab live).
#[tauri::command]
pub async fn reserves_column(
    object: ObjectId,
    node: Vec<String>,
    dataflow: Option<zd_core::Dataflow>,
    tables: Option<Vec<(ObjectId, zd_core::ParameterTable)>>,
    state: State<'_, AppState>,
) -> Result<zd_commands::builtin::ColumnSteps, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let dataflow = dataflow.map(|d| bound(app.session.project(), d).map(|(d, _)| d)).transpose()?;
        let unsaved = unsaved(dataflow, tables);
        zd_commands::builtin::table_column(app.session.project(), app.session.log(), object, &node, &unsaved).map_err(|e| e.to_string())
    })
    .await?
}

/// Every block of a reserves table's case at one step, as a grid (Tables and scripts plan, decision 10): what the
/// Dataflow tab shows for one block, for all of them, grouped and added up by each field's own rule.
#[derive(Serialize)]
pub struct StepGrid {
    #[serde(flatten)]
    units: zd_commands::builtin::StepUnits,
    grid: zd_commands::table_grid::Grid,
}

/// The step's units for every block, run on what's saved (`step` steps, 0 for the columns as input), then grouped. The
/// units are kept between asks while the project and the step stay the same, so opening or sorting a group doesn't run
/// every block again; nothing goes in the project.
#[tauri::command]
pub async fn reserves_step_grid(object: ObjectId, step: usize, ask: zd_commands::table_grid::GridAsk, state: State<'_, AppState>) -> Result<StepGrid, String> {
    let app = state.app.clone();
    blocking(move || {
        let mut app = app.lock().unwrap();
        let project = app.session.project();
        // The project as it stands: every object at its revision, and the log, which an edit to anything a run reads adds to.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for o in project.objects() {
            std::hash::Hash::hash(&(o.id, o.revision), &mut hasher);
        }
        let key = format!("{object}|{step}|{}|{}", std::hash::Hasher::finish(&hasher), app.session.log().len());
        let units = match &app.step_units {
            Some((k, units)) if *k == key => units.clone(),
            _ => {
                let units = Arc::new(zd_commands::builtin::units_at_step(project, app.session.log(), object, step, &zd_commands::builtin::Unsaved::default()).map_err(|e| e.to_string())?);
                app.step_units = Some((key, units.clone()));
                units
            }
        };
        let grid = zd_commands::table_grid::grid(&units.table, &ask)?;
        Ok(StepGrid { units: (*units).clone(), grid })
    })
    .await?
}

/// A notebook cell's formula traced on one unit (decision 0081): the column run up to the step (1, the first after the
/// column as input), the formula traced on the unit with that id as it stands there. Nothing is kept.
#[tauri::command]
pub async fn reserves_column_trace(
    object: ObjectId,
    node: Vec<String>,
    step: usize,
    unit: u32,
    dataflow: Option<zd_core::Dataflow>,
    tables: Option<Vec<(ObjectId, zd_core::ParameterTable)>>,
    state: State<'_, AppState>,
) -> Result<zd_commands::builtin::FormulaTraceView, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let dataflow = dataflow.map(|d| bound(app.session.project(), d).map(|(d, _)| d)).transpose()?;
        let unsaved = unsaved(dataflow, tables);
        zd_commands::builtin::column_trace(app.session.project(), app.session.log(), object, &node, step, unit, &unsaved).map_err(|e| e.to_string())
    })
    .await?
}

/// The settings to run a reserves table's blocks under `only` again into it (decision 0085): its own run's, the table and
/// the run that last wrote it named. Run with `reserves.run`, as any command, one undo.
#[tauri::command]
pub async fn reserves_rerun_params(object: ObjectId, only: Vec<String>, state: State<'_, AppState>) -> Result<zd_commands::builtin::RunReservesParams, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        zd_commands::builtin::rerun_params(app.session.project(), app.session.log(), object, &only).map_err(|e| e.to_string())
    })
    .await?
}

/// A dataflow and parameter tables as edited, for a column run on them.
fn unsaved(dataflow: Option<zd_core::Dataflow>, tables: Option<Vec<(ObjectId, zd_core::ParameterTable)>>) -> zd_commands::builtin::Unsaved {
    zd_commands::builtin::Unsaved {
        dataflow: dataflow.map(std::sync::Arc::new),
        tables: tables.unwrap_or_default().into_iter().map(|(id, t)| (id, std::sync::Arc::new(t))).collect(),
    }
}

/// The parameter tables read with one, in order, and where a picked solid takes its values from (decision 0075, 5d).
#[derive(Serialize)]
pub struct ParametersPicked {
    /// The tables read: a dataflow's that reads this one, in its order, or this one alone.
    tables: Vec<(ObjectId, String)>,
    /// The dataflow they're read by, if any.
    dataflow: Option<String>,
    picked: Option<zd_core::Picked>,
}

/// The tables read with a parameter table (those of the first dataflow that reads it, in its order, or it alone) and, for
/// a picked solid, the rows it fits and the row each value comes from.
#[tauri::command]
pub async fn parameters_picked(table: ObjectId, solid: Option<ObjectId>, state: State<'_, AppState>) -> Result<ParametersPicked, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let reader = project.objects().find_map(|o| match &o.body {
            zd_core::Body::Dataflow(d) if d.parameters.contains(&table) => Some((o.name.clone(), d.parameters.clone())),
            _ => None,
        });
        let (dataflow, ids) = match reader {
            Some((name, ids)) => (Some(name), ids),
            None => (None, vec![table]),
        };
        let mut tables = Vec::new();
        let mut bodies = Vec::new();
        for id in ids {
            let Some(o) = project.object(id) else { continue };
            if let zd_core::Body::ParameterTable(t) = &o.body {
                tables.push((id, o.name.clone()));
                bodies.push(t.clone());
            }
        }
        let refs: Vec<&zd_core::ParameterTable> = bodies.iter().map(|t| &**t).collect();
        let picked = solid.and_then(|s| project.object(s)).filter(|s| matches!(s.body, zd_core::Body::Solid(_))).map(|s| zd_core::ParameterTable::picked(&refs, &s.attributes));
        Ok(ParametersPicked { tables, dataflow, picked })
    })
    .await?
}

/// Whether a parameter table as edited holds together, before it's saved: the engine's own check.
#[tauri::command]
pub fn parameters_check(content: zd_core::ParameterTable) -> Result<(), String> {
    content.check()
}

/// Whether a dataflow as edited holds together, before it's saved: the engine's own check, its formula steps compiled
/// against its fields and parameter tables and their worked examples run (decision 0077), as `dataflow.save` will.
#[tauri::command]
pub async fn dataflow_check(content: zd_core::Dataflow, state: State<'_, AppState>) -> Result<(), String> {
    content.check()?;
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let tables: Vec<std::sync::Arc<zd_core::ParameterTable>> = content
            .parameters
            .iter()
            .filter_map(|id| match project.object(*id).map(|o| &o.body) {
                Some(zd_core::Body::ParameterTable(t)) => Some(t.clone()),
                _ => None,
            })
            .collect();
        let refs: Vec<&zd_core::ParameterTable> = tables.iter().map(|t| &**t).collect();
        let fields = content.engine_fields()?;
        let built_in: Vec<&str> = zd_commands::builtin::STEPS.iter().map(|(n, _)| *n).collect();
        let (content, libraries) = bound(project, content)?;
        let mut declared = zd_commands::builtin::formula_declared(&fields, &refs);
        zd_commands::builtin::declare_words(&mut declared, &content.words);
        declared.libraries = libraries;
        zd_commands::builtin::check_formula_steps(&content, &built_in, &declared).map(|_| ())
    })
    .await?
}

/// A formula step of a dataflow as edited, checked (decision 0077, 7d): where it sits, every problem with its line, its
/// known answers, and the names it may use.
#[tauri::command]
pub async fn formula_check(content: zd_core::Dataflow, index: usize, state: State<'_, AppState>) -> Result<zd_commands::builtin::FormulaReport, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let tables: Vec<std::sync::Arc<zd_core::ParameterTable>> = content
            .parameters
            .iter()
            .filter_map(|id| match project.object(*id).map(|o| &o.body) {
                Some(zd_core::Body::ParameterTable(t)) => Some(t.clone()),
                _ => None,
            })
            .collect();
        let refs: Vec<&zd_core::ParameterTable> = tables.iter().map(|t| &**t).collect();
        let built_in: Vec<&str> = zd_commands::builtin::STEPS.iter().map(|(n, _)| *n).collect();
        // A load of a name the project hasn't is said on the formula, as any problem is.
        let libraries = match bound(project, content.clone()) {
            Ok((_, libraries)) => libraries,
            Err(_) => Default::default(),
        };
        // The tables it looks up by name, so its worked examples can look them up as a run does.
        let named = zd_commands::builtin::bind_lookups(project.objects(), &content.steps.iter().filter_map(|s| s.script()).collect::<Vec<_>>()).map(|(_, named)| named).unwrap_or_default();
        zd_commands::builtin::check_formula_with(&content, &refs, index, &built_in, libraries, named)
    })
    .await?
}

/// A formula step of a dataflow as edited, traced on a picked solid (decision 0077, 7d): the solid's block column run
/// up to the step, with the blocks on the solid's layer and the project's seam model, then the formula traced on its
/// unit. Nothing is kept.
#[tauri::command]
pub async fn formula_trace(content: zd_core::Dataflow, step: String, solid: ObjectId, state: State<'_, AppState>) -> Result<zd_commands::builtin::FormulaTraceView, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let picked = project.object(solid).ok_or("the picked solid isn't in the project")?;
        let blocks: Vec<ObjectId> = project.objects().filter(|o| o.layer == picked.layer && matches!(o.body, zd_core::Body::Solid(_))).map(|o| o.id).collect();
        let model = project.objects().find(|o| matches!(o.body, zd_core::Body::SeamModel(_))).map(|o| o.id).ok_or("the project has no seam model to put the column in order")?;
        let p = zd_commands::builtin::RunReservesParams { dataflow: ObjectId::new(), blocks, model: Some(model), stack: Vec::new(), layer: "trace".into(), parameters: None, name: String::new(), table: None, only: Vec::new(), previous: None };
        let get = |id: ObjectId| project.object(id).cloned().ok_or(zd_commands::CommandError::MissingObject(id));
        let (content, _) = bound(project, content)?;
        zd_commands::builtin::formula_trace(&get, &std::sync::Arc::new(content), &p, &step, solid).map_err(|e| e.to_string())
    })
    .await?
}

/// How to write a formula: the formula panel's help and what Copy for AI hands over (decision 0077, 7d).
#[tauri::command]
pub fn formula_reference() -> &'static str {
    zd_commands::builtin::FORMULA_REFERENCE
}

/// What a script can call, for the script editors' completions and hover help: each list of calls with how each is
/// called and what it gives (zd-script's `help`, tested against what runs them).
#[tauri::command]
pub fn script_help() -> serde_json::Value {
    use zd_commands::builtin::script_help as h;
    let list = |calls: &[h::Call]| calls.iter().map(|c| serde_json::json!({ "name": c.name, "signature": c.signature, "what": c.what })).collect::<Vec<_>>();
    serde_json::json!({
        "table": list(h::TABLE_CALLS),
        "tableGlobals": list(h::TABLE_GLOBALS),
        "library": list(h::LIBRARY_CALLS),
        "unitParts": list(h::UNIT_PARTS),
        "column": list(h::COLUMN_CALLS),
        "allocationUnit": list(h::ALLOCATION_UNIT),
        "starlark": list(h::STARLARK),
    })
}

/// The solids a reserves table's node was made from (decision 0075): every solid of the table's case on a leaf the table
/// holds, under the node, and with `when` on a leaf the table's filter keeps (decision 0090). To select or isolate them.
#[tauri::command]
pub async fn reserves_solids(object: ObjectId, node: Vec<String>, when: Option<std::collections::BTreeMap<String, String>>, state: State<'_, AppState>) -> Result<Vec<ObjectId>, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let table = project.object(object).ok_or("no such object")?;
        let zd_core::Body::Table(t) = &table.body else { return Err("not a table".to_string()) };
        let leaves = zd_core::leaves::TableLeaves::new(t, &table.attributes, &when.unwrap_or_default());
        Ok(leaves.solids(&node, project.objects().filter(|o| matches!(o.body, zd_core::Body::Solid(_))).map(|o| (o.id, &o.attributes))))
    })
    .await?
}

/// A reserves field's value for each block solid of the table's case (decision 0075, point 7): its leaf's, or with
/// `by_block` its block's, added up by the field's rule. For colouring the blocks in the view.
#[tauri::command]
pub async fn reserves_field_values(object: ObjectId, column: String, by_block: bool, state: State<'_, AppState>) -> Result<Vec<(ObjectId, f64)>, String> {
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let table = project.object(object).ok_or("no such object")?;
        let zd_core::Body::Table(t) = &table.body else { return Err("not a table".to_string()) };
        let solids = project.objects().filter(|o| matches!(o.body, zd_core::Body::Solid(_))).map(|o| (o.id, &o.attributes));
        Ok(zd_commands::reserves_view::values_by_solid(t, &table.attributes, &column, by_block, solids))
    })
    .await?
}

/// Where a solid sits in a reserves table (decision 0075): its levels' words as the run placed it, if it's of the table's
/// case and the table holds that leaf.
#[tauri::command]
pub async fn reserves_node_of(object: ObjectId, solid: ObjectId, state: State<'_, AppState>) -> Result<Option<Vec<String>>, String> {
    use zd_commands::reserves_view::{case_of, leaf_of, level_defaults, levels};
    let app = state.app.clone();
    blocking(move || {
        let app = app.lock().unwrap();
        let project = app.session.project();
        let table = project.object(object).ok_or("no such object")?;
        let zd_core::Body::Table(t) = &table.body else { return Err("not a table".to_string()) };
        let Some(s) = project.object(solid) else { return Ok(None) };
        if !matches!(s.body, zd_core::Body::Solid(_)) || case_of(&s.attributes) != case_of(&table.attributes) {
            return Ok(None);
        }
        let levels = levels(t);
        let leaf = leaf_of(&levels, &level_defaults(&table.attributes), &s.attributes);
        let held = (0..t.rows()).any(|i| {
            levels.iter().zip(&leaf).all(|(l, w)| match t.column(l).map(|c| &c.values) {
                Some(zd_core::ColumnValues::Text(v)) => v[i].as_deref().unwrap_or("") == w,
                _ => w.is_empty(),
            })
        });
        Ok(held.then_some(leaf))
    })
    .await?
}

/// A layer of a Deswik drawing, to pick from before `deswik.import`.
#[derive(Serialize)]
pub struct DeswikLayerRow {
    name: String,
    bytes: u64,
}

/// The layers of a Deswik `.dcf`, from its zip's directory alone (decision 0036).
#[tauri::command]
pub async fn deswik_layers(path: String) -> Result<Vec<DeswikLayerRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let layers = list_deswik_layers(Path::new(&path)).map_err(|e| format!("{path}: {e}"))?;
        Ok(layers.into_iter().map(|l| DeswikLayerRow { name: l.name, bytes: l.bytes }).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A Solid field of a Spry project, to pick from before `spry.import`.
#[derive(Serialize)]
pub struct SprySolidFieldRow {
    name: String,
    solids: usize,
    bytes: u64,
}

/// What a Spry project holds to import.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpryContentsRow {
    database_version: i64,
    solid_fields: Vec<SprySolidFieldRow>,
    layers: Vec<DeswikLayerRow>,
}

/// The Solid fields and design layers of a Spry `.spry`, from its tables alone (decision 0039).
#[tauri::command]
pub async fn spry_contents(path: String) -> Result<SpryContentsRow, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let contents = list_spry(Path::new(&path)).map_err(|e| format!("{path}: {e}"))?;
        Ok(SpryContentsRow {
            database_version: contents.database_version,
            solid_fields: contents.solid_fields.into_iter().map(|f| SprySolidFieldRow { name: f.name, solids: f.solids, bytes: f.bytes }).collect(),
            layers: contents.layers.into_iter().map(|l| DeswikLayerRow { name: l.name, bytes: l.bytes }).collect(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct StagedMesh {
    /// Pass to `fetch_mesh_shared` or `fetch_mesh_ipc`.
    handle: u32,
    name: String,
    /// `grid` or `triangulation`: which compact layout the buffer uses.
    kind: &'static str,
    triangles: usize,
    bytes: usize,
    log: String,
    timings: Value,
}

/// Encodes a drawn object's geometry for the viewport and holds it to be fetched.
#[tauri::command]
pub async fn stage_object(id: ObjectId, state: State<'_, AppState>) -> Result<StagedMesh, String> {
    let app = state.app.clone();
    let (geometry, name, origin) = {
        let mut app = app.lock().unwrap();
        let origin = app.origin().unwrap_or([0.0; 3]);
        let object = app.session.project().object(id).ok_or("no such object")?.clone();
        if !is_drawn_surface(&object) {
            return Err(format!("'{}' is not drawn as a surface", object.name));
        }
        (object.body.clone(), object.name.clone(), origin)
    };
    let header = serde_json::json!({ "id": 0, "name": name, "timings": {} });
    let (buffer, kind, triangles, encode_ms) = blocking(move || {
        let t = Instant::now();
        let (buffer, kind, triangles) = match &geometry {
            Body::Surface(s) => (encode_triangulation(s.mesh(), origin, header), "triangulation", s.mesh().triangle_count()),
            Body::Solid(s) => (encode_triangulation(s.mesh(), origin, header), "triangulation", s.mesh().triangle_count()),
            Body::Grid(g) => {
                let buffer = encode_grid(g, origin, header);
                (buffer, "grid", 2 * (g.cols - 1) * (g.rows - 1))
            }
            Body::String(_) | Body::SeamModel(_) | Body::FaceDesign(_) | Body::Workflow(_) | Body::ParameterTable(_) | Body::Dataflow(_) | Body::Table(_) | Body::Script(_) => unreachable!("only drawn surfaces are staged"),
        };
        (buffer, kind, triangles, ms(t))
    })
    .await?;
    Ok(stage(&state, buffer, name, kind, triangles, String::new(), serde_json::json!({ "encode": encode_ms })))
}

/// Makes the coarser versions of a drawn surface's chunks (decision 0097, workstream 2) and holds them to be fetched,
/// after the surface is drawn at full detail, so the view never waits on them. Chunk k is chunk k of what
/// `stage_object` sent. A grid's versions aren't made yet: it's refused, and drawn at full detail as before.
#[tauri::command]
pub async fn stage_levels(id: ObjectId, state: State<'_, AppState>) -> Result<StagedMesh, String> {
    let app = state.app.clone();
    let (geometry, name, origin) = {
        let mut app = app.lock().unwrap();
        let origin = app.origin().unwrap_or([0.0; 3]);
        let object = app.session.project().object(id).ok_or("no such object")?.clone();
        if !is_drawn_surface(&object) {
            return Err(format!("'{}' is not drawn as a surface", object.name));
        }
        (object.body.clone(), object.name.clone(), origin)
    };
    let header = serde_json::json!({ "id": 0, "name": name, "timings": {} });
    let (buffer, triangles, encode_ms) = blocking(move || {
        let t = Instant::now();
        let mesh = match &geometry {
            Body::Surface(s) => s.mesh(),
            Body::Solid(s) => s.mesh(),
            _ => return Err("only a triangulated surface or solid has coarser versions yet".to_string()),
        };
        Ok((encode_levels(mesh, origin, header), mesh.triangle_count(), ms(t)))
    })
    .await??;
    Ok(stage(&state, buffer, name, "levels", triangles, String::new(), serde_json::json!({ "encode": encode_ms })))
}

/// Encodes a tile of a layer's solids as one mesh for the viewport and holds it to be fetched: a pit's tens of
/// thousands of solids are then a few chunks a tile to draw, not one mesh each, and a change to one sends its tile again
/// (decision 0060, workstream 4). Hidden solids are in it too, each vertex saying which solid it's of, so hiding or
/// picking one changes a table beside the mesh and not the mesh (workstream 3).
#[tauri::command]
pub async fn stage_layer_solids(layer: LayerId, tile: [i64; 2], state: State<'_, AppState>) -> Result<StagedMesh, String> {
    let app = state.app.clone();
    let (solids, name, origin) = {
        let mut app = app.lock().unwrap();
        let origin = app.origin().unwrap_or([0.0; 3]);
        let AppInner { session, tiles, .. } = &mut *app;
        let project = session.project();
        let name = project.layer(layer).ok_or("no such layer")?.name.clone();
        (tile_solids(project, tiles, TileId { layer, col: tile[0], row: tile[1] }), name, origin)
    };
    let header = serde_json::json!({ "id": 0, "name": name, "timings": {} });
    let (buffer, triangles, encode_ms) = blocking(move || {
        let t = Instant::now();
        let (ids, meshes): (Vec<ObjectId>, Vec<Arc<TriMesh>>) = solids.into_iter().unzip();
        let triangles = meshes.iter().map(|m| m.triangle_count()).sum();
        (encode_solids(&ids, &meshes, origin, header), triangles, ms(t))
    })
    .await?;
    Ok(stage(&state, buffer, name, "triangulation", triangles, String::new(), serde_json::json!({ "encode": encode_ms })))
}

/// Makes the coarser versions of a tile of a layer's solids (decision 0097, workstream 4) and holds them to be fetched,
/// after the tile is drawn at full detail. Chunk k is chunk k of what `stage_layer_solids` sent for the tile; block
/// walls stay as they are, and the shared walls' chunks have no versions.
#[tauri::command]
pub async fn stage_layer_levels(layer: LayerId, tile: [i64; 2], state: State<'_, AppState>) -> Result<StagedMesh, String> {
    let app = state.app.clone();
    let (solids, name, origin) = {
        let mut app = app.lock().unwrap();
        let origin = app.origin().unwrap_or([0.0; 3]);
        let AppInner { session, tiles, .. } = &mut *app;
        let project = session.project();
        let name = project.layer(layer).ok_or("no such layer")?.name.clone();
        (tile_solids(project, tiles, TileId { layer, col: tile[0], row: tile[1] }), name, origin)
    };
    let header = serde_json::json!({ "id": 0, "name": name, "timings": {} });
    let (buffer, triangles, encode_ms) = blocking(move || {
        let t = Instant::now();
        let meshes: Vec<Arc<TriMesh>> = solids.into_iter().map(|(_, m)| m).collect();
        let triangles = meshes.iter().map(|m| m.triangle_count()).sum();
        (encode_solid_levels(&meshes, origin, header), triangles, ms(t))
    })
    .await?;
    Ok(stage(&state, buffer, name, "levels", triangles, String::new(), serde_json::json!({ "encode": encode_ms })))
}

/// The box round these objects together, world coordinates, for zoom to selection. `None` if none has a place.
#[tauri::command]
pub async fn objects_bounds(objects: Vec<ObjectId>, state: State<'_, AppState>) -> Result<Option<[[f64; 3]; 2]>, String> {
    let app = state.app.lock().unwrap();
    Ok(selection::bounds_of(app.session.project(), &objects).map(|b| [b.min, b.max]))
}

/// The solids among these objects holding less than `volume` (m³), in the order given: the filter that picks out small
/// pieces to look at and delete by hand (decision 0062, its seventh: no automatic limit, as decided in Phase 1).
#[tauri::command]
pub async fn solids_under(objects: Vec<ObjectId>, volume: f64, state: State<'_, AppState>) -> Result<Vec<ObjectId>, String> {
    let app = state.app.lock().unwrap();
    let project = app.session.project();
    Ok(objects
        .into_iter()
        .filter(|id| matches!(project.object(*id).map(|o| &o.body), Some(zd_core::Body::Solid(solid)) if solid.volume() < volume))
        .collect())
}

/// What slicing these solids into strips by these faces would refuse, found before the cut and nothing cut (decision
/// 0049): each face, joined by strip as the slice joins it, against each solid, past its ends, above its top and below
/// its floor. Read off the project as it stands while the app carries on.
#[tauri::command]
pub async fn check_slice(solids: Vec<ObjectId>, faces: Vec<ObjectId>, state: State<'_, AppState>) -> Result<Vec<zd_commands::Note>, String> {
    let project = state.app.lock().unwrap().session.shared_project();
    blocking(move || {
        let params = serde_json::from_value(serde_json::json!({ "solids": solids, "faces": faces, "layer": "strips" })).map_err(|e| e.to_string())?;
        zd_commands::builtin::check_slice(&project, &params).map_err(|e| e.to_string())
    })
    .await?
}

/// Where block polygons leave the solids uncovered in plan, found before a cut by polygons: a warning for each part of a
/// solid outside them all. Read off the project as it stands while the app carries on.
#[tauri::command]
pub async fn check_cut_by_polygons(solids: Vec<ObjectId>, polygons: Vec<ObjectId>, state: State<'_, AppState>) -> Result<Vec<zd_commands::Note>, String> {
    let project = state.app.lock().unwrap().session.shared_project();
    blocking(move || {
        let params = serde_json::from_value(serde_json::json!({ "solids": solids, "polygons": polygons, "layer": "blocks" })).map_err(|e| e.to_string())?;
        zd_commands::builtin::check_cut_by_polygons(&project, &params).map_err(|e| e.to_string())
    })
    .await?
}

/// Where a seam model's grids don't reach over the solids, found before a cut by seams: a warning for each part of a
/// solid past their edge or over a hole in them. Reads the model's grids in place, while the app carries on.
#[tauri::command]
pub async fn check_cut_by_seams(solids: Vec<ObjectId>, model: ObjectId, state: State<'_, AppState>) -> Result<Vec<zd_commands::Note>, String> {
    let project = state.app.lock().unwrap().session.shared_project();
    blocking(move || {
        let params = serde_json::from_value(serde_json::json!({ "solids": solids, "model": model, "layer": "layers" })).map_err(|e| e.to_string())?;
        zd_commands::builtin::check_cut_by_seams(&project, &params).map_err(|e| e.to_string())
    })
    .await?
}

/// Everything on show lying wholly inside a box drawn on the viewport: the camera's view-projection matrix in the
/// scene's local frame (column-major, as three.js keeps it) and the box's corners in normalised device coordinates.
#[tauri::command]
pub async fn objects_in_box(matrix: [f64; 16], min: [f64; 2], max: [f64; 2], state: State<'_, AppState>) -> Result<Vec<ObjectId>, String> {
    let app = state.app.clone();
    blocking(move || {
        let mut app = app.lock().unwrap();
        let origin = app.origin().unwrap_or([0.0; 3]);
        selection::objects_in_box(app.session.project(), &selection::ScreenBox { matrix, origin, min, max })
    })
    .await
}

/// The section through two plan points (mine grid) as binary: where the plane cuts every visible
/// layer, with strings within `width` drawn flat on it and the `selected` object's lines over the rest.
/// View state, not a command.
#[tauri::command]
pub async fn section_lines(a: [f64; 2], b: [f64; 2], width: f64, fill: bool, selected: Option<ObjectId>, state: State<'_, AppState>) -> Result<Response, String> {
    let app = state.app.clone();
    blocking(move || {
        let start = Instant::now();
        let plane = SectionPlane::through(a, b).map_err(|e| e.to_string())?;
        let mut app = app.lock().unwrap();
        let mut groups = section_groups(&mut app, &plane, width, fill)?;
        if let Some(id) = selected {
            add_selected(&app, &plane, width, &mut groups, id);
        }
        let origin = app.origin().unwrap_or([0.0; 3]);
        let encoded = encode_section(&groups, origin, &plane, ms(start));
        app.section_cut = Some(SectionCut { plane, groups });
        Ok(Response::new(encoded))
    })
    .await?
}

/// One horizon of a seam model, staged for the viewport: its roof coloured by thickness.
#[derive(Serialize)]
pub struct StagedHorizon {
    staged: StagedMesh,
    horizon: String,
    /// The thickest coal on the horizon (m): the top of the colour ramp.
    thickness_max: f64,
    coal_nodes: usize,
    nodes: usize,
    /// Nodes marked for an issue beyond the model's tolerance.
    flagged_nodes: usize,
    /// The worst node of each blocking issue on this horizon, in scene-local coordinates.
    markers: Vec<IssueMarker>,
    /// Grid files that have changed since the model was built.
    changed: Vec<String>,
    /// Every horizon, top first, to choose from.
    horizons: Vec<HorizonChoice>,
    /// The grid the horizon is coloured by, by its code; none for its thickness.
    variable: Option<String>,
    /// The values the colours span: the thickness from 0, or the grid's lowest to highest where there's coal.
    value_range: [f64; 2],
}

#[derive(Serialize)]
pub struct HorizonChoice {
    code: String,
    material: String,
    working: bool,
    /// Its grids besides roof and floor, by code: what it can be coloured by.
    variables: Vec<String>,
}

#[derive(Serialize)]
pub struct IssueMarker {
    /// Index into the model's issues.
    issue: usize,
    position: [f64; 3],
}

/// Reads a seam model's roofs and floors in place and stages one horizon's roof (the first working
/// horizon if none is named) with its thickness per node, marking the nodes of blocking issues; or, with `variable`, one of
/// its grids per node where it holds coal, to check its blocks by eye against (decision 0061, workstream 6).
#[tauri::command]
pub async fn stage_seam_horizon(id: ObjectId, horizon: Option<String>, variable: Option<String>, state: State<'_, AppState>) -> Result<StagedHorizon, String> {
    let (model, name, origin, key) = {
        let mut app = state.app.lock().unwrap();
        let origin = app.origin().unwrap_or([0.0; 3]);
        let object = app.session.project().object(id).ok_or("no such object")?.clone();
        let Body::SeamModel(model) = &object.body else { return Err(format!("'{}' is not a seam model", object.name)) };
        (model.clone(), object.name.clone(), origin, geometry_key(&object.body))
    };
    let code = match horizon {
        Some(code) => code,
        None => model.working_section().next().or(model.horizons.first()).ok_or("the model has no horizons")?.code.clone(),
    };
    let title = name.clone();
    let coloured_by = variable.clone();
    let horizons = model.horizons.iter().map(|h| HorizonChoice { code: h.code.clone(), material: h.material.clone(), working: h.working, variables: h.grids.keys().cloned().collect() }).collect();
    let (buffer, triangles, staged_horizon, encode_ms, caster) = blocking(move || -> Result<_, String> {
        let t = Instant::now();
        let index = model.horizons.iter().position(|h| h.code == code).ok_or_else(|| format!("no horizon '{code}' in the model"))?;
        let grids = read_stack_grids(&model).map_err(|e| e.to_string())?;
        let parent = |p: &str| model.horizons.iter().position(|h| h.code.eq_ignore_ascii_case(p));
        let stack: Vec<StackHorizon<'_>> = model
            .horizons
            .iter()
            .enumerate()
            .map(|(i, h)| StackHorizon {
                code: &h.code,
                parent: h.parent.as_deref().and_then(parent),
                working: h.working,
                roof: &grids.roofs[i],
                floor: &grids.floors[i],
            })
            .collect();
        let flags = stack_issue_nodes(&stack, model.tolerance).map_err(|e| e.to_string())?.swap_remove(index);
        let (roof, floor) = (&grids.roofs[index], &grids.floors[index]);
        let thickness: Vec<f64> = roof.z.iter().zip(&floor.z).map(|(r, f)| if r - f > model.tolerance { r - f } else { f64::NAN }).collect();
        let thickness_max = thickness.iter().copied().filter(|t| !t.is_nan()).fold(0.0, f64::max);
        let markers = model
            .issues
            .iter()
            .enumerate()
            .filter(|(_, i)| i.blocking && i.horizon == code)
            .filter_map(|(issue, i)| {
                let [x, y] = i.at?;
                let col = ((x - roof.origin[0]) / roof.cell_size[0]).round() as usize;
                let row = ((y - roof.origin[1]) / roof.cell_size[1]).round() as usize;
                let z = roof.z.get(roof.index(col.min(roof.cols - 1), row.min(roof.rows - 1))).copied().filter(|z| !z.is_nan())?;
                Some(IssueMarker { issue, position: [x - origin[0], y - origin[1], z - origin[2]] })
            })
            .collect();
        // A grid of the horizon, where it holds coal, in place of its thickness.
        let chosen: Option<(String, Vec<f64>)> = match &variable {
            None => None,
            Some(v) => {
                let link = model.horizons[index].grids.get(v).ok_or_else(|| format!("horizon '{code}' has no grid '{v}'"))?;
                let grid = zd_io::read_linked_grid(&model.path(link)).map_err(|e| e.to_string())?.grid;
                if !grid.same_lattice(roof) {
                    return Err(format!("{v} of '{code}' isn't on the model's lattice"));
                }
                Some((v.clone(), grid.z.iter().zip(&thickness).map(|(z, t)| if t.is_nan() { f64::NAN } else { *z }).collect()))
            }
        };
        let (shown, range): (&[f64], [f64; 2]) = match &chosen {
            None => (&thickness, [0.0, thickness_max]),
            Some((_, values)) => {
                let live = values.iter().copied().filter(|z| z.is_finite());
                let range = live.fold([f64::INFINITY, f64::NEG_INFINITY], |[lo, hi], z| [lo.min(z), hi.max(z)]);
                (values, if range[0] <= range[1] { range } else { [0.0, 0.0] })
            }
        };
        // A grid's own values carry no issues to mark: the stack's are on the thickness.
        let no_flags = vec![false; shown.len()];
        let values = NodeValues { name: if chosen.is_some() { "variable" } else { "thickness" }, values: shown, flagged: if chosen.is_some() { &no_flags } else { &flags }, range };
        let header = serde_json::json!({ "id": 0, "name": format!("{title} {code}"), "timings": {} });
        let buffer = encode_grid_with(roof, Some(&values), origin, header);
        let triangles = 2 * (roof.cols - 1) * (roof.rows - 1);
        let horizon = (code, thickness_max, thickness.iter().filter(|t| !t.is_nan()).count(), roof.z.len(), flags.iter().filter(|&&f| f).count(), markers, grids.changed, range);
        // The roof drawn is what a press on the model snaps and turns about (decision 0044): its index, as for a grid.
        let caster = SurfaceRayCaster::new(&triangulate_grid(roof)).ok();
        Ok((buffer, triangles, horizon, ms(t), caster))
    })
    .await??;
    // Kept with the model's own key, so it goes when the model changes or is deleted, and is passed over while hidden.
    {
        let mut app = state.app.lock().unwrap();
        match caster {
            Some(caster) => app.casters.insert(id, (key, Arc::new(caster))),
            None => app.casters.remove(&id),
        };
    }
    let (code, thickness_max, coal_nodes, nodes, flagged_nodes, markers, changed, value_range) = staged_horizon;
    let log = if changed.is_empty() { String::new() } else { format!("{} grid file(s) changed since the model was built", changed.len()) };
    let staged = stage(&state, buffer, format!("{name} {code}"), "grid", triangles, log, serde_json::json!({ "read_and_encode": encode_ms }));
    Ok(StagedHorizon { staged, horizon: code, thickness_max, coal_nodes, nodes, flagged_nodes, markers, changed, horizons, variable: coloured_by, value_range })
}

/// Generates a synthetic topography of `cells` x `cells` and stages it in the `grid` or
/// `triangulation` layout, so the transfer check can time the hand-over alone.
#[tauri::command]
pub async fn prepare_synthetic(cells: usize, kind: String, state: State<'_, AppState>) -> Result<StagedMesh, String> {
    let origin = state.app.lock().unwrap().origin().unwrap_or([650_000.0, 7_450_000.0, 200.0]);
    let as_grid = match kind.as_str() {
        "grid" => true,
        "triangulation" => false,
        other => return Err(format!("unknown layout '{other}'")),
    };
    let name = format!("synthetic {cells}x{cells}");
    let header = serde_json::json!({ "id": 0, "name": name, "timings": {} });
    let (buffer, triangles, encode_ms) = blocking(move || {
        let grid = synthetic_topography(cells);
        let t = Instant::now();
        let buffer = if as_grid {
            encode_grid(&grid, origin, header)
        } else {
            encode_triangulation(&triangulate_grid(&grid), origin, header)
        };
        (buffer, 2 * cells * cells, ms(t))
    })
    .await?;
    let kind = if as_grid { "grid" } else { "triangulation" };
    Ok(stage(&state, buffer, name, kind, triangles, String::new(), serde_json::json!({ "encode": encode_ms })))
}

/// Hands a staged buffer to the page through a WebView2 shared buffer. The page receives it as a
/// `sharedbufferreceived` event whose additional data is `{"handle": <handle>}`.
#[tauri::command]
pub async fn fetch_mesh_shared(handle: u32, webview: Webview, state: State<'_, AppState>) -> Result<(), String> {
    let buffer = take_staged(&state, handle)?;
    let data = serde_json::json!({ "handle": handle }).to_string();
    blocking(move || post_shared_buffer(&webview, buffer, data)).await?
}

/// Hands a staged buffer to the page as raw bytes over the IPC bridge (the fallback path).
#[tauri::command]
pub fn fetch_mesh_ipc(handle: u32, state: State<'_, AppState>) -> Result<Response, String> {
    Ok(Response::new(take_staged(&state, handle)?))
}

#[derive(Serialize)]
pub struct SnapIndex {
    ms: f64,
    surfaces: usize,
    triangles: usize,
}

/// Builds the snap index for every drawn surface and layer of solids whose geometry has none yet, and drops indexes of
/// objects and layers that are gone. With `shown` (as the app asks after every change), only what's on show: a layer
/// shown later is indexed then. Else that first, then the rest. Surfaces go before solids, so the ground snaps first, a
/// few at a time on the cores (a big surface splits across them itself, `zd_geom::PIECE_TRIANGLES`); each goes in as
/// soon as it's built.
#[tauri::command]
pub async fn build_snap_index(shown: Option<bool>, state: State<'_, AppState>) -> Result<SnapIndex, String> {
    enum Source {
        Mesh(ObjectId, usize, Arc<TriMesh>),
        Grid(ObjectId, usize, Arc<zd_core::Grid>),
        Solids(TileId, String, Vec<(ObjectId, Arc<TriMesh>)>),
    }
    enum Built {
        Object(ObjectId, usize, SurfaceRayCaster),
        Layer(TileId, String, SolidCaster),
    }
    let pending: Vec<(bool, Source)> = {
        let mut app = state.app.lock().unwrap();
        let live: std::collections::HashSet<ObjectId> = app.session.project().objects().map(|o| o.id).collect();
        app.casters.retain(|id, _| live.contains(id));
        let keys = app.solid_keys.clone();
        app.solid_casters.retain(|tile, _| keys.contains_key(tile));
        // Each tile whose index is missing or stale, with its solids: only the tiles that changed are built again.
        let stale: Vec<(TileId, String)> = keys.iter().filter(|(tile, key)| app.solid_casters.get(tile).is_none_or(|(built, _)| built != *key)).map(|(t, k)| (*t, k.clone())).collect();
        let tiles: Vec<(bool, Source)> = {
            let AppInner { session, tiles, .. } = &mut *app;
            let project = session.project();
            stale.into_iter().map(|(tile, key)| (project.layer(tile.layer).is_some_and(|l| l.visible), Source::Solids(tile, key, tile_solids(project, tiles, tile)))).collect()
        };
        let project = app.session.project();
        let on_show = |o: &zd_core::Object| !project.is_concealed(o.id) && project.layer(o.layer).is_some_and(|l| l.visible);
        let objects = project
            .objects()
            .filter(|o| is_drawn_surface(o) && !matches!(o.body, Body::Solid(_)))
            .filter(|o| app.casters.get(&o.id).is_none_or(|(key, _)| *key != geometry_key(&o.body)))
            .map(|o| {
                let source = match &o.body {
                    Body::Surface(s) => Source::Mesh(o.id, geometry_key(&o.body), s.mesh_arc().clone()),
                    Body::Grid(g) => Source::Grid(o.id, geometry_key(&o.body), g.clone()),
                    Body::Solid(_) | Body::String(_) | Body::SeamModel(_) | Body::FaceDesign(_) | Body::Workflow(_) | Body::ParameterTable(_) | Body::Dataflow(_) | Body::Table(_) | Body::Script(_) => unreachable!("only drawn surfaces are snapped to one by one"),
                };
                (on_show(o), source)
            });
        // On show first, then surfaces before solids; a stable sort, so the order is otherwise kept.
        let mut pending: Vec<(bool, Source)> = objects.chain(tiles).filter(|(on, _)| *on || shown != Some(true)).collect();
        pending.sort_by_key(|(on, source)| (!*on, matches!(source, Source::Solids(..))));
        pending
    };
    let surfaces = pending.len();
    let t = Instant::now();
    let shared = state.app.clone();
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let triangles = blocking(move || {
        zd_geom::at_most(pending.len(), cores, |i| {
            let (_, source) = &pending[i];
            let triangles = c_triangles(source);
            let built = match source {
                Source::Mesh(id, key, mesh) => SurfaceRayCaster::new(mesh).map(|c| Built::Object(*id, *key, c)),
                Source::Grid(id, key, grid) => SurfaceRayCaster::new(&triangulate_grid(grid)).map(|c| Built::Object(*id, *key, c)),
                Source::Solids(layer, key, solids) => SolidCaster::new(solids).map(|c| Built::Layer(*layer, key.clone(), c)),
            }
            .map_err(|e| e.to_string())?;
            // In at once, so a press over it turns about it while the rest are still being indexed.
            let mut app = shared.lock().unwrap();
            match built {
                Built::Object(id, key, caster) => {
                    app.casters.insert(id, (key, Arc::new(caster)));
                }
                Built::Layer(tile, key, caster) => {
                    app.solid_casters.insert(tile, (key, Arc::new(caster)));
                }
            }
            Ok::<_, String>(triangles)
        })
        .into_iter()
        .sum::<Result<usize, _>>()
    })
    .await??;
    return Ok(SnapIndex { ms: ms(t), surfaces, triangles });

    fn c_triangles(source: &Source) -> usize {
        match source {
            Source::Mesh(_, _, m) => m.triangle_count(),
            Source::Grid(_, _, g) => 2 * (g.cols - 1) * (g.rows - 1),
            Source::Solids(_, _, solids) => solids.iter().map(|(_, m)| m.triangle_count()).sum(),
        }
    }
}

#[derive(Serialize)]
pub struct SnapHit {
    /// Offset from the scene origin, for the viewport.
    local: [f64; 3],
    /// Mine grid coordinates: what commands take.
    world: [f64; 3],
    /// The surface or grid hit; none for a solid, which is searched with the rest of its layer.
    object: Option<ObjectId>,
    /// What a section's cut lines gave: a vertex, segment or intersection, and what to call it.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// Nearest hit on a drawn surface of a visible layer, along a ray in scene-local coordinates. Hidden
/// objects are passed over.
#[tauri::command]
pub async fn snap_ray(origin: [f64; 3], direction: [f64; 3], state: State<'_, AppState>) -> Result<Option<SnapHit>, String> {
    Ok(snap_along(origin, direction, &state))
}

fn snap_along(origin: [f64; 3], direction: [f64; 3], state: &AppState) -> Option<SnapHit> {
    Snappable::of(state)?.cast(origin, direction)
}

/// What a ray snaps to: the drawn surfaces and layers of solids with an index that matches what they are now, taken
/// under the lock once, so a fan of rays casts without it.
struct Snappable {
    scene_origin: [f64; 3],
    objects: Vec<(ObjectId, Arc<SurfaceRayCaster>)>,
    layers: Vec<Arc<SolidCaster>>,
    hidden: Arc<std::collections::HashSet<ObjectId>>,
}

impl Snappable {
    fn of(state: &AppState) -> Option<Snappable> {
        let mut app = state.app.lock().unwrap();
        let scene_origin = app.origin()?;
        let project = app.session.project();
        let visible = |o: &zd_core::Object| !project.is_concealed(o.id) && project.layer(o.layer).is_some_and(|l| l.visible);
        let objects: Vec<(ObjectId, Arc<SurfaceRayCaster>)> = app
            .casters
            .iter()
            .filter_map(|(id, (key, caster))| {
                let object = project.object(*id)?;
                (visible(object) && *key == geometry_key(&object.body)).then(|| (*id, caster.clone()))
            })
            .collect();
        let layers: Vec<Arc<SolidCaster>> = app.solid_casters.iter().filter(|(tile, (key, _))| app.solid_keys.get(tile) == Some(key) && project.layer(tile.layer).is_some_and(|l| l.visible)).map(|(_, (_, caster))| caster.clone()).collect();
        Some(Snappable { scene_origin, objects, layers, hidden: project.concealed().clone() })
    }

    /// Nearest hit along a ray from `origin` (scene-local).
    fn cast(&self, origin: [f64; 3], direction: [f64; 3]) -> Option<SnapHit> {
        let o = self.scene_origin;
        let from = [origin[0] + o[0], origin[1] + o[1], origin[2] + o[2]];
        let on_objects = self.objects.iter().filter_map(|(id, c)| c.cast(from, direction).map(|hit| (Some(*id), hit)));
        let on_solids = self.layers.iter().filter_map(|c| c.cast(from, direction, &self.hidden).map(|(id, hit)| (Some(id), hit)));
        on_objects.chain(on_solids).min_by(|a, b| distance_sq(from, a.1).total_cmp(&distance_sq(from, b.1))).map(|(object, world)| SnapHit {
            local: [world[0] - o[0], world[1] - o[1], world[2] - o[2]],
            world,
            object,
            kind: None,
            label: None,
        })
    }
}

/// Over empty space, the nearest thing to the cursor (decision 0044): `rings` of rays from `origin` (scene-local), in
/// order out from the cursor. The first ring with a hit gives its hit nearest the camera, with the ring's number; rings
/// further out aren't cast. None in any ring gives none.
#[tauri::command]
pub async fn snap_rays(origin: [f64; 3], rings: Vec<Vec<[f64; 3]>>, state: State<'_, AppState>) -> Result<Option<(usize, SnapHit)>, String> {
    let Some(snappable) = Snappable::of(&state) else { return Ok(None) };
    Ok(nearest_in_rings(&rings, |direction| snappable.cast(origin, direction).map(|hit| (distance_sq(origin, hit.local), hit))))
}

/// The first ring in which `cast` hits anything, and its hit with the least distance.
fn nearest_in_rings<H>(rings: &[Vec<[f64; 3]>], cast: impl Fn([f64; 3]) -> Option<(f64, H)>) -> Option<(usize, H)> {
    rings.iter().enumerate().find_map(|(ring, directions)| directions.iter().filter_map(|d| cast(*d)).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, hit)| (ring, hit)))
}

/// Where a cursor at distance `along` and `rl` on the section snaps to the last cut, if anywhere near.
#[tauri::command]
pub async fn section_snap(along: f64, rl: f64, vertex_reach: f64, segment_reach: f64, ignoring: Option<[f64; 2]>, state: State<'_, AppState>) -> Result<Option<SnapHit>, String> {
    let mut app = state.app.lock().unwrap();
    let Some(origin) = app.origin() else { return Ok(None) };
    let Some(hit) = snap_on_section(&app, [along, rl], vertex_reach, segment_reach, ignoring) else { return Ok(None) };
    let world = hit.point;
    Ok(Some(SnapHit { local: [world[0] - origin[0], world[1] - origin[1], world[2] - origin[2]], world, object: None, kind: Some(hit.kind), label: Some(hit.label) }))
}

/// What a click at distance `along` and `rl` on the last section cut picks, within `reach` metres.
#[tauri::command]
pub async fn section_pick(along: f64, rl: f64, reach: f64, state: State<'_, AppState>) -> Result<Option<ObjectId>, String> {
    Ok(pick_on_section(&state.app.lock().unwrap(), [along, rl], reach))
}

#[derive(Serialize)]
pub struct BenchConfig {
    surfaces: Vec<String>,
    /// With `--solids <zip or folder>`: time this import and nothing else.
    solids: Option<String>,
    /// With `--solids`, `--strings a.dxf,b.dxf` loads these into the same scene first.
    strings: Vec<String>,
    /// With `--solids`, `--seam stack.csv` builds this seam model into the same scene first.
    seam: Option<String>,
    /// With `--solids`, `--walls a.sfg,b.srg` steps a wall up these grids, bottom first, on the section.
    walls: Vec<String>,
    results_path: String,
    scale_targets: Vec<u64>,
    transfer_cells: Vec<usize>,
    orbit_seconds: f64,
    drag_updates: usize,
    /// With `--opacity 0.5`, every surface is drawn that see-through: stacked see-through surfaces timed.
    opacity: Option<f64>,
    /// With `--pictures <folder>`, the `--surfaces` are drawn in their looks and saved there as pictures, and
    /// nothing is timed.
    pictures: Option<String>,
    /// With `--webgl`, the viewport runs on three's WebGL2 backend, as it does where there's no WebGPU.
    webgl: bool,
    /// With `--pictures` and `--palettes`, a few views are drawn on every RL ramp, and put on one sheet.
    palettes: bool,
    /// With `--design-lines`, a step design's own line and a copy of it on top: what a click picks, and Delete on each.
    design_lines: bool,
    /// With `--builder`, the Builder panel driven as Tim would: steps built from a toe, a step picked from the list, a berm's
    /// line edited and the wall above built on it (decision 0048).
    builder: bool,
    /// With `--project`, Project on the Strings tab driven as Tim would: the side set by a click in the view, three toes
    /// picked by a box and projected in one run, on to a surface with walls kept, a refusal, and one undo (decision 0079).
    project: bool,
    /// With `--measure`, the distance measure on the view bar driven as Tim would: M, two clicks on a string's ends for a
    /// known leg, a click on for a total, Backspace, Esc, and nothing made (decision 0079).
    measure: bool,
    /// With `--clicks`, how the view takes clicks and Esc driven as Tim would: NS starts a string, a right click finishes
    /// it, a right drag pans, Esc takes back one thing a press, from a tool panel's box too (decision 0080).
    clicks: bool,
    /// With `--string-edits`, Trim, Extend, Split at a click and Join driven as Tim would, by picks and clicks in the
    /// view (String tools plan, workstream 1).
    string_edits: bool,
    /// With `--ramp`, a ramp made by hand from the Walls row as Tim would: an edge graded, offset, projected up and down,
    /// and one surface between the four strings, its area known (decision 0079).
    ramp: bool,
    /// With `--ramp-start`, Ramp from a start driven as Tim would: placed along a toe by two clicks, moved, undone, and
    /// one straight from a start to a surface (decision 0079).
    ramp_start: bool,
    /// With `--cut`, the Cut solids panel driven as Tim would: a pit made between two surfaces inside its limit, sliced
    /// into strips by fifteen numbered faces, the faces joined a face a strip, and the pit cut at an RL (decision 0049).
    cut: bool,
    /// With `--select`, several objects selected by a box, Ctrl-click and Esc in 3D and plan, and zoomed to with F and
    /// Shift+F (decision 0050).
    select: bool,
    /// With `--shell`, panels minimised to strips and brought back, every panel hidden with the ` key, and the toolbar's
    /// groups. The layout saved on the machine is put back after.
    shell: bool,
    /// With `--section-guide`, the dashed line a two-point section draws from its first point to the cursor, on a
    /// made-up topography: shown after the first click, following the cursor, cleared by Esc and by the second click.
    section_guide: bool,
    /// With `--workflow`, the Run log's Capture and the Workflow panel driven as Tim would: two runs captured, run again,
    /// a step's Add to workflow, a pause, one undo (decision 0065).
    workflow: bool,
    /// With `--dataflow <project.zdp>`, a project holding a parameter table, a dataflow and a results table opened, each
    /// clicked in the Layers panel and what Properties shows read back (decision 0072).
    dataflow: Option<String>,
    /// A big project to open and orbit at Full and at 1 px (decision 0097, workstream 5).
    lod: Option<String>,
}

/// Present when the app was started with `--bench`: the viewport then runs Checks 1, 3 and 4
/// unattended and writes JSON results. `--surfaces a.xyz,b.dxf` overrides the default synthetic
/// surfaces in `bench/out/synthetic`.
#[tauri::command]
pub fn bench_config() -> Option<BenchConfig> {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == "--bench") {
        return None;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let bench_out = repo.join("bench").join("out");
    let solids = args.iter().position(|a| a == "--solids").and_then(|i| args.get(i + 1)).cloned();
    let list = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(|s| s.split(',').map(str::to_string).collect::<Vec<_>>());
    let surfaces = match args.iter().position(|a| a == "--surfaces") {
        Some(i) => args.get(i + 1).map(|s| s.split(',').map(str::to_string).collect()).unwrap_or_default(),
        None if solids.is_some() => Vec::new(),
        None => ["topo.xyz", "roof.xyz", "floor.xyz", "design.dxf"]
            .iter()
            .map(|f| bench_out.join("synthetic").join(f))
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
    };
    let results = if solids.is_some() { "app-solids-results.json" } else { "app-results.json" };
    Some(BenchConfig {
        surfaces,
        solids,
        strings: list("--strings").unwrap_or_default(),
        seam: args.iter().position(|a| a == "--seam").and_then(|i| args.get(i + 1)).cloned(),
        walls: list("--walls").unwrap_or_default(),
        results_path: bench_out.join(results).to_string_lossy().into_owned(),
        scale_targets: vec![10_000_000, 25_000_000, 50_000_000, 100_000_000],
        transfer_cells: vec![1000, 2000, 3000],
        orbit_seconds: 8.0,
        drag_updates: 300,
        opacity: args.iter().position(|a| a == "--opacity").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()),
        pictures: args.iter().position(|a| a == "--pictures").and_then(|i| args.get(i + 1)).cloned(),
        webgl: args.iter().any(|a| a == "--webgl"),
        palettes: args.iter().any(|a| a == "--palettes"),
        design_lines: args.iter().any(|a| a == "--design-lines"),
        builder: args.iter().any(|a| a == "--builder"),
        project: args.iter().any(|a| a == "--project"),
        measure: args.iter().any(|a| a == "--measure"),
        clicks: args.iter().any(|a| a == "--clicks"),
        string_edits: args.iter().any(|a| a == "--string-edits"),
        ramp: args.iter().any(|a| a == "--ramp"),
        ramp_start: args.iter().any(|a| a == "--ramp-start"),
        cut: args.iter().any(|a| a == "--cut"),
        select: args.iter().any(|a| a == "--select"),
        shell: args.iter().any(|a| a == "--shell"),
        section_guide: args.iter().any(|a| a == "--section-guide"),
        workflow: args.iter().any(|a| a == "--workflow"),
        dataflow: args.iter().position(|a| a == "--dataflow").and_then(|i| args.get(i + 1)).cloned(),
        lod: args.iter().position(|a| a == "--lod").and_then(|i| args.get(i + 1)).cloned(),
    })
}

/// Saves one picture of the viewport, a PNG data URL, as `<name>.png` in the `--pictures` folder. Bench only,
/// and only there.
#[tauri::command]
pub fn write_picture(name: String, data_url: String) -> Result<String, String> {
    use base64::Engine;
    let folder = bench_config().and_then(|c| c.pictures).ok_or("not taking pictures")?;
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(format!("'{name}' isn't a plain file name"));
    }
    let png = data_url.strip_prefix("data:image/png;base64,").ok_or("not a PNG data URL")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(png).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let path = Path::new(&folder).join(format!("{name}.png"));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn write_results(text: String) -> Result<String, String> {
    let path = bench_config().ok_or("not in bench mode")?.results_path;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Whether closing, or starting or opening another project, would lose work: anything run, undone or redone since the
/// last save or open, or a run still going (app review brief, 17 Nov, item 1).
#[tauri::command]
pub async fn project_unsaved(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.app.lock().unwrap().would_lose_work())
}

#[tauri::command]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}

fn stage(
    state: &State<'_, AppState>,
    buffer: Vec<u8>,
    name: String,
    kind: &'static str,
    triangles: usize,
    log: String,
    timings: Value,
) -> StagedMesh {
    let mut app = state.app.lock().unwrap();
    let handle = app.handle();
    let bytes = buffer.len();
    app.staged.insert(handle, buffer);
    StagedMesh { handle, name, kind, triangles, bytes, log, timings }
}

fn take_staged(state: &State<'_, AppState>, handle: u32) -> Result<Vec<u8>, String> {
    state.app.lock().unwrap().staged.remove(&handle).ok_or_else(|| "no staged mesh with that handle".into())
}

/// Runs CPU-heavy or blocking work off the async runtime threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

fn distance_sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1e3
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Instant;

    use super::*;
    use crate::changes::changes;

    /// A fan of rays over a square hole in a flat surface (decision 0044): a press over the hole turns about the hole's
    /// nearest edge. The surface is RL 0 with a hole x, y in (-10, 10), the eye 100 m above (4, 0), so the east edge is
    /// nearest, and rays in rings of 8 landing 2, 7, 12 and 20 m out on the ground. Every ray of the 2 m ring falls in
    /// the hole; of the 7 m ring only the east one lands on the ground, at (11, 0): within 1e-9 m.
    #[test]
    fn a_fan_over_a_hole_turns_about_its_nearest_edge() {
        // Four rectangles round the hole, from -50 to 50.
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for [x0, y0, x1, y1] in [[-50.0, -50.0, 50.0, -10.0], [-50.0, 10.0, 50.0, 50.0], [-50.0, -10.0, -10.0, 10.0], [10.0, -10.0, 50.0, 10.0]] {
            let n = vertices.len() as u32;
            vertices.extend([[x0, y0, 0.0], [x1, y0, 0.0], [x1, y1, 0.0], [x0, y1, 0.0]]);
            triangles.extend([[n, n + 1, n + 2], [n, n + 2, n + 3]]);
        }
        let caster = SurfaceRayCaster::new(&TriMesh::new(vertices, triangles)).unwrap();
        let eye = [4.0, 0.0, 100.0];
        let ring = |r: f64| (0..8).map(|i| (i as f64) * std::f64::consts::TAU / 8.0).map(|a| [r * a.cos(), r * a.sin(), -eye[2]]).collect::<Vec<_>>();
        let rings = [ring(2.0), ring(7.0), ring(12.0), ring(20.0)];
        let (n, hit) = nearest_in_rings(&rings, |d| caster.cast(eye, d).map(|h| (distance_sq(eye, h), h))).expect("a hit in some ring");
        assert_eq!(n, 1, "the ring at 7 m reaches past the east edge; the ring at 2 m is all over the hole");
        assert!((hit[0] - 11.0).abs() < 1e-9 && hit[1].abs() < 1e-9 && hit[2].abs() < 1e-9, "{hit:?}");
        // All over the hole, nothing: the caller falls back to the last pivot.
        assert!(nearest_in_rings(&[ring(1.0), ring(3.0)], |d| caster.cast(eye, d).map(|h| (distance_sq(eye, h), h))).is_none());
    }

    /// Run all cases carried on after a pause (app review brief, 17 Nov, item 3): the case that paused from the step
    /// after the pause, then every case after it from step 1; with nothing to carry on from, every case from step 1.
    #[test]
    fn continue_after_a_pause_runs_the_cases_after_it() {
        let names = || vec!["Base".to_string(), "50m".to_string(), "60m".to_string()];
        let from = |case: &str, step| CasesFrom { case: case.into(), step };
        let run = |list: Vec<(String, usize)>| list.into_iter().map(|(n, s)| format!("{n}@{s}")).collect::<Vec<_>>();
        assert_eq!(run(cases_from(names(), None).unwrap()), ["Base@1", "50m@1", "60m@1"]);
        assert_eq!(run(cases_from(names(), Some(&from("Base", 15))).unwrap()), ["Base@15", "50m@1", "60m@1"]);
        assert_eq!(run(cases_from(names(), Some(&from("50m", 3))).unwrap()), ["50m@3", "60m@1"]);
        assert!(cases_from(names(), Some(&from("70m", 1))).is_err(), "a case the workflow hasn't");
    }

    /// A long run on a made-up project named by `ZD_SCALE_PROJECT` (the store bench's `store-scale.zdp`, made with
    /// `store_scale -- 43000 250`), as `run_command` does it: what a snap waits for while the run goes, and how soon a
    /// cancelled run stops (decision 0060). Counts and timings only.
    #[test]
    #[ignore]
    fn a_run_does_not_hold_the_view() {
        let Ok(path) = std::env::var("ZD_SCALE_PROJECT") else { return };
        let state = AppState { app: Arc::new(Mutex::new(AppInner::new(Session::open(&path).unwrap(), None))) };
        // The snap index over the layer of blocks, as build_snap_index leaves it.
        let (ids, rl, down_at) = {
            let mut app = state.app.lock().unwrap();
            changes(&mut app, true);
            let t = Instant::now();
            for (tile, key) in app.solid_keys.clone() {
                let AppInner { session, tiles, .. } = &mut *app;
                let caster = SolidCaster::new(&tile_solids(session.project(), tiles, tile)).unwrap();
                app.solid_casters.insert(tile, (key, Arc::new(caster)));
            }
            println!("snap index: {} tiles in {:.0} ms", app.solid_casters.len(), t.elapsed().as_secs_f64() * 1e3);
            let first = bounds(app.session.project().objects().next().unwrap()).unwrap();
            let ids: Vec<ObjectId> = app.session.project().objects().map(|o| o.id).collect();
            let origin = app.origin().unwrap();
            let c = first.centre();
            (ids, (first.min[2] + first.max[2]) / 2.0, [c[0] - origin[0], c[1] - origin[1], first.max[2] + 100.0 - origin[2]])
        };
        let snap = |state: &AppState| {
            let t = Instant::now();
            let hit = snap_along(down_at, [0.0, 0.0, -1.0], state);
            (t.elapsed().as_secs_f64() * 1e3, hit.is_some())
        };
        let (idle_ms, hit) = snap(&state);
        println!("snap with no run going: {idle_ms:.2} ms, hit {hit}");

        let start = |state: &AppState| {
            let mut app = state.app.lock().unwrap();
            app.idle().unwrap();
            let job = app.session.begin_run("solid.cut_by_surface", serde_json::json!({ "solids": ids, "rl": rl, "keep": "below", "layer": "cut" }));
            app.running = Some(Running { command: "solid.cut_by_surface".into(), started: Instant::now(), control: job.control() });
            job
        };
        let job = start(&state);
        let control = job.control();
        let t = Instant::now();
        let worker = std::thread::spawn(move || job.run());
        let mut waits = Vec::new();
        while !worker.is_finished() {
            let (ms, _) = snap(&state);
            waits.push(ms);
            assert!(state.app.lock().unwrap().idle().is_err(), "a change waits while it runs");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let done = worker.join().unwrap();
        let run_ms = t.elapsed().as_secs_f64() * 1e3;
        let progress = control.progress();
        let t = Instant::now();
        let record = {
            let mut app = state.app.lock().unwrap();
            app.running = None;
            let record = app.session.end_run(done).unwrap();
            changes(&mut app, false);
            record
        };
        let apply_ms = t.elapsed().as_secs_f64() * 1e3;
        waits.sort_by(f64::total_cmp);
        println!(
            "cut {} solids: {run_ms:.0} ms, {} made; applied and sent in {apply_ms:.0} ms; progress {} of {} {}",
            ids.len(),
            record.outputs.len(),
            progress.done,
            progress.total,
            progress.of
        );
        println!("snaps during it: {}, p50 {:.2} ms, max {:.2} ms", waits.len(), waits[waits.len() / 2], waits[waits.len() - 1]);

        // Again, cancelled a second in.
        state.app.lock().unwrap().session.undo();
        let job = start(&state);
        let control = job.control();
        let worker = std::thread::spawn(move || job.run());
        std::thread::sleep(std::time::Duration::from_secs(1));
        let at = control.progress();
        let t = Instant::now();
        control.cancel();
        let done = worker.join().unwrap();
        let stop_ms = t.elapsed().as_secs_f64() * 1e3;
        let mut app = state.app.lock().unwrap();
        app.running = None;
        let before = app.session.project().objects().count();
        let refused = app.session.end_run(done).unwrap_err();
        println!("cancelled at {} of {} {}: stopped in {stop_ms:.0} ms, {refused}", at.done, at.total, at.of);
        assert_eq!(app.session.project().objects().count(), before, "nothing applied");
    }
}
