use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use zd_commands::Session;
use zd_core::{AttrValue, Body, Object, ObjectId};
use zd_geom::SurfaceRayCaster;

/// The shell's state: one engine session, plus what the viewport needs around it. Shared with
/// worker threads, hence the `Arc`.
#[derive(Clone)]
pub struct AppState {
    pub app: Arc<Mutex<AppInner>>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState { app: Arc::new(Mutex::new(AppInner::new(Session::with_builtins(), None))) }
    }
}

pub struct AppInner {
    pub session: Session,
    /// The project folder last opened or saved.
    pub path: Option<PathBuf>,
    /// Local origin shared by every buffer sent to the viewport (engine rule 2); set from the
    /// first object with geometry and kept for the life of the project.
    pub origin: Option<[f64; 3]>,
    /// Encoded render buffers waiting to be fetched.
    pub staged: HashMap<u32, Vec<u8>>,
    pub next_handle: u32,
    /// Snap index per drawn object, with the geometry it was built from.
    pub casters: HashMap<ObjectId, (usize, Arc<SurfaceRayCaster>)>,
}

impl AppInner {
    pub fn new(session: Session, path: Option<PathBuf>) -> Self {
        AppInner { session, path, origin: None, staged: HashMap::new(), next_handle: 0, casters: HashMap::new() }
    }

    pub fn handle(&mut self) -> u32 {
        self.next_handle += 1;
        self.next_handle
    }

    /// The scene origin, set from the first object with geometry: its bounding-box centre rounded
    /// to the metre.
    pub fn origin(&mut self) -> Option<[f64; 3]> {
        if self.origin.is_none() {
            let centre = self.session.project().objects().find_map(|o| bounds(o)).map(|b| {
                let c = b.centre();
                [c[0].round(), c[1].round(), c[2].round()]
            });
            self.origin = centre;
        }
        self.origin
    }
}

/// An identity for an object's geometry that survives revisions that keep it (a rename shares
/// the mesh), so the viewport and snap index rebuild only when the geometry changes.
pub fn geometry_key(body: &Body) -> usize {
    match body {
        Body::Surface(s) => Arc::as_ptr(s.mesh_arc()) as usize,
        Body::Solid(s) => Arc::as_ptr(s.mesh_arc()) as usize,
        Body::Grid(g) => Arc::as_ptr(g) as usize,
        Body::String(_) => 0,
    }
}

/// Grids are drawn and snapped to only when they hold elevations; quality and thickness grids
/// (`role` = `property`) are not surfaces.
pub fn is_drawn_surface(object: &Object) -> bool {
    match &object.body {
        Body::Surface(_) | Body::Solid(_) => true,
        Body::Grid(_) => object.attributes.get("role") != Some(&AttrValue::Text("property".into())),
        Body::String(_) => false,
    }
}

pub fn bounds(object: &Object) -> Option<zd_core::Aabb> {
    match &object.body {
        Body::Surface(s) => s.mesh().bounds(),
        Body::Solid(s) => s.mesh().bounds(),
        Body::String(line) => zd_core::Aabb::from_points(&line.vertices),
        Body::Grid(g) => {
            if !is_drawn_surface(object) {
                return None;
            }
            let live = g.z.iter().copied().filter(|z| !z.is_nan());
            let (lo, hi) = live.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), z| (a.min(z), b.max(z)));
            if !lo.is_finite() {
                return None;
            }
            let far = g.node_xy(g.cols - 1, g.rows - 1);
            Some(zd_core::Aabb { min: [g.origin[0], g.origin[1], lo], max: [far[0], far[1], hi] })
        }
    }
}
