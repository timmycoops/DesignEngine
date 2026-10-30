use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use zd_commands::Session;
use zd_core::{AttrValue, Body, LayerId, Object, ObjectId, Project, TriMesh};
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
    /// Snap index per drawn object, with the geometry it was built from. Solids are not here.
    pub casters: HashMap<ObjectId, (usize, Arc<SurfaceRayCaster>)>,
    /// Snap index per layer of solids, with the key of the solids it was built from.
    pub solid_casters: HashMap<LayerId, (String, Arc<SurfaceRayCaster>)>,
    /// Each layer's solids as they stand: see [`solid_layer_keys`].
    pub solid_keys: BTreeMap<LayerId, String>,
}

impl AppInner {
    pub fn new(session: Session, path: Option<PathBuf>) -> Self {
        AppInner { session, path, origin: None, staged: HashMap::new(), next_handle: 0, casters: HashMap::new(), solid_casters: HashMap::new(), solid_keys: BTreeMap::new() }
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
        Body::SeamModel(m) => Arc::as_ptr(m) as usize,
        Body::String(_) => 0,
    }
}

/// A key for each layer holding solids, changed by any change to their geometry. Solids are drawn
/// and snapped to by layer, so tens of thousands of them are one mesh rather than one each.
pub fn solid_layer_keys(project: &Project) -> BTreeMap<LayerId, String> {
    let mut layers: BTreeMap<LayerId, (usize, u64, u64)> = BTreeMap::new();
    for object in project.objects() {
        if matches!(object.body, Body::Solid(_)) {
            let key = (geometry_key(&object.body) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(23);
            let (count, sum, either) = layers.entry(object.layer).or_default();
            *count += 1;
            *sum = sum.wrapping_add(key);
            *either ^= key;
        }
    }
    layers.into_iter().map(|(layer, (count, sum, either))| (layer, format!("{count}-{sum:x}-{either:x}"))).collect()
}

/// The meshes of a layer's solids.
pub fn layer_solids(project: &Project, layer: LayerId) -> Vec<Arc<TriMesh>> {
    project
        .objects()
        .filter(|o| o.layer == layer)
        .filter_map(|o| match &o.body {
            Body::Solid(s) => Some(s.mesh_arc().clone()),
            _ => None,
        })
        .collect()
}

/// Meshes as one, for drawing and snapping only: nothing here is a solid of record.
pub fn joined_meshes(meshes: &[Arc<TriMesh>]) -> TriMesh {
    let mut whole = TriMesh::new(Vec::with_capacity(meshes.iter().map(|m| m.vertices.len()).sum()), Vec::with_capacity(meshes.iter().map(|m| m.triangles.len()).sum()));
    for mesh in meshes {
        let offset = whole.vertices.len() as u32;
        whole.vertices.extend_from_slice(&mesh.vertices);
        whole.triangles.extend(mesh.triangles.iter().map(|t| t.map(|i| i + offset)));
    }
    whole
}

/// Grids are drawn and snapped to only when they hold elevations; quality and thickness grids
/// (`role` = `property`) are not surfaces.
pub fn is_drawn_surface(object: &Object) -> bool {
    match &object.body {
        Body::Surface(_) | Body::Solid(_) => true,
        Body::Grid(_) => object.attributes.get("role") != Some(&AttrValue::Text("property".into())),
        Body::String(_) | Body::SeamModel(_) => false,
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
            let [lo, hi] = g.z_range()?;
            let far = g.node_xy(g.cols - 1, g.rows - 1);
            Some(zd_core::Aabb { min: [g.origin[0], g.origin[1], lo], max: [far[0], far[1], hi] })
        }
        // Its grids stay in their files; the build recorded the working section's elevation range.
        Body::SeamModel(m) => {
            let [lo, hi] = m.z_range?;
            let l = m.lattice;
            let far = [l.origin[0] + (l.cols - 1) as f64 * l.cell_size[0], l.origin[1] + (l.rows - 1) as f64 * l.cell_size[1]];
            Some(zd_core::Aabb { min: [l.origin[0], l.origin[1], lo], max: [far[0], far[1], hi] })
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A 10 m cube at `x` as `solid.from_mesh` parameters.
    fn cube(name: &str, x: f64) -> serde_json::Value {
        let v = (0..8).map(|i| [650_000.0 + x + (i & 1) as f64 * 10.0, 7_450_000.0 + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]).collect::<Vec<_>>();
        let t = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        json!({ "name": name, "layer": "coal", "vertices": v, "triangles": t })
    }

    #[test]
    fn a_layers_solids_join_as_one_mesh_under_a_key_that_follows_them() {
        let mut s = Session::with_builtins();
        s.run("solid.from_mesh", cube("a", 0.0)).unwrap();
        let second = s.run("solid.from_mesh", cube("b", 20.0)).unwrap().outputs[0].id;
        let project = s.project();
        let layer = project.objects().next().unwrap().layer;
        let keys = solid_layer_keys(project);
        assert_eq!(keys.len(), 1);
        let meshes = layer_solids(project, layer);
        let whole = joined_meshes(&meshes);
        assert_eq!((whole.vertices.len(), whole.triangle_count()), (16, 24));
        // The second cube's triangles use the second cube's vertices, so the volumes add: 2,000 m3.
        assert!((whole.signed_volume() - 2000.0).abs() < 1e-6, "{}", whole.signed_volume());

        s.run("object.delete", json!({ "object": second })).unwrap();
        assert_ne!(solid_layer_keys(s.project()), keys);
        s.undo();
        assert_eq!(solid_layer_keys(s.project()), keys, "the same solids give the same key, so nothing is fetched again");
    }
}
