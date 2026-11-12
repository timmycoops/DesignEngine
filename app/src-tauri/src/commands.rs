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
use crate::render::{NodeValues, encode_grid, encode_grid_with, encode_section, encode_solids, encode_triangulation};
use crate::selection;
use crate::section::{SectionCut, add_selected, pick_on_section, section_groups, snap_on_section};
use crate::shared::post_shared_buffer;
use crate::state::{AppInner, AppState, Running, SolidCaster, bounds, every_layer_solid, geometry_key, is_drawn_surface, solid_snap_keys};
use crate::synthetic::synthetic_topography;

/// The whole project, in place of what the app holds: after a reload, or when a change didn't follow on from what the
/// app holds.
#[tauri::command]
pub async fn project_summary(state: State<'_, AppState>) -> Result<ProjectChanges, String> {
    Ok(changes(&mut state.app.lock().unwrap(), true))
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
            Body::String(_) | Body::SeamModel(_) | Body::FaceDesign(_) => unreachable!("only drawn surfaces are staged"),
        };
        (buffer, kind, triangles, ms(t))
    })
    .await?;
    Ok(stage(&state, buffer, name, kind, triangles, String::new(), serde_json::json!({ "encode": encode_ms })))
}

/// Encodes every solid on a layer as one mesh for the viewport and holds it to be fetched: a pit's
/// tens of thousands of solids are then a few chunks to draw, not one mesh each. Hidden solids are in it too, each
/// vertex saying which solid it's of, so hiding or picking one changes a table beside the mesh and not the mesh
/// (decision 0060, workstream 3).
#[tauri::command]
pub async fn stage_layer_solids(layer: LayerId, state: State<'_, AppState>) -> Result<StagedMesh, String> {
    let app = state.app.clone();
    let (solids, name, origin) = {
        let mut app = app.lock().unwrap();
        let origin = app.origin().unwrap_or([0.0; 3]);
        let project = app.session.project();
        let name = project.layer(layer).ok_or("no such layer")?.name.clone();
        (every_layer_solid(project, layer), name, origin)
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

/// The box round these objects together, world coordinates, for zoom to selection. `None` if none has a place.
#[tauri::command]
pub async fn objects_bounds(objects: Vec<ObjectId>, state: State<'_, AppState>) -> Result<Option<[[f64; 3]; 2]>, String> {
    let app = state.app.lock().unwrap();
    Ok(selection::bounds_of(app.session.project(), &objects).map(|b| [b.min, b.max]))
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
}

#[derive(Serialize)]
pub struct HorizonChoice {
    code: String,
    material: String,
    working: bool,
}

#[derive(Serialize)]
pub struct IssueMarker {
    /// Index into the model's issues.
    issue: usize,
    position: [f64; 3],
}

/// Reads a seam model's roofs and floors in place and stages one horizon's roof (the first working
/// horizon if none is named) with its thickness per node, marking the nodes of blocking issues.
#[tauri::command]
pub async fn stage_seam_horizon(id: ObjectId, horizon: Option<String>, state: State<'_, AppState>) -> Result<StagedHorizon, String> {
    let (model, name, origin) = {
        let mut app = state.app.lock().unwrap();
        let origin = app.origin().unwrap_or([0.0; 3]);
        let object = app.session.project().object(id).ok_or("no such object")?.clone();
        let Body::SeamModel(model) = &object.body else { return Err(format!("'{}' is not a seam model", object.name)) };
        (model.clone(), object.name.clone(), origin)
    };
    let code = match horizon {
        Some(code) => code,
        None => model.working_section().next().or(model.horizons.first()).ok_or("the model has no horizons")?.code.clone(),
    };
    let title = name.clone();
    let horizons = model.horizons.iter().map(|h| HorizonChoice { code: h.code.clone(), material: h.material.clone(), working: h.working }).collect();
    let (buffer, triangles, staged_horizon, encode_ms) = blocking(move || -> Result<_, String> {
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
        let values = NodeValues { name: "thickness", values: &thickness, flagged: &flags, range: [0.0, thickness_max] };
        let header = serde_json::json!({ "id": 0, "name": format!("{title} {code}"), "timings": {} });
        let buffer = encode_grid_with(roof, Some(&values), origin, header);
        let triangles = 2 * (roof.cols - 1) * (roof.rows - 1);
        let horizon = (code, thickness_max, thickness.iter().filter(|t| !t.is_nan()).count(), roof.z.len(), flags.iter().filter(|&&f| f).count(), markers, grids.changed);
        Ok((buffer, triangles, horizon, ms(t)))
    })
    .await??;
    let (code, thickness_max, coal_nodes, nodes, flagged_nodes, markers, changed) = staged_horizon;
    let log = if changed.is_empty() { String::new() } else { format!("{} grid file(s) changed since the model was built", changed.len()) };
    let staged = stage(&state, buffer, format!("{name} {code}"), "grid", triangles, log, serde_json::json!({ "read_and_encode": encode_ms }));
    Ok(StagedHorizon { staged, horizon: code, thickness_max, coal_nodes, nodes, flagged_nodes, markers, changed, horizons })
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

/// Builds the snap index for every drawn surface and layer of solids whose geometry has none yet,
/// one thread each, and drops indexes of objects and layers that are gone.
#[tauri::command]
pub async fn build_snap_index(state: State<'_, AppState>) -> Result<SnapIndex, String> {
    enum Source {
        Mesh(ObjectId, usize, Arc<TriMesh>),
        Grid(ObjectId, usize, Arc<zd_core::Grid>),
        Solids(LayerId, String, Vec<(ObjectId, Arc<TriMesh>)>),
    }
    enum Built {
        Object(ObjectId, usize, SurfaceRayCaster),
        Layer(LayerId, String, SolidCaster),
    }
    let pending: Vec<Source> = {
        let mut app = state.app.lock().unwrap();
        let live: std::collections::HashSet<ObjectId> = app.session.project().objects().map(|o| o.id).collect();
        app.casters.retain(|id, _| live.contains(id));
        let keys = solid_snap_keys(app.session.project());
        app.solid_casters.retain(|layer, _| keys.contains_key(layer));
        let project = app.session.project();
        let objects = project
            .objects()
            .filter(|o| is_drawn_surface(o) && !matches!(o.body, Body::Solid(_)))
            .filter(|o| app.casters.get(&o.id).is_none_or(|(key, _)| *key != geometry_key(&o.body)))
            .map(|o| match &o.body {
                Body::Surface(s) => Source::Mesh(o.id, geometry_key(&o.body), s.mesh_arc().clone()),
                Body::Grid(g) => Source::Grid(o.id, geometry_key(&o.body), g.clone()),
                Body::Solid(_) | Body::String(_) | Body::SeamModel(_) | Body::FaceDesign(_) => unreachable!("only drawn surfaces are snapped to one by one"),
            });
        let layers = keys
            .iter()
            .filter(|(layer, key)| app.solid_casters.get(layer).is_none_or(|(built, _)| built != *key))
            .map(|(layer, key)| Source::Solids(*layer, key.clone(), every_layer_solid(project, *layer)));
        objects.chain(layers).collect()
    };
    let surfaces = pending.len();
    let t = Instant::now();
    let built = blocking(move || {
        std::thread::scope(|scope| {
            let handles: Vec<_> = pending
                .into_iter()
                .map(|source| {
                    scope.spawn(move || {
                        let triangles = c_triangles(&source);
                        let built = match source {
                            Source::Mesh(id, key, mesh) => SurfaceRayCaster::new(&mesh).map(|c| Built::Object(id, key, c)),
                            Source::Grid(id, key, grid) => SurfaceRayCaster::new(&triangulate_grid(&grid)).map(|c| Built::Object(id, key, c)),
                            Source::Solids(layer, key, solids) => SolidCaster::new(&solids).map(|c| Built::Layer(layer, key, c)),
                        };
                        built.map(|b| (b, triangles))
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("snap index thread").map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()
        })
    })
    .await??;
    let ms = ms(t);
    let triangles = built.iter().map(|b| b.1).sum();
    let mut app = state.app.lock().unwrap();
    for (index, _) in built {
        match index {
            Built::Object(id, key, caster) => {
                app.casters.insert(id, (key, Arc::new(caster)));
            }
            Built::Layer(layer, key, caster) => {
                app.solid_casters.insert(layer, (key, Arc::new(caster)));
            }
        }
    }
    return Ok(SnapIndex { ms, surfaces, triangles });

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
    let (scene_origin, objects, layers, hidden) = {
        let mut app = state.app.lock().unwrap();
        let scene_origin = app.origin()?;
        let project = app.session.project();
        let visible = |o: &zd_core::Object| !project.is_hidden(o.id) && project.layer(o.layer).is_some_and(|l| l.visible);
        let objects: Vec<(ObjectId, Arc<SurfaceRayCaster>)> = app
            .casters
            .iter()
            .filter_map(|(id, (key, caster))| {
                let object = project.object(*id)?;
                (visible(object) && *key == geometry_key(&object.body)).then(|| (*id, caster.clone()))
            })
            .collect();
        let layers: Vec<Arc<SolidCaster>> = app.solid_casters.iter().filter(|(layer, (key, _))| app.solid_keys.get(layer) == Some(key) && project.layer(**layer).is_some_and(|l| l.visible)).map(|(_, (_, caster))| caster.clone()).collect();
        (scene_origin, objects, layers, project.hidden().clone())
    };
    let from = [origin[0] + scene_origin[0], origin[1] + scene_origin[1], origin[2] + scene_origin[2]];
    let on_objects = objects.iter().filter_map(|(id, c)| c.cast(from, direction).map(|hit| (Some(*id), hit)));
    let on_solids = layers.iter().filter_map(|c| c.cast(from, direction, &hidden).map(|(id, hit)| (Some(id), hit)));
    on_objects
        .chain(on_solids)
        .min_by(|a, b| distance_sq(from, a.1).total_cmp(&distance_sq(from, b.1)))
        .map(|(object, world)| SnapHit {
            local: [world[0] - scene_origin[0], world[1] - scene_origin[1], world[2] - scene_origin[2]],
            world,
            object,
            kind: None,
            label: None,
        })
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
    /// With `--cut`, the Cut solids panel driven as Tim would: a pit made between two surfaces inside its limit, sliced
    /// into strips by fifteen numbered faces, the faces joined a face a strip, and the pit cut at an RL (decision 0049).
    cut: bool,
    /// With `--select`, several objects selected by a box, Ctrl-click and Esc in 3D and plan, and zoomed to with F and
    /// Shift+F (decision 0050).
    select: bool,
    /// With `--shell`, panels minimised to strips and brought back, every panel hidden with the ` key, and the toolbar's
    /// groups. The layout saved on the machine is put back after.
    shell: bool,
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
        cut: args.iter().any(|a| a == "--cut"),
        select: args.iter().any(|a| a == "--select"),
        shell: args.iter().any(|a| a == "--shell"),
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
            let project = app.session.project();
            let layer = project.objects().next().unwrap().layer;
            let t = Instant::now();
            let caster = SolidCaster::new(&every_layer_solid(project, layer)).unwrap();
            println!("snap index: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
            let key = app.solid_keys[&layer].clone();
            let first = bounds(app.session.project().objects().next().unwrap()).unwrap();
            let ids: Vec<ObjectId> = app.session.project().objects().map(|o| o.id).collect();
            app.solid_casters.insert(layer, (key, Arc::new(caster)));
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
