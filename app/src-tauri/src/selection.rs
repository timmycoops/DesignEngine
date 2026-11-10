//! What a selection of several objects needs from the engine: where they are together, to zoom to them; which
//! objects on show lie wholly inside a box drawn on screen, to select them; and the selected solids as one mesh,
//! to light them up. View state: nothing here changes the project, and none of it is a command.

use std::sync::Arc;

use zd_core::{Aabb, Body, Object, ObjectId, Project, TriMesh};

use crate::state::{bounds, is_drawn_surface};

/// The box round every one of `ids` that has a place (a face design or a property grid has none), in world
/// coordinates. `None` if none has.
pub fn bounds_of(project: &Project, ids: &[ObjectId]) -> Option<Aabb> {
    ids.iter().filter_map(|&id| project.object(id)).filter_map(|o| bounds(o)).reduce(|a, b| Aabb {
        min: [0, 1, 2].map(|k| a.min[k].min(b.min[k])),
        max: [0, 1, 2].map(|k| a.max[k].max(b.max[k])),
    })
}

/// A box drawn on screen: the camera's view-projection matrix in the scene's local frame, column-major as three.js
/// keeps it, the scene origin it's local to, and the box's corners in normalised device coordinates (-1 to 1, y up).
#[derive(Debug, Clone, Copy)]
pub struct ScreenBox {
    pub matrix: [f64; 16],
    pub origin: [f64; 3],
    pub min: [f64; 2],
    pub max: [f64; 2],
}

impl ScreenBox {
    /// Whether a world point lands inside the box, in front of the camera.
    fn holds(&self, p: [f64; 3]) -> bool {
        let [x, y, z] = [p[0] - self.origin[0], p[1] - self.origin[1], p[2] - self.origin[2]];
        let m = &self.matrix;
        // 1 for an orthographic camera; for a perspective one, the depth in front of it.
        let w = m[3] * x + m[7] * y + m[11] * z + m[15];
        if w <= 0.0 {
            return false;
        }
        let sx = (m[0] * x + m[4] * y + m[8] * z + m[12]) / w;
        let sy = (m[1] * x + m[5] * y + m[9] * z + m[13]) / w;
        sx >= self.min[0] && sx <= self.max[0] && sy >= self.min[1] && sy <= self.max[1]
    }

    /// Whether all of a box lands inside. A camera draws straight lines straight, so the box's eight corners
    /// inside means everything in it is.
    fn holds_box(&self, b: &Aabb) -> bool {
        (0..8).all(|i| self.holds([[b.min[0], b.max[0]][i & 1], [b.min[1], b.max[1]][i >> 1 & 1], [b.min[2], b.max[2]][i >> 2]]))
    }

    /// Whether all of an object lands inside: its box if that does, else every point of it. A seam model's grids
    /// stay in their files, so it's its box alone.
    fn holds_object(&self, object: &Object) -> bool {
        let Some(b) = bounds(object) else { return false };
        if self.holds_box(&b) {
            return true;
        }
        match &object.body {
            Body::String(line) => line.vertices.iter().all(|&p| self.holds(p)),
            Body::Surface(s) => s.mesh().vertices.iter().all(|&p| self.holds(p)),
            Body::Solid(s) => s.mesh().vertices.iter().all(|&p| self.holds(p)),
            // Only its live nodes: a grid's corners are often nulls.
            Body::Grid(g) => (0..g.rows).all(|r| (0..g.cols).all(|c| g.z_at(c, r).is_nan() || self.holds({
                let [x, y] = g.node_xy(c, r);
                [x, y, g.z_at(c, r)]
            }))),
            Body::SeamModel(_) | Body::FaceDesign(_) => false,
        }
    }
}

/// Everything on show that lies wholly inside a box drawn on screen, in project order: strings, surfaces, grids,
/// solids and seam models. Hidden objects and those on hidden layers are left out, as they aren't drawn.
pub fn objects_in_box(project: &Project, screen: &ScreenBox) -> Vec<ObjectId> {
    let on_show = |o: &Object| !project.is_hidden(o.id) && project.layer(o.layer).is_some_and(|l| l.visible);
    let pickable = |o: &Object| matches!(o.body, Body::String(_) | Body::SeamModel(_)) || is_drawn_surface(o);
    project.objects().filter(|o| on_show(o) && pickable(o) && screen.holds_object(o)).map(|o| o.id).collect()
}

