//! Commands the viewport calls. Every edit to the project goes through the engine session as a
//! typed command (`run_command`); the viewport is sent a small JSON summary of the project after
//! each change, and fetches geometry as binary render buffers (WebGPU shared buffer or raw IPC
//! bytes), never as JSON.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Response;
use tauri::{AppHandle, State, Webview};
use zd_commands::{LogEntry, RunRecord, Session};
use zd_core::{Attributes, Body, Layer, LayerId, MeshValidity, ObjectId, ObjectKind, RunId, SeamModel, TriMesh};
use zd_geom::{StackHorizon, SurfaceRayCaster, stack_issue_nodes, triangulate_grid};
use zd_io::read_stack_grids;

use crate::render::{NodeValues, encode_grid, encode_grid_with, encode_triangulation};
use crate::shared::post_shared_buffer;
use crate::state::{AppInner, AppState, bounds, geometry_key, is_drawn_surface};
use crate::synthetic::synthetic_topography;

/// The project as the interface needs it: layers and every live object, with string vertices
/// inline (strings are small) and surfaces as a key to fetch their geometry by.
#[derive(Serialize)]
pub struct ProjectSummary {
    path: Option<String>,
    can_undo: bool,
    can_redo: bool,
    origin: Option<[f64; 3]>,
    layers: Vec<Layer>,
    objects: Vec<ObjectSummary>,
}

#[derive(Serialize)]
pub struct ObjectSummary {
    id: ObjectId,
    revision: u32,
    name: String,
    layer: LayerId,
    kind: ObjectKind,
    attributes: Attributes,
    /// Drawn as a surface (surfaces, solids, elevation grids).
    drawn: bool,
    /// Changes only when the geometry does; fetch a new render buffer when it changes.
    geometry_key: String,
    bounds_local: Option<[[f64; 3]; 2]>,
    /// String vertices, offset from the scene origin.
    vertices_local: Option<Vec<[f64; 3]>>,
    closed: Option<bool>,
}

