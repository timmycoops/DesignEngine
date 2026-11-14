use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use zd_commands::{RunControl, Session};
use zd_core::{AttrValue, Body, LayerId, Object, ObjectId, Project, TriMesh};
use zd_geom::{GeomError, SurfaceRayCaster};

use crate::changes::Sent;
use crate::section::{SectionCut, SectionInputs, SectionMeshes};

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
    /// The project file last opened or saved (or an older `.zdproj` folder opened, which saves as a `.zdp` beside it).
    pub path: Option<PathBuf>,
    /// Who else had the file open when it was opened: a save to it is refused.
    pub locked_by: Option<String>,
    /// Local origin shared by every buffer sent to the viewport (engine rule 2); set from the
    /// first object with geometry and kept for the life of the project.
    pub origin: Option<[f64; 3]>,
    /// Encoded render buffers waiting to be fetched.
    pub staged: HashMap<u32, Vec<u8>>,
    pub next_handle: u32,
    /// Snap index per drawn object, with the geometry it was built from. Solids are not here.
    pub casters: HashMap<ObjectId, (usize, Arc<SurfaceRayCaster>)>,
    /// Snap index per tile of a layer's solids, with the key of the solids it was built from.
    pub solid_casters: HashMap<TileId, (String, Arc<SolidCaster>)>,
    /// Each tile of each layer's solids as it stands: see [`solid_tile_keys`].
    pub solid_keys: BTreeMap<TileId, String>,
    /// Which tile each solid is in, kept with the mesh it was worked out from.
    pub tiles: Tiles,
    /// Meshes made for the section view, kept between steps.
    pub section_meshes: SectionMeshes,
    /// What the section view cuts, kept until the project changes.
    pub section_inputs: Option<Arc<SectionInputs>>,
    /// The last section cut, for snapping to.
    pub section_cut: Option<SectionCut>,
    /// What the app was last sent of the project, to send it only what changes.
    pub sent: Sent,
    /// Which session's run log the app is sent: a new one with each project opened or started.
    pub log_session: u64,
    /// The run going, if one is: one at a time, and nothing else changes the project until it ends (decision 0060).
    pub running: Option<Running>,
}

/// A run going off the lock: what it is, since when, and how to watch and stop it.
pub struct Running {
    pub command: String,
    pub started: std::time::Instant,
    pub control: Arc<RunControl>,
}

impl AppInner {
    /// Refuses a change to the project while a run goes: it would make the run's work stale (decision 0060).
    pub fn idle(&self) -> Result<(), String> {
        match &self.running {
            Some(r) => Err(format!("Waiting on {}: the project can't change until it's done", r.command)),
            None => Ok(()),
        }
    }
}

/// A number no session's log has had before, in this run of the app.
fn next_log_session() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl AppInner {
    pub fn new(session: Session, path: Option<PathBuf>) -> Self {
        let locked_by = session.locked_by();
        AppInner { session, path, locked_by, origin: None, staged: HashMap::new(), next_handle: 0, casters: HashMap::new(), solid_casters: HashMap::new(), solid_keys: BTreeMap::new(), tiles: Tiles::default(), section_meshes: SectionMeshes::new(), section_inputs: None, section_cut: None, sent: Sent::default(), log_session: next_log_session(), running: None }
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
        Body::FaceDesign(d) => Arc::as_ptr(d) as usize,
        Body::Workflow(w) => Arc::as_ptr(w) as usize,
        Body::String(_) => 0,
    }
}

/// A key for each layer holding solids that aren't hidden, changed by any change to their geometry or
/// by hiding or showing one. Solids are drawn and cut by a section by layer, so tens of thousands of
/// them are one mesh rather than one each.
pub fn solid_layer_keys(project: &Project) -> BTreeMap<LayerId, String> {
    layer_keys(project, |object| !project.is_hidden(object.id))
}

