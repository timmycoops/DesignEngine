use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zd_core::TriMesh;
use zd_geom::SurfaceRayCaster;

/// Surfaces held by the shell, in f64 world coordinates. The viewport sees them only as f32
/// offsets from `origin`.
#[derive(Default)]
pub struct AppState {
    pub scene: Mutex<Scene>,
}

#[derive(Default)]
pub struct Scene {
    /// Local origin shared by every buffer sent to the viewport; set by the first surface.
    pub origin: Option<[f64; 3]>,
    pub surfaces: Vec<LoadedSurface>,
    /// Encoded render buffers waiting to be fetched (transfer check).
    pub staged: HashMap<u32, Vec<u8>>,
    pub next_id: u32,
}

pub struct LoadedSurface {
    pub id: u32,
    pub mesh: Arc<TriMesh>,
    pub caster: Option<Arc<SurfaceRayCaster>>,
}

impl Scene {
    pub fn next_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// The scene origin, set from `mesh` if this is the first surface: its bounding-box centre
    /// rounded to the metre.
    pub fn origin_for(&mut self, mesh: &TriMesh) -> [f64; 3] {
        *self.origin.get_or_insert_with(|| {
            let c = mesh.bounds().map(|b| b.centre()).unwrap_or([0.0; 3]);
            [c[0].round(), c[1].round(), c[2].round()]
        })
    }
}