fn summary(app: &mut AppInner) -> ProjectSummary {
    let origin = app.origin();
    let o = origin.unwrap_or([0.0; 3]);
    let local = |p: [f64; 3]| [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
    let project = app.session.project();
    let objects = project
        .objects()
        .map(|object| {
            let (vertices_local, closed) = match &object.body {
                Body::String(line) => (Some(line.vertices.iter().map(|&p| local(p)).collect()), Some(line.closed)),
                _ => (None, None),
            };
            ObjectSummary {
                id: object.id,
                revision: object.revision,
                name: object.name.clone(),
                layer: object.layer,
                kind: object.kind(),
                attributes: object.attributes.clone(),
                drawn: is_drawn_surface(object),
                geometry_key: geometry_key(&object.body).to_string(),
                bounds_local: bounds(object).map(|b| [local(b.min), local(b.max)]),
                vertices_local,
                closed,
            }
        })
        .collect();
    ProjectSummary {
        path: app.path.as_ref().map(|p| p.display().to_string()),
        can_undo: project.can_undo(),
        can_redo: project.can_redo(),
        origin,
        layers: project.layers().to_vec(),
        objects,
    }
}

#[tauri::command]
pub fn project_summary(state: State<'_, AppState>) -> ProjectSummary {
    summary(&mut state.app.lock().unwrap())
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
    Surface { vertices: usize, triangles: usize, area: f64, validity: MeshValidity },
    Solid { vertices: usize, triangles: usize, area: f64, volume: f64, validity: MeshValidity },
    Grid { origin: [f64; 2], cell_size: [f64; 2], cols: usize, rows: usize, nulls: usize, z_range: Option<[f64; 2]> },
    SeamModel { model: SeamModel },
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
        },
        Body::Solid(s) => Measures::Solid {
            vertices: s.mesh().vertices.len(),
            triangles: s.mesh().triangle_count(),
            area: s.mesh().area(),
            volume: s.volume(),
            validity: s.validity(),
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
    summary: ProjectSummary,
}

/// Runs an engine command by name with JSON parameters. A failure comes back as the error text
/// (and is in the run log either way).
#[tauri::command]
pub async fn run_command(name: String, params: Value, state: State<'_, AppState>) -> Result<RunReply, String> {
    let app = state.app.clone();
    blocking(move || {
        let mut app = app.lock().unwrap();
        let record = app.session.run(&name, params).map_err(|e| e.to_string())?;
        Ok(RunReply { record, summary: summary(&mut app) })
    })
    .await?
}

#[tauri::command]
pub fn undo(state: State<'_, AppState>) -> ProjectSummary {
    let mut app = state.app.lock().unwrap();
    app.session.undo();
    summary(&mut app)
}

#[tauri::command]
pub fn redo(state: State<'_, AppState>) -> ProjectSummary {
    let mut app = state.app.lock().unwrap();
    app.session.redo();
    summary(&mut app)
}

#[tauri::command]
pub fn new_project(state: State<'_, AppState>) -> ProjectSummary {
    let mut app = state.app.lock().unwrap();
    *app = AppInner::new(Session::with_builtins(), None);
    summary(&mut app)
}

#[tauri::command]
pub async fn open_project(path: String, state: State<'_, AppState>) -> Result<ProjectSummary, String> {
    let app = state.app.clone();
    blocking(move || {
        let session = Session::open(&path).map_err(|e| e.to_string())?;
        let mut app = app.lock().unwrap();
        *app = AppInner::new(session, Some(PathBuf::from(path)));
        Ok(summary(&mut app))
    })
    .await?
}

#[derive(Serialize)]
pub struct SaveReply {
    path: String,
    object_files_written: usize,
    object_files_reused: usize,
    files_deleted: usize,
    bytes_written: u64,
    ms: f64,
}

/// Saves to `path`, or to the folder last opened or saved when `path` is not given.
#[tauri::command]
pub async fn save_project(path: Option<String>, state: State<'_, AppState>) -> Result<SaveReply, String> {
    let app = state.app.clone();
    blocking(move || {
        let mut app = app.lock().unwrap();
        let path = match path.map(PathBuf::from).or_else(|| app.path.clone()) {
            Some(p) => p,
            None => return Err("this project has not been saved yet; choose a folder".to_string()),
        };
        let report = app.session.save(&path).map_err(|e| e.to_string())?;
        app.path = Some(path.clone());
        Ok(SaveReply {
            path: path.display().to_string(),
            object_files_written: report.object_files_written,
            object_files_reused: report.object_files_reused,
            files_deleted: report.files_deleted,
            bytes_written: report.bytes_written,
            ms: report.ms,
        })
    })
    .await?
}

/// Display state only: not a command, not undone.
#[tauri::command]
pub fn set_layer_visible(layer: LayerId, visible: bool, state: State<'_, AppState>) -> ProjectSummary {
    let mut app = state.app.lock().unwrap();
    app.session.set_layer_visible(layer, visible);
    summary(&mut app)
}

#[tauri::command]
pub fn run_log(state: State<'_, AppState>) -> Vec<LogEntry> {
    state.app.lock().unwrap().session.log().to_vec()
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
            Body::String(_) | Body::SeamModel(_) => unreachable!("only drawn surfaces are staged"),
        };
        (buffer, kind, triangles, ms(t))
    })
    .await?;
    Ok(stage(&state, buffer, name, kind, triangles, String::new(), serde_json::json!({ "encode": encode_ms })))
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
    let horizons = model.horizons.iter().map(|h| HorizonChoice { code: h.code.clone(), working: h.working }).collect();
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