fn layer_keys(project: &Project, keep: impl Fn(&Object) -> bool) -> BTreeMap<LayerId, String> {
    let mut layers: BTreeMap<LayerId, (usize, u64, u64)> = BTreeMap::new();
    for object in project.objects() {
        if matches!(object.body, Body::Solid(_)) && keep(object) {
            let key = (geometry_key(&object.body) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(23);
            let (count, sum, either) = layers.entry(object.layer).or_default();
            *count += 1;
            *sum = sum.wrapping_add(key);
            *either ^= key;
        }
    }
    layers.into_iter().map(|(layer, (count, sum, either))| (layer, format!("{count}-{sum:x}-{either:x}"))).collect()
}

/// A tile's side (m): a layer's solids are drawn and snapped to a tile at a time, by where each sits in plan, so a change
/// to one sends one tile's mesh and builds one tile's snap index (decision 0060, workstream 4). Fixed, so a solid's tile
/// never depends on the others: a pit's blocks are a few thousand to a tile.
pub const TILE_M: f64 = 1000.0;

/// One tile of one layer's solids: the square of [`TILE_M`] holding the middle of each solid's plan, counted from the
/// grid's zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct TileId {
    pub layer: LayerId,
    pub col: i64,
    pub row: i64,
}

/// Which tile each solid is in, kept with the mesh it was worked out from, so a pit's solids aren't measured again
/// after every change. A mesh is held while it's here, so its address can't be another's.
#[derive(Default)]
pub struct Tiles {
    of: HashMap<ObjectId, (Arc<TriMesh>, [i64; 2])>,
    /// Each tile's solids as the keys were last worked out, in the order they were made: a tile's mesh or snap index is
    /// made from these, not by looking through every object again.
    members: HashMap<TileId, Vec<ObjectId>>,
}

impl Tiles {
    /// The tile in plan of a solid's mesh: where the middle of its plan lies.
    pub fn place(mesh: &TriMesh) -> [i64; 2] {
        mesh.bounds().map_or([0, 0], |b| {
            let c = b.centre();
            [(c[0] / TILE_M).floor() as i64, (c[1] / TILE_M).floor() as i64]
        })
    }

    fn of(&mut self, id: ObjectId, mesh: &Arc<TriMesh>) -> [i64; 2] {
        match self.of.get(&id) {
            Some((was, at)) if Arc::ptr_eq(was, mesh) => *at,
            _ => {
                let at = Tiles::place(mesh);
                self.of.insert(id, (mesh.clone(), at));
                at
            }
        }
    }

    /// Forgets solids gone, once there are many more remembered than live.
    fn prune(&mut self, live: usize, project: &Project) {
        if self.of.len() > 2 * live + 1024 {
            self.of.retain(|id, _| project.object(*id).is_some());
        }
    }
}

