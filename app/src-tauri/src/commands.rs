//! Commands the viewport calls. Meshes go out as binary render buffers (`ipc::Response`), never
//! JSON; everything else is small JSON.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, State};
use zd_core::TriMesh;
use zd_geom::{SurfaceRayCaster, triangulate_grid};
use zd_io::{SurfaceImport, read_surface};

use crate::render::encode_render_buffer;
use crate::state::{AppState, LoadedSurface};
use crate::synthetic::synthetic_topography;

/// Reads a DXF or XYZ/CSV surface, keeps it in f64 for snapping, and returns its render buffer.
#[tauri::command]
pub async fn open_surface(path: String, state: State<'_, AppState>) -> Result<Response, String> {
    let path = PathBuf::from(path);
    let t = Instant::now();
    let import = blocking(move || read_surface(&path).map_err(|e| e.to_string()).map(|i| (path, i))).await??;
    let read_ms = ms(t);
    let (path, import) = import;

    let t = Instant::now();
    let (mesh, log) = match import {
        SurfaceImport::Triangulation(dxf) => (dxf.mesh, format!("{:?}", dxf.log)),
        SurfaceImport::Grid(xyz) => {
            let log = format!(
                "{:?}; grid {} x {} nodes at {} x {} m",
                xyz.log, xyz.grid.cols, xyz.grid.rows, xyz.grid.cell_size[0], xyz.grid.cell_size[1]
            );
            (blocking(move || triangulate_grid(&xyz.grid)).await?, log)
        }
    };
    let triangulate_ms = ms(t);
    let mesh = Arc::new(mesh);

    let (id, origin) = {
        let mut scene = state.scene.lock().unwrap();
        let id = scene.next_id();
        let origin = scene.origin_for(&mesh);
        scene.surfaces.push(LoadedSurface { id, mesh: mesh.clone(), caster: None });
        (id, origin)
    };

    let header = serde_json::json!({
        "id": id,
        "name": file_name(&path),
        "log": log,
        "timings": { "read_ms": read_ms, "triangulate_ms": triangulate_ms },
    });
    let buffer = blocking(move || encode_render_buffer(&mesh, origin, header)).await?;
    Ok(Response::new(buffer))
}

#[tauri::command]
pub fn clear_surfaces(state: State<'_, AppState>) {
    let mut scene = state.scene.lock().unwrap();
    scene.surfaces.clear();
    scene.staged.clear();
    scene.origin = None;
}

#[derive(Serialize)]
pub struct StagedMesh {
    handle: u32,
    triangles: usize,
    bytes: usize,
    prepare_ms: f64,
}

/// Generates and encodes a synthetic topography of `cells` x `cells` and holds the buffer, so
/// the transfer check can time the hand-over alone.
#[tauri::command]
pub async fn prepare_synthetic(cells: usize, state: State<'_, AppState>) -> Result<StagedMesh, String> {
    let origin = state.scene.lock().unwrap().origin.unwrap_or([650_000.0, 7_450_000.0, 200.0]);
    let t = Instant::now();
    let (buffer, triangles) = blocking(move || {
        let mesh = triangulate_grid(&synthetic_topography(cells));
        let header = serde_json::json!({ "id": 0, "name": format!("synthetic {cells}x{cells}"), "log": "", "timings": {} });
        (encode_render_buffer(&mesh, origin, header), mesh.triangle_count())
    })
    .await?;
    let prepare_ms = ms(t);
    let mut scene = state.scene.lock().unwrap();
    let handle = scene.next_id();
    let bytes = buffer.len();
    scene.staged.insert(handle, buffer);
    Ok(StagedMesh { handle, triangles, bytes, prepare_ms })
}

#[tauri::command]
pub fn fetch_staged(handle: u32, state: State<'_, AppState>) -> Result<Response, String> {
    let buffer = state.scene.lock().unwrap().staged.remove(&handle).ok_or("no staged mesh with that handle")?;
    Ok(Response::new(buffer))
}

#[derive(Serialize)]
pub struct SnapIndex {
    ms: f64,
    surfaces: usize,
    triangles: usize,
}

/// Builds the BVH for every loaded surface that lacks one.
#[tauri::command]
pub async fn build_snap_index(state: State<'_, AppState>) -> Result<SnapIndex, String> {
    let pending: Vec<(u32, Arc<TriMesh>)> = {
        let scene = state.scene.lock().unwrap();
        scene.surfaces.iter().filter(|s| s.caster.is_none()).map(|s| (s.id, s.mesh.clone())).collect()
    };
    let triangles = pending.iter().map(|(_, m)| m.triangle_count()).sum();
    let surfaces = pending.len();
    let t = Instant::now();
    // One thread per surface: BVH builds are independent.
    let built = blocking(move || {
        std::thread::scope(|scope| {
            let handles: Vec<_> = pending
                .iter()
                .map(|(id, mesh)| scope.spawn(move || SurfaceRayCaster::new(mesh).map(|c| (*id, Arc::new(c)))))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("BVH build thread").map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()
        })
    })
    .await??;
    let ms = ms(t);
    let mut scene = state.scene.lock().unwrap();
    for (id, caster) in built {
        if let Some(s) = scene.surfaces.iter_mut().find(|s| s.id == id) {
            s.caster = Some(caster);
        }
    }
    Ok(SnapIndex { ms, surfaces, triangles })
}

#[derive(Serialize)]
pub struct SnapHit {
    /// Offset from the scene origin, for the viewport.
    local: [f64; 3],
    world: [f64; 3],
    surface: u32,
}

/// Nearest surface hit along a ray given in scene-local coordinates.
#[tauri::command]
pub fn snap_ray(origin: [f64; 3], direction: [f64; 3], state: State<'_, AppState>) -> Option<SnapHit> {
    let (scene_origin, casters) = {
        let scene = state.scene.lock().unwrap();
        let casters: Vec<(u32, Arc<SurfaceRayCaster>)> =
            scene.surfaces.iter().filter_map(|s| s.caster.clone().map(|c| (s.id, c))).collect();
        (scene.origin?, casters)
    };
    let from = [origin[0] + scene_origin[0], origin[1] + scene_origin[1], origin[2] + scene_origin[2]];
    casters
        .iter()
        .filter_map(|(id, c)| c.cast(from, direction).map(|hit| (*id, hit)))
        .min_by(|a, b| distance_sq(from, a.1).total_cmp(&distance_sq(from, b.1)))
        .map(|(surface, world)| SnapHit {
            local: [world[0] - scene_origin[0], world[1] - scene_origin[1], world[2] - scene_origin[2]],
            world,
            surface,
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

/// Runs CPU-heavy work off the async runtime threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn distance_sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1e3
}
