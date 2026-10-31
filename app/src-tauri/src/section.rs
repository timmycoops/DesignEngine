//! The section view's lines: where a plane cuts everything on the visible layers, by layer and kind.
//! A section is view state (decision 4 of the Phase 2 plan): nothing here is a command or changes
//! the project. Solids cut as one mesh per layer, as they draw, and seam models as their working
//! horizons' roofs and floors. What takes work to make is kept until its geometry changes, so
//! stepping along a section costs only the cut.

use std::collections::HashMap;
use std::sync::Arc;

use zd_core::{Body, LayerId, ObjectKind, TriMesh};
use zd_geom::{SectionPlane, SliceIndex, slice_indexed, string_near_plane, triangulate_grid};
use zd_io::read_stack_grids;

use crate::state::{AppInner, geometry_key, is_drawn_surface, joined_meshes, layer_solids, solid_layer_keys};

/// Two ends of a segment, in mine grid coordinates.
pub type Segment = [[f64; 3]; 2];

/// One layer's lines of one kind.
pub struct SectionGroup {
    pub layer: LayerId,
    pub kind: &'static str,
    pub segments: Vec<Segment>,
}

/// A mesh ready to cut.
pub struct SectionMesh {
    mesh: Arc<TriMesh>,
    index: SliceIndex,
}

/// Meshes made for sections, each with a stamp of what it was made from.
pub type SectionMeshes = HashMap<String, (String, Arc<SectionMesh>)>;