/// The solids among `ids`, on show, as they're drawn: hidden ones aren't.
pub fn solids_on_show(project: &Project, ids: &[ObjectId]) -> Vec<Arc<TriMesh>> {
    ids.iter()
        .filter_map(|&id| project.object(id))
        .filter(|o| !project.is_hidden(o.id) && project.layer(o.layer).is_some_and(|l| l.visible))
        .filter_map(|o| match &o.body {
            Body::Solid(s) => Some(s.mesh_arc().clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zd_commands::Session;

    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A 10 m cube with its south-west foot at (`x`, `y`, `z`) from the origin below, as `solid.from_mesh` parameters.
    fn cube(name: &str, x: f64, y: f64, z: f64) -> serde_json::Value {
        let v = (0..8).map(|i| [E + x + (i & 1) as f64 * 10.0, N + y + (i >> 1 & 1) as f64 * 10.0, z + (i >> 2) as f64 * 10.0]).collect::<Vec<_>>();
        let t = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        json!({ "name": name, "layer": "coal", "vertices": v, "triangles": t })
    }

    /// Looking straight down on a plan 200 m square centred on (E + 50, N + 50), as three.js's orthographic
    /// camera has it: x east and y north on screen, -1 to 1 across.
    fn plan_box(min: [f64; 2], max: [f64; 2]) -> ScreenBox {
        let s = 1.0 / 100.0;
        // Column-major: x' = s (x - 50), y' = s (y - 50), w = 1. The origin is (E, N, 0).
        let matrix = [s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, -0.001, 0.0, -50.0 * s, -50.0 * s, 0.0, 1.0];
        ScreenBox { matrix, origin: [E, N, 0.0], min, max }
    }

    fn two_cubes_and_a_string() -> (Session, [ObjectId; 3]) {
        let mut s = Session::with_builtins();
        let west = s.run("solid.from_mesh", cube("west", 0.0, 0.0, 100.0)).unwrap().outputs[0].id;
        let east = s.run("solid.from_mesh", cube("east", 60.0, 0.0, 100.0)).unwrap().outputs[0].id;
        let line = s.run("string.create", json!({ "name": "toe", "layer": "design", "points": [[E + 20.0, N + 40.0, 100.0], [E + 50.0, N + 40.0, 100.0]] })).unwrap().outputs[0].id;
        (s, [west, east, line])
    }

    #[test]
    fn the_box_round_several_objects_holds_them_all() {
        let (s, [west, east, line]) = two_cubes_and_a_string();
        let b = bounds_of(s.project(), &[west, line]).unwrap();
        assert_eq!((b.min, b.max), ([E, N, 100.0], [E + 50.0, N + 40.0, 110.0]));
        let b = bounds_of(s.project(), &[west, east]).unwrap();
        assert_eq!((b.min, b.max), ([E, N, 100.0], [E + 70.0, N + 10.0, 110.0]));
        assert!(bounds_of(s.project(), &[]).is_none());
        assert!(bounds_of(s.project(), &[ObjectId::new()]).is_none(), "an object that's gone has no place");
    }

    /// A box takes what lies wholly inside it, and nothing it only crosses.
    #[test]
    fn a_box_on_screen_takes_what_lies_wholly_inside_it() {
        let (mut s, [west, east, line]) = two_cubes_and_a_string();
        // Screen x = (east - 50) / 100. The west cube spans -0.5 to -0.4, the east 0.1 to 0.2, the string -0.3 to 0.
        let every = plan_box([-1.0, -1.0], [1.0, 1.0]);
        assert_eq!(objects_in_box(s.project(), &every), vec![west, east, line]);
        assert_eq!(objects_in_box(s.project(), &plan_box([-0.55, -0.6], [-0.35, -0.3])), vec![west]);
        assert_eq!(objects_in_box(s.project(), &plan_box([-0.45, -0.6], [0.3, 0.0])), vec![east, line], "half the west cube isn't in it");
        assert!(objects_in_box(s.project(), &plan_box([-0.25, -0.6], [0.3, 0.0])).iter().all(|&id| id != line), "nor is half the string");

        s.set_objects_visible(&[east], false);
        assert_eq!(objects_in_box(s.project(), &every), vec![west, line], "hidden, so not drawn, so not taken");
    }

    /// In perspective a point behind the camera can land in the box by the sums alone; it isn't taken.
    #[test]
    fn nothing_behind_the_camera_is_taken() {
        let (s, [west, ..]) = two_cubes_and_a_string();
        // Looking north, 10 m south of the cube: w is the depth northwards, so the cube is in front.
        let ahead = |y0: f64| ScreenBox {
            // x' = x, y' = z - 105, w = y - y0 (column-major).
            matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, -5.0, -105.0, 0.0, -y0],
            origin: [E, N, 0.0],
            min: [-1.0, -1.0],
            max: [1.0, 1.0],
        };
        assert!(objects_in_box(s.project(), &ahead(-10.0)).contains(&west));
        assert!(!objects_in_box(s.project(), &ahead(30.0)).contains(&west), "the camera is north of it, looking away");
    }

    /// PIT B's block solids (`testdata/golden`, git-ignored), imported and repaired as the app bench does, then the
    /// three calls timed on all of them: `cargo test -p zd-app --release pit_b -- --ignored --nocapture`.
    #[test]
    #[ignore = "golden data, run by hand"]
    fn pit_b_blocks_timed() {
        use std::time::Instant;
        let zip = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden/Data/PIT B/relimits.zip");
        assert!(zip.exists(), "no golden data at {}", zip.display());
        let mut s = Session::with_builtins();
        s.run("solid.import_many", json!({ "paths": [zip], "layer": "solids", "repair": true })).unwrap();
        let project = s.project();
        let ids: Vec<ObjectId> = project.objects().map(|o| o.id).collect();
        let t = Instant::now();
        let b = bounds_of(project, &ids).unwrap();
        let bounds_ms = t.elapsed().as_secs_f64() * 1e3;
        // Plan, straight down, the pit filling the view with 5 % to spare.
        let [w, h, _] = b.size();
        let (sx, sy) = (1.0 / (0.525 * w), 1.0 / (0.525 * h));
        let plan = |min: [f64; 2], max: [f64; 2]| ScreenBox {
            matrix: [sx, 0.0, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 0.0, -1e-4, 0.0, 0.0, 0.0, 0.0, 1.0],
            origin: b.centre(),
            min,
            max,
        };
        let t = Instant::now();
        let every = objects_in_box(project, &plan([-1.0, -1.0], [1.0, 1.0]));
        let every_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let west = objects_in_box(project, &plan([-1.0, -1.0], [0.0, 1.0]));
        let west_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let joined = crate::state::joined_meshes(&solids_on_show(project, &ids));
        let joined_ms = t.elapsed().as_secs_f64() * 1e3;
        eprintln!(
            "PIT B: {} solids; bounds of all {bounds_ms:.1} ms; box round the pit {} in {every_ms:.1} ms; west half {} in {west_ms:.1} ms; all joined {} triangles in {joined_ms:.1} ms",
            ids.len(),
            every.len(),
            west.len(),
            joined.triangle_count()
        );
        assert_eq!(every.len(), ids.len(), "the whole pit is inside a box round it");
        assert!(!west.is_empty() && west.len() < ids.len());
    }

    #[test]
    fn the_selected_solids_on_show_draw_as_one() {
        let (mut s, [west, east, line]) = two_cubes_and_a_string();
        let joined = crate::state::joined_meshes(&solids_on_show(s.project(), &[west, east, line]));
        assert!((joined.signed_volume() - 2000.0).abs() < 1e-6, "{}", joined.signed_volume());
        s.set_objects_visible(&[west], false);
        assert_eq!(solids_on_show(s.project(), &[west, east, line]).len(), 1);
    }
}