/// Builds the snap index for every drawn surface whose geometry has none yet, one thread per
/// surface, and drops indexes of objects that are gone.
#[tauri::command]
pub async fn build_snap_index(state: State<'_, AppState>) -> Result<SnapIndex, String> {
    enum Source {
        Mesh(Arc<TriMesh>),
        Grid(Arc<zd_core::Grid>),
    }
    let pending: Vec<(ObjectId, usize, Source)> = {
        let mut app = state.app.lock().unwrap();
        let live: std::collections::HashSet<ObjectId> = app.session.project().objects().map(|o| o.id).collect();
        app.casters.retain(|id, _| live.contains(id));
        let project = app.session.project();
        project
            .objects()
            .filter(|o| is_drawn_surface(o))
            .filter(|o| app.casters.get(&o.id).is_none_or(|(key, _)| *key != geometry_key(&o.body)))
            .map(|o| {
                let source = match &o.body {
                    Body::Surface(s) => Source::Mesh(s.mesh_arc().clone()),
                    Body::Solid(s) => Source::Mesh(s.mesh_arc().clone()),
                    Body::Grid(g) => Source::Grid(g.clone()),
                    Body::String(_) | Body::SeamModel(_) => unreachable!("only drawn surfaces are snapped to"),
                };
                (o.id, geometry_key(&o.body), source)
            })
            .collect()
    };
    let surfaces = pending.len();
    let t = Instant::now();
    let built = blocking(move || {
        std::thread::scope(|scope| {
            let handles: Vec<_> = pending
                .iter()
                .map(|(id, key, source)| {
                    scope.spawn(move || {
                        let caster = match source {
                            Source::Mesh(mesh) => SurfaceRayCaster::new(mesh),
                            Source::Grid(grid) => SurfaceRayCaster::new(&triangulate_grid(grid)),
                        };
                        caster.map(|c| (*id, *key, Arc::new(c), c_triangles(source)))
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("snap index thread").map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()
        })
    })
    .await??;
    let ms = ms(t);
    let triangles = built.iter().map(|b| b.3).sum();
    let mut app = state.app.lock().unwrap();
    for (id, key, caster, _) in built {
        app.casters.insert(id, (key, caster));
    }
    return Ok(SnapIndex { ms, surfaces, triangles });

    fn c_triangles(source: &Source) -> usize {
        match source {
            Source::Mesh(m) => m.triangle_count(),
            Source::Grid(g) => 2 * (g.cols - 1) * (g.rows - 1),
        }
    }
}

#[derive(Serialize)]
pub struct SnapHit {
    /// Offset from the scene origin, for the viewport.
    local: [f64; 3],
    /// Mine grid coordinates: what commands take.
    world: [f64; 3],
    object: ObjectId,
}

/// Nearest hit on a drawn surface of a visible layer, along a ray in scene-local coordinates.
#[tauri::command]
pub fn snap_ray(origin: [f64; 3], direction: [f64; 3], state: State<'_, AppState>) -> Option<SnapHit> {
    let (scene_origin, casters) = {
        let mut app = state.app.lock().unwrap();
        let scene_origin = app.origin()?;
        let project = app.session.project();
        let visible = |o: &zd_core::Object| project.layer(o.layer).is_some_and(|l| l.visible);
        let casters: Vec<(ObjectId, Arc<SurfaceRayCaster>)> = project
            .objects()
            .filter(|o| visible(o))
            .filter_map(|o| app.casters.get(&o.id).filter(|(key, _)| *key == geometry_key(&o.body)).map(|(_, c)| (o.id, c.clone())))
            .collect();
        (scene_origin, casters)
    };
    let from = [origin[0] + scene_origin[0], origin[1] + scene_origin[1], origin[2] + scene_origin[2]];
    casters
        .iter()
        .filter_map(|(id, c)| c.cast(from, direction).map(|hit| (*id, hit)))
        .min_by(|a, b| distance_sq(from, a.1).total_cmp(&distance_sq(from, b.1)))
        .map(|(object, world)| SnapHit {
            local: [world[0] - scene_origin[0], world[1] - scene_origin[1], world[2] - scene_origin[2]],
            world,
            object,
        })
}

#[derive(Serialize)]
pub struct BenchConfig {
    surfaces: Vec<String>,
    results_path: String,
    scale_targets: Vec<u64>,
    transfer_cells: Vec<usize>,
    orbit_seconds: f64,
    drag_updates: usize,
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
    let surfaces = match args.iter().position(|a| a == "--surfaces") {
        Some(i) => args.get(i + 1).map(|s| s.split(',').map(str::to_string).collect()).unwrap_or_default(),
        None => ["topo.xyz", "roof.xyz", "floor.xyz", "design.dxf"]
            .iter()
            .map(|f| bench_out.join("synthetic").join(f))
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
    };
    Some(BenchConfig {
        surfaces,
        results_path: bench_out.join("app-results.json").to_string_lossy().into_owned(),
        scale_targets: vec![10_000_000, 25_000_000, 50_000_000, 100_000_000],
        transfer_cells: vec![1000, 2000, 3000],
        orbit_seconds: 8.0,
        drag_updates: 300,
    })
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