/// The lines where `plane` cuts every visible layer. Strings within `width` of the plane draw flat on it.
pub fn section_groups(app: &mut AppInner, plane: &SectionPlane, width: f64) -> Result<Vec<SectionGroup>, String> {
    let mut groups: Vec<SectionGroup> = Vec::new();
    let project = app.session.project();
    let visible = |layer: LayerId| project.layer(layer).is_some_and(|l| l.visible);
    let mut meshes: Vec<(LayerId, &'static str, Arc<SectionMesh>)> = Vec::new();
    let mut made = std::mem::take(&mut app.section_meshes);
    let mut kept = SectionMeshes::new();
    let mut keep = |key: String, stamp: String, make: &mut dyn FnMut() -> Result<Arc<TriMesh>, String>| -> Result<Arc<SectionMesh>, String> {
        let entry = match made.remove(&key) {
            Some(entry) if entry.0 == stamp => entry,
            _ => {
                let mesh = make()?;
                let index = SliceIndex::new(&mesh);
                (stamp, Arc::new(SectionMesh { mesh, index }))
            }
        };
        let mesh = entry.1.clone();
        kept.insert(key, entry);
        Ok(mesh)
    };

    for (layer, key) in solid_layer_keys(project) {
        if visible(layer) {
            let mesh = keep(format!("solids {layer}"), key, &mut || Ok(Arc::new(joined_meshes(&layer_solids(project, layer)))))?;
            meshes.push((layer, "solid", mesh));
        }
    }
    for object in project.objects().filter(|o| visible(o.layer)) {
        let stamp = geometry_key(&object.body).to_string();
        match &object.body {
            Body::Surface(s) => {
                let mesh = keep(object.id.to_string(), stamp, &mut || Ok(s.mesh_arc().clone()))?;
                meshes.push((object.layer, "surface", mesh));
            }
            Body::Grid(g) if is_drawn_surface(object) => {
                let mesh = keep(object.id.to_string(), stamp, &mut || Ok(Arc::new(triangulate_grid(g))))?;
                meshes.push((object.layer, "surface", mesh));
            }
            Body::SeamModel(model) => {
                let mesh = keep(object.id.to_string(), stamp, &mut || {
                    let grids = read_stack_grids(model).map_err(|e| e.to_string())?;
                    let working = model.horizons.iter().enumerate().filter(|(_, h)| h.working).map(|(i, _)| i);
                    let parts: Vec<Arc<TriMesh>> = working.flat_map(|i| [&grids.roofs[i], &grids.floors[i]]).map(|g| Arc::new(triangulate_grid(g))).collect();
                    Ok(Arc::new(joined_meshes(&parts)))
                })?;
                meshes.push((object.layer, "seam", mesh));
            }
            _ => {}
        }
    }
    for (layer, kind, section) in meshes {
        let mut segments = Vec::new();
        for line in slice_indexed(&section.mesh, &section.index, plane) {
            let closing = line.closed.then(|| [line.points[line.points.len() - 1], line.points[0]]);
            segments.extend(line.points.windows(2).map(|w| [w[0], w[1]]).chain(closing));
        }
        add(&mut groups, layer, kind, segments);
    }
    for object in project.objects().filter(|o| o.kind() == ObjectKind::String && visible(o.layer)) {
        if let Body::String(line) = &object.body {
            add(&mut groups, object.layer, "string", string_near_plane(&line.vertices, line.closed, plane, width));
        }
    }
    app.section_meshes = kept;
    Ok(groups)
}

/// Adds segments to the layer's group of that kind.
fn add(groups: &mut Vec<SectionGroup>, layer: LayerId, kind: &'static str, segments: Vec<Segment>) {
    if segments.is_empty() {
        return;
    }
    match groups.iter_mut().find(|g| g.layer == layer && g.kind == kind) {
        Some(group) => group.segments.extend(segments),
        None => groups.push(SectionGroup { layer, kind, segments }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zd_commands::Session;
    use zd_core::Project;

    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A 10 m cube at `x` as `solid.from_mesh` parameters.
    fn cube(name: &str, layer: &str, x: f64) -> serde_json::Value {
        let v = (0..8).map(|i| [E + x + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]).collect::<Vec<_>>();
        let t = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        json!({ "name": name, "layer": layer, "vertices": v, "triangles": t })
    }

    fn length(segments: &[Segment]) -> f64 {
        segments.iter().map(|[p, q]| ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt()).sum()
    }

    fn group<'a>(groups: &'a [SectionGroup], project: &Project, layer: &str, kind: &str) -> Option<&'a SectionGroup> {
        let id = project.layers().iter().find(|l| l.name == layer)?.id;
        groups.iter().find(|g| g.layer == id && g.kind == kind)
    }

    #[test]
    fn a_section_cuts_solids_by_layer_draws_near_strings_and_skips_hidden_layers() {
        let mut s = Session::with_builtins();
        s.run("solid.from_mesh", cube("a", "coal", 0.0)).unwrap();
        s.run("solid.from_mesh", cube("b", "coal", 0.0 + 20.0)).unwrap();
        s.run("solid.from_mesh", cube("hidden", "waste", 40.0)).unwrap();
        // One string along the plane within the width, one well off it.
        s.run("string.create", json!({ "name": "near", "layer": "design", "points": [[E, N + 5.5, 90.0], [E + 30.0, N + 5.5, 90.0]] })).unwrap();
        s.run("string.create", json!({ "name": "far", "layer": "design", "points": [[E, N + 50.0, 90.0], [E + 30.0, N + 50.0, 90.0]] })).unwrap();
        let waste = s.project().layers().iter().find(|l| l.name == "waste").unwrap().id;
        s.set_layer_visible(waste, false);
        let mut app = AppInner::new(s, None);

        // A plane along y = N + 5, west to east, through both cubes on "coal" and the hidden one.
        let plane = SectionPlane::through([E - 10.0, N + 5.0], [E + 60.0, N + 5.0]).unwrap();
        let groups = section_groups(&mut app, &plane, 1.0).unwrap();
        let project = app.session.project();
        let coal = group(&groups, project, "coal", "solid").expect("the coal layer's cut");
        assert!((length(&coal.segments) - 80.0).abs() < 1e-9, "two 10 m squares: {}", length(&coal.segments));
        assert!(group(&groups, project, "waste", "solid").is_none(), "a hidden layer is not cut");
        let strings = group(&groups, project, "design", "string").expect("the near string");
        assert!((length(&strings.segments) - 30.0).abs() < 1e-9, "only the near string: {}", length(&strings.segments));
        assert!(strings.segments.iter().flatten().all(|&p| plane.offset(p).abs() < 1e-9), "drawn flat on the plane");
    }

    #[test]
    fn stepping_reuses_what_it_made_until_the_geometry_changes() {
        let mut s = Session::with_builtins();
        s.run("solid.from_mesh", cube("a", "coal", 0.0)).unwrap();
        let mut app = AppInner::new(s, None);
        let plane = SectionPlane::through([E - 10.0, N + 5.0], [E + 60.0, N + 5.0]).unwrap();
        section_groups(&mut app, &plane, 1.0).unwrap();
        let first = app.section_meshes.values().next().unwrap().1.clone();
        section_groups(&mut app, &plane.stepped(1.0), 1.0).unwrap();
        assert!(Arc::ptr_eq(&first, &app.section_meshes.values().next().unwrap().1), "kept between steps");

        app.session.run("solid.from_mesh", cube("b", "coal", 20.0)).unwrap();
        let groups = section_groups(&mut app, &plane, 1.0).unwrap();
        assert!(!Arc::ptr_eq(&first, &app.section_meshes.values().next().unwrap().1), "made again when a solid is added");
        assert!((length(&groups[0].segments) - 80.0).abs() < 1e-9);
    }
}