/// A key for each tile of each layer's solids, hidden or not, changed by any change to the geometry of a solid in it,
/// one coming into it or one leaving. Hiding one changes no key: it's a table beside the mesh.
pub fn solid_tile_keys(project: &Project, tiles: &mut Tiles) -> BTreeMap<TileId, String> {
    let mut keys: BTreeMap<TileId, (usize, u64, u64)> = BTreeMap::new();
    let mut members: HashMap<TileId, Vec<ObjectId>> = HashMap::new();
    let mut live = 0;
    for object in project.objects() {
        let Body::Solid(solid) = &object.body else { continue };
        live += 1;
        let [col, row] = tiles.of(object.id, solid.mesh_arc());
        let tile = TileId { layer: object.layer, col, row };
        let key = (geometry_key(&object.body) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(23);
        let (count, sum, either) = keys.entry(tile).or_default();
        *count += 1;
        *sum = sum.wrapping_add(key);
        *either ^= key;
        members.entry(tile).or_default().push(object.id);
    }
    tiles.prune(live, project);
    tiles.members = members;
    keys.into_iter().map(|(tile, (count, sum, either))| (tile, format!("{count}-{sum:x}-{either:x}"))).collect()
}

/// A tile's solids, hidden or not, with their meshes, in the order they were made.
pub fn tile_solids(project: &Project, tiles: &mut Tiles, tile: TileId) -> Vec<(ObjectId, Arc<TriMesh>)> {
    // From the solids the keys were worked out from, each checked still to be in the tile.
    if let Some(ids) = tiles.members.get(&tile).cloned() {
        return ids
            .into_iter()
            .filter_map(|id| {
                let object = project.object(id)?;
                match &object.body {
                    Body::Solid(s) if object.layer == tile.layer && tiles.of(id, s.mesh_arc()) == [tile.col, tile.row] => Some((id, s.mesh_arc().clone())),
                    _ => None,
                }
            })
            .collect();
    }
    project
        .objects()
        .filter(|o| o.layer == tile.layer)
        .filter_map(|o| match &o.body {
            Body::Solid(s) => (tiles.of(o.id, s.mesh_arc()) == [tile.col, tile.row]).then(|| (o.id, s.mesh_arc().clone())),
            _ => None,
        })
        .collect()
}

/// The meshes of a layer's solids that aren't hidden.
pub fn layer_solids(project: &Project, layer: LayerId) -> Vec<Arc<TriMesh>> {
    every_layer_solid(project, layer).into_iter().filter(|(id, _)| !project.is_hidden(*id)).map(|(_, mesh)| mesh).collect()
}

/// A layer's solids, hidden or not, with their meshes.
pub fn every_layer_solid(project: &Project, layer: LayerId) -> Vec<(ObjectId, Arc<TriMesh>)> {
    project
        .objects()
        .filter(|o| o.layer == layer)
        .filter_map(|o| match &o.body {
            Body::Solid(s) => Some((o.id, s.mesh_arc().clone())),
            _ => None,
        })
        .collect()
}

/// A snap index over a layer's solids as one mesh, and which solid each triangle came from. Hiding a
/// solid then needs no new index, which takes seconds on a pit: a cast passes over it.
pub struct SolidCaster {
    caster: SurfaceRayCaster,
    /// Each solid's first triangle, in order.
    first_triangles: Vec<u32>,
    ids: Vec<ObjectId>,
}

impl SolidCaster {
    pub fn new(solids: &[(ObjectId, Arc<TriMesh>)]) -> Result<Self, GeomError> {
        let meshes: Vec<Arc<TriMesh>> = solids.iter().map(|(_, mesh)| mesh.clone()).collect();
        let caster = SurfaceRayCaster::new(&joined_meshes(&meshes))?;
        let mut next = 0;
        let first_triangles = meshes
            .iter()
            .map(|mesh| {
                let first = next;
                next += mesh.triangle_count() as u32;
                first
            })
            .collect();
        Ok(SolidCaster { caster, first_triangles, ids: solids.iter().map(|(id, _)| *id).collect() })
    }

    fn solid_of(&self, triangle: u32) -> ObjectId {
        self.ids[self.first_triangles.partition_point(|&first| first <= triangle) - 1]
    }

    /// Nearest hit on a solid that isn't hidden, and which solid it is.
    pub fn cast(&self, from: [f64; 3], direction: [f64; 3], hidden: &HashSet<ObjectId>) -> Option<(ObjectId, [f64; 3])> {
        self.caster.cast_where(from, direction, |triangle| !hidden.contains(&self.solid_of(triangle))).map(|(triangle, hit)| (self.solid_of(triangle), hit))
    }
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
        Body::String(_) | Body::SeamModel(_) | Body::FaceDesign(_) | Body::Workflow(_) => false,
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
        Body::FaceDesign(_) | Body::Workflow(_) => None,
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
        cube_at(name, x, 100.0)
    }

    fn cube_at(name: &str, x: f64, z: f64) -> serde_json::Value {
        let v = (0..8).map(|i| [650_000.0 + x + (i & 1) as f64 * 10.0, 7_450_000.0 + (i >> 1 & 1) as f64 * 10.0, z + (i >> 2) as f64 * 10.0]).collect::<Vec<_>>();
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

    /// Hiding changes what a section cuts and not a tile's mesh or snap index.
    #[test]
    fn a_hidden_solid_leaves_the_layers_mesh_and_a_snap_passes_over_it() {
        let mut s = Session::with_builtins();
        let upper = s.run("solid.from_mesh", cube_at("upper", 0.0, 200.0)).unwrap().outputs[0].id;
        let lower = s.run("solid.from_mesh", cube_at("lower", 0.0, 100.0)).unwrap().outputs[0].id;
        let layer = s.project().objects().next().unwrap().layer;
        let mut tiles = Tiles::default();
        let (drawn, snapped) = (solid_layer_keys(s.project()), solid_tile_keys(s.project(), &mut tiles));

        s.set_objects_visible(&[upper], false);
        assert_ne!(solid_layer_keys(s.project()), drawn, "a section cuts the layer again");
        assert_eq!(solid_tile_keys(s.project(), &mut tiles), snapped, "the tile's mesh and snap index stand");
        let shown = joined_meshes(&layer_solids(s.project(), layer));
        assert!((shown.signed_volume() - 1000.0).abs() < 1e-6, "{}", shown.signed_volume());

        // Straight down through both cubes, either of which can come first in the index.
        let caster = SolidCaster::new(&every_layer_solid(s.project(), layer)).unwrap();
        let (from, down) = ([650_004.0, 7_450_003.0, 500.0], [0.0, 0.0, -1.0]);
        assert_eq!(caster.cast(from, down, &HashSet::new()).unwrap(), (upper, [from[0], from[1], 210.0]));
        assert_eq!(caster.cast(from, down, s.project().hidden()).unwrap(), (lower, [from[0], from[1], 110.0]));
        s.set_objects_visible(&[lower], false);
        assert_eq!(caster.cast(from, down, s.project().hidden()), None);
        assert!(solid_layer_keys(s.project()).is_empty(), "a layer with every solid hidden draws nothing");
        s.set_objects_visible(&[upper, lower], true);
        assert_eq!(solid_layer_keys(s.project()), drawn, "shown again, the same key, so nothing is fetched");
        assert_eq!(caster.cast(from, down, s.project().hidden()).unwrap().0, upper);
    }

    /// A layer's solids by plan tile: a change to one changes its tile's key alone, and a solid moved into another tile
    /// changes both.
    #[test]
    fn a_change_to_one_solid_changes_its_tile_alone() {
        let mut s = Session::with_builtins();
        // Two cubes in one 1,000 m tile and one 2 km east, all on one layer.
        let a = s.run("solid.from_mesh", cube("a", 0.0)).unwrap().outputs[0].id;
        s.run("solid.from_mesh", cube("b", 20.0)).unwrap();
        s.run("solid.from_mesh", cube("far", 2000.0)).unwrap();
        let mut tiles = Tiles::default();
        let keys = solid_tile_keys(s.project(), &mut tiles);
        assert_eq!(keys.len(), 2, "{keys:?}");
        let near = *keys.keys().find(|t| tile_solids(s.project(), &mut tiles, **t).len() == 2).unwrap();
        assert_eq!((near.col, near.row), (650, 7450), "the tile holding the middle of each cube's plan");

        s.run("object.rename", json!({ "object": a, "name": "renamed" })).unwrap();
        assert_eq!(solid_tile_keys(s.project(), &mut tiles), keys, "a rename shares the mesh: nothing to send");
        s.run("object.delete", json!({ "object": a })).unwrap();
        let after = solid_tile_keys(s.project(), &mut tiles);
        let changed: Vec<_> = keys.iter().filter(|(t, k)| after.get(t) != Some(k)).map(|(t, _)| *t).collect();
        assert_eq!(changed, vec![near], "one tile changed, the other stands");
        let whole = joined_meshes(&tile_solids(s.project(), &mut tiles, near).into_iter().map(|(_, m)| m).collect::<Vec<_>>());
        assert!((whole.signed_volume() - 1000.0).abs() < 1e-6, "one cube left in it: {}", whole.signed_volume());
    }
}
