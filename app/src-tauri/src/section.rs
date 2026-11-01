//! The section view's lines: where a plane cuts everything on the visible layers, by layer and kind.
//! A section is view state (decision 4 of the Phase 2 plan): nothing here is a command or changes
//! the project. Solids cut as one mesh per layer, as they draw, and seam models as each working
//! horizon's roof and floor, which is what lets them fill. What takes work to make is kept until its
//! geometry changes, and the list of what to cut until the project changes, so stepping along a
//! section costs only the cut: with tens of thousands of solids, finding them all again took twice
//! as long as cutting them.

use std::collections::HashMap;
use std::sync::Arc;

use zd_core::{Body, LayerId, Object, ObjectId, ObjectKind, TriMesh};
use zd_geom::{
    CutSegment, SectionFill, SectionPlane, SectionSnapKind, SliceIndex, SliceLine, fill_between_lines, fill_section_loops, slice_indexed, slice_mesh, snap_to_section, string_near_plane, triangulate_grid,
};
use zd_io::{StackGrids, read_stack_grids};

use crate::state::{AppInner, geometry_key, is_drawn_surface, joined_meshes, layer_solids, solid_layer_keys};

/// Two ends of a segment, in mine grid coordinates.
pub type Segment = CutSegment;

/// One layer's lines of one kind, one object's for a surface or seam model.
pub struct SectionGroup {
    pub layer: LayerId,
    pub kind: &'static str,
    /// The surface or seam model the lines belong to, or the selected object in a group of its own.
    /// Solids and strings share a group for each layer, so they name nothing here.
    pub object: Option<ObjectId>,
    /// A seam horizon's material in lower case, which sets its colour; empty for anything else.
    pub material: String,
    pub segments: Vec<Segment>,
    /// What the solids' closed loops and the seams between roof and floor fill.
    pub fill: SectionFill,
}

/// A mesh ready to cut.
pub struct SectionMesh {
    mesh: Arc<TriMesh>,
    index: SliceIndex,
}

/// Meshes made for sections, each with a stamp of what it was made from.
pub type SectionMeshes = HashMap<String, (String, Arc<SectionMesh>)>;

/// The last section cut, kept so the cursor can snap to it.
pub struct SectionCut {
    pub plane: SectionPlane,
    pub groups: Vec<SectionGroup>,
}

/// A point on the cut lines, in mine grid coordinates, with what to call it.
pub struct SectionPoint {
    pub point: [f64; 3],
    pub kind: &'static str,
    pub label: String,
}

/// Where a cursor at `at` (distance along the section, RL) snaps on the last section cut. See
/// [`snap_to_section`].
pub fn snap_on_section(app: &AppInner, at: [f64; 2], vertex_reach: f64, segment_reach: f64, ignoring: Option<[f64; 2]>) -> Option<SectionPoint> {
    let cut = app.section_cut.as_ref()?;
    let groups: Vec<&[Segment]> = cut.groups.iter().map(|g| g.segments.as_slice()).collect();
    let snap = snap_to_section(&groups, &cut.plane, at, vertex_reach, segment_reach, ignoring)?;
    let name = |group: usize| {
        let group = &cut.groups[group];
        let project = app.session.project();
        let owner = group.object.and_then(|id| project.object(id)).map(|o| o.name.as_str());
        let name = owner.or(project.layer(group.layer).map(|l| l.name.as_str())).unwrap_or("a layer");
        if group.material.is_empty() { name.to_string() } else { format!("{name} ({})", group.material) }
    };
    let (kind, label) = match snap.kind {
        SectionSnapKind::Crossing => ("intersection", format!("the crossing of {} and {}", name(snap.group), name(snap.other.unwrap_or(snap.group)))),
        SectionSnapKind::Vertex => ("vertex", format!("a vertex on {}", name(snap.group))),
        SectionSnapKind::Segment => ("segment", format!("the line of {}", name(snap.group))),
    };
    Some(SectionPoint { point: snap.point, kind, label })
}

/// What a click at `at` (distance along the section, RL) picks: the surface or seam model whose line is
/// within `reach`, else the solid the point is in or within `reach` of straight up or down.
pub fn pick_on_section(app: &AppInner, at: [f64; 2], reach: f64) -> Option<ObjectId> {
    let cut = app.section_cut.as_ref()?;
    let groups: Vec<&[Segment]> = cut.groups.iter().map(|g| g.segments.as_slice()).collect();
    if let Some(object) = snap_to_section(&groups, &cut.plane, at, reach, reach, None).and_then(|snap| cut.groups[snap.group].object) {
        return Some(object);
    }
    let project = app.session.project();
    let (origin, along) = (cut.plane.origin, cut.plane.direction);
    let point = [origin[0] + along[0] * at[0], origin[1] + along[1] * at[0], at[1]];
    let height = |hit: &(ObjectId, [f64; 3])| (hit.1[2] - at[1]).abs();
    let nearest = |direction: [f64; 3]| {
        app.solid_casters
            .iter()
            .filter(|(layer, (key, _))| app.solid_keys.get(layer) == Some(key) && project.layer(**layer).is_some_and(|l| l.visible))
            .filter_map(|(_, (_, caster))| caster.cast(point, direction, project.hidden()))
            .min_by(|a, b| height(a).total_cmp(&height(b)))
    };
    match (nearest([0.0, 0.0, 1.0]), nearest([0.0, 0.0, -1.0])) {
        (Some(up), Some(down)) if up.0 == down.0 => Some(up.0),
        (up, down) => up.into_iter().chain(down).filter(|hit| height(hit) <= reach).min_by(|a, b| height(a).total_cmp(&height(b))).map(|hit| hit.0),
    }
}

/// Adds the selected object's lines as a group of their own, to draw over everything else.
pub fn add_selected(app: &AppInner, plane: &SectionPlane, width: f64, groups: &mut Vec<SectionGroup>, id: ObjectId) {
    let project = app.session.project();
    let Some(object) = project.object(id).filter(|o| !project.is_hidden(id) && project.layer(o.layer).is_some_and(|l| l.visible)) else {
        return;
    };
    let segments: Vec<Segment> = match &object.body {
        Body::Solid(solid) => segments_of(slice_mesh(solid.mesh(), plane)),
        Body::String(line) => string_near_plane(&line.vertices, line.closed, plane, width),
        _ => groups.iter().filter(|g| g.object == Some(id)).flat_map(|g| g.segments.iter().copied()).collect(),
    };
    if !segments.is_empty() {
        groups.push(SectionGroup { layer: object.layer, kind: "selected", object: Some(id), material: String::new(), segments, fill: SectionFill::default() });
    }
}

/// A mesh to cut: its layer, kind, and the surface it is, if it is one object.
type CutMesh = (LayerId, &'static str, Option<ObjectId>, Arc<SectionMesh>);
/// A seam horizon to cut: its layer, model, material, roof and floor.
type CutSeam = (LayerId, ObjectId, String, Arc<SectionMesh>, Arc<SectionMesh>);

/// Everything a section cuts, as the project stood at `generation`.
pub struct SectionInputs {
    generation: u64,
    meshes: Vec<CutMesh>,
    seams: Vec<CutSeam>,
    strings: Vec<Arc<Object>>,
}

/// The lines where `plane` cuts every visible layer, and with `fill` what solids and seams fill.
/// Strings within `width` of the plane draw flat on it.
pub fn section_groups(app: &mut AppInner, plane: &SectionPlane, width: f64, fill: bool) -> Result<Vec<SectionGroup>, String> {
    let inputs = section_inputs(app)?;
    let mut groups: Vec<SectionGroup> = Vec::new();
    for (layer, kind, object, section) in &inputs.meshes {
        let lines = slice_indexed(&section.mesh, &section.index, plane);
        let filled = if fill && *kind == "solid" { fill_section_loops(&lines, plane) } else { SectionFill::default() };
        add(&mut groups, *layer, kind, *object, "", segments_of(lines), filled);
    }
    // A model has a roof and floor for every horizon, and each pair stands alone.
    let cut: Vec<(LayerId, ObjectId, &str, Vec<Segment>, SectionFill)> = std::thread::scope(|scope| {
        let horizons = inputs.seams.iter().map(|(layer, model, material, roof, floor)| {
            scope.spawn(move || {
                let (top, base) = (slice_indexed(&roof.mesh, &roof.index, plane), slice_indexed(&floor.mesh, &floor.index, plane));
                let filled = if fill { fill_between_lines(&top, &base, plane) } else { SectionFill::default() };
                (*layer, *model, material.as_str(), segments_of(top.into_iter().chain(base)), filled)
            })
        });
        horizons.collect::<Vec<_>>().into_iter().map(|horizon| horizon.join().expect("seam worker")).collect()
    });
    for (layer, model, material, segments, filled) in cut {
        add(&mut groups, layer, "seam", Some(model), material, segments, filled);
    }
    for object in &inputs.strings {
        if let Body::String(line) = &object.body {
            add(&mut groups, object.layer, "string", None, "", string_near_plane(&line.vertices, line.closed, plane, width), SectionFill::default());
        }
    }
    Ok(groups)
}

/// What a section cuts: as kept, unless the project has changed.
fn section_inputs(app: &mut AppInner) -> Result<Arc<SectionInputs>, String> {
    let generation = app.session.project().generation();
    if let Some(inputs) = app.section_inputs.as_ref().filter(|inputs| inputs.generation == generation) {
        return Ok(inputs.clone());
    }
    let project = app.session.project();
    let visible = |layer: LayerId| project.layer(layer).is_some_and(|l| l.visible);
    let shown = |object: &Object| visible(object.layer) && !project.is_hidden(object.id);
    let mut meshes: Vec<CutMesh> = Vec::new();
    let mut seams: Vec<CutSeam> = Vec::new();
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
            meshes.push((layer, "solid", None, mesh));
        }
    }
    for object in project.objects().filter(|o| shown(o)) {
        let stamp = geometry_key(&object.body).to_string();
        match &object.body {
            Body::Surface(s) => {
                let mesh = keep(object.id.to_string(), stamp, &mut || Ok(s.mesh_arc().clone()))?;
                meshes.push((object.layer, "surface", Some(object.id), mesh));
            }
            Body::Grid(g) if is_drawn_surface(object) => {
                let mesh = keep(object.id.to_string(), stamp, &mut || Ok(Arc::new(triangulate_grid(g))))?;
                meshes.push((object.layer, "surface", Some(object.id), mesh));
            }
            Body::SeamModel(model) => {
                // Every grid reads at once, and only if one of them has to be made. A section shows the
                // seams, so it stops where a roof meets its floor, though the surfaces carry on.
                let mut read: Option<StackGrids> = None;
                for i in model.horizons.iter().enumerate().filter(|(_, h)| h.working).map(|(i, _)| i) {
                    let mut surface = |pick: fn(&StackGrids, usize) -> &zd_core::Grid| -> Result<Arc<TriMesh>, String> {
                        if read.is_none() {
                            read = Some(read_stack_grids(model).map_err(|e| e.to_string())?.seams_only());
                        }
                        Ok(Arc::new(triangulate_grid(pick(read.as_ref().expect("read above"), i))))
                    };
                    let roof = keep(format!("{} roof {i}", object.id), stamp.clone(), &mut || surface(|g, i| &g.roofs[i]))?;
                    let floor = keep(format!("{} floor {i}", object.id), stamp.clone(), &mut || surface(|g, i| &g.floors[i]))?;
                    seams.push((object.layer, object.id, model.horizons[i].material.to_ascii_lowercase(), roof, floor));
                }
            }
            _ => {}
        }
    }
    let strings = project.objects().filter(|o| o.kind() == ObjectKind::String && shown(o)).cloned().collect();
    let inputs = Arc::new(SectionInputs { generation, meshes, seams, strings });
    app.section_meshes = kept;
    app.section_inputs = Some(inputs.clone());
    Ok(inputs)
}

/// A section's lines as segments, closing the loops.
fn segments_of(lines: impl IntoIterator<Item = SliceLine>) -> Vec<Segment> {
    let mut segments = Vec::new();
    for line in lines {
        let closing = line.closed.then(|| [line.points[line.points.len() - 1], line.points[0]]);
        segments.extend(line.points.windows(2).map(|w| [w[0], w[1]]).chain(closing));
    }
    segments
}

/// Adds segments to the layer's group of that kind, object and material.
fn add(groups: &mut Vec<SectionGroup>, layer: LayerId, kind: &'static str, object: Option<ObjectId>, material: &str, segments: Vec<Segment>, fill: SectionFill) {
    if segments.is_empty() {
        return;
    }
    match groups.iter_mut().find(|g| g.layer == layer && g.kind == kind && g.object == object && g.material == material) {
        Some(group) => {
            group.segments.extend(segments);
            group.fill.triangles.extend(fill.triangles);
            group.fill.unfilled += fill.unfilled;
        }
        None => groups.push(SectionGroup { layer, kind, object, material: material.to_string(), segments, fill }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use zd_commands::Session;
    use zd_core::Project;

    use super::*;
    use crate::state::{SolidCaster, every_layer_solid, solid_snap_keys};

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A 10 m cube at `x` as `solid.from_mesh` parameters.
    fn cube(name: &str, layer: &str, x: f64) -> serde_json::Value {
        let v = (0..8).map(|i| [E + x + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]).collect::<Vec<_>>();
        let t = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        json!({ "name": name, "layer": layer, "vertices": v, "triangles": t })
    }

    /// A flat sheet at RL 200 over the section, as `surface.from_mesh` parameters.
    fn sheet() -> serde_json::Value {
        let v = [[E - 50.0, N - 50.0, 200.0], [E + 150.0, N - 50.0, 200.0], [E + 150.0, N + 50.0, 200.0], [E - 50.0, N + 50.0, 200.0]];
        json!({ "name": "topo", "layer": "surfaces", "vertices": v, "triangles": [[0, 1, 2], [0, 2, 3]] })
    }

    /// Two cubes and a sheet over them, cut along the middle of the cubes.
    fn cut_scene() -> (AppInner, ObjectId, ObjectId, ObjectId, SectionPlane) {
        let mut s = Session::with_builtins();
        let a = s.run("solid.from_mesh", cube("a", "coal", 0.0)).unwrap().outputs[0].id;
        let b = s.run("solid.from_mesh", cube("b", "coal", 20.0)).unwrap().outputs[0].id;
        let sheet = s.run("surface.from_mesh", sheet()).unwrap().outputs[0].id;
        let mut app = AppInner::new(s, None);
        let layer = app.session.project().object(a).unwrap().layer;
        let solids = every_layer_solid(app.session.project(), layer);
        let keys = solid_snap_keys(app.session.project());
        app.solid_casters.insert(layer, (keys[&layer].clone(), Arc::new(SolidCaster::new(&solids).unwrap())));
        app.solid_keys = keys;
        (app, a, b, sheet, SectionPlane::through([E, N + 5.0], [E + 100.0, N + 5.0]).unwrap())
    }

    #[test]
    fn the_selected_object_is_cut_again_as_a_group_of_its_own() {
        let (mut app, a, _, sheet, plane) = cut_scene();
        let selected = |app: &mut AppInner, id: ObjectId| {
            let mut groups = section_groups(app, &plane, 1.0, true).unwrap();
            add_selected(app, &plane, 1.0, &mut groups, id);
            groups.iter().filter(|g| g.kind == "selected").map(|g| length(&g.segments)).sum::<f64>()
        };
        assert!((selected(&mut app, a) - 40.0).abs() < 1e-9, "one 10 m cube's outline, not both");
        assert!((selected(&mut app, sheet) - 200.0).abs() < 1e-9, "the sheet from one side to the other");
        app.session.set_objects_visible(&[a], false);
        assert_eq!(selected(&mut app, a), 0.0, "nothing hidden is drawn");
    }

    /// Cube `a` spans 0 to 10 m along the section and RL 100 to 110, `b` 20 to 30 m, and the sheet is at RL 200.
    #[test]
    fn a_click_on_a_section_names_the_solid_it_is_in_or_the_line_it_is_on() {
        let (mut app, a, b, sheet, plane) = cut_scene();
        let groups = section_groups(&mut app, &plane, 1.0, true).unwrap();
        app.section_cut = Some(SectionCut { plane, groups });
        assert_eq!(pick_on_section(&app, [5.0, 105.0], 1.0), Some(a), "inside");
        assert_eq!(pick_on_section(&app, [25.0, 105.0], 1.0), Some(b), "inside the other");
        assert_eq!(pick_on_section(&app, [5.0, 110.5], 1.0), Some(a), "just over its roof");
        assert_eq!(pick_on_section(&app, [50.0, 200.4], 1.0), Some(sheet), "on a surface's line");
        assert_eq!(pick_on_section(&app, [15.0, 105.0], 1.0), None, "between the two");
        assert_eq!(pick_on_section(&app, [5.0, 150.0], 1.0), None, "well over it");
        app.session.set_objects_visible(&[a], false);
        assert_eq!(pick_on_section(&app, [5.0, 105.0], 1.0), None, "a hidden solid isn't picked");
    }

    fn length(segments: &[Segment]) -> f64 {
        segments.iter().map(|[p, q]| ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt()).sum()
    }

    fn area(fill: &SectionFill) -> f64 {
        // Plan length along the section, by RL: a vertical plane, so this is the true area.
        let flat = |t: &[[f64; 3]; 3]| {
            let (u, v) = ((t[1][0] - t[0][0]).hypot(t[1][1] - t[0][1]) * (t[1][0] - t[0][0]).signum(), (t[2][0] - t[0][0]).hypot(t[2][1] - t[0][1]) * (t[2][0] - t[0][0]).signum());
            (u * (t[2][2] - t[0][2]) - v * (t[1][2] - t[0][2])).abs() / 2.0
        };
        fill.triangles.iter().map(flat).sum()
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
        let groups = section_groups(&mut app, &plane, 1.0, true).unwrap();
        let project = app.session.project();
        let coal = group(&groups, project, "coal", "solid").expect("the coal layer's cut");
        assert!((length(&coal.segments) - 80.0).abs() < 1e-9, "two 10 m squares: {}", length(&coal.segments));
        assert!((area(&coal.fill) - 200.0).abs() < 1e-6, "and they fill: {}", area(&coal.fill));
        assert!(group(&groups, project, "waste", "solid").is_none(), "a hidden layer is not cut");
        let strings = group(&groups, project, "design", "string").expect("the near string");
        assert!((length(&strings.segments) - 30.0).abs() < 1e-9, "only the near string: {}", length(&strings.segments));
        assert!(strings.segments.iter().flatten().all(|&p| plane.offset(p).abs() < 1e-9), "drawn flat on the plane");
        assert!(strings.fill.triangles.is_empty(), "strings fill nothing");
    }

    /// A 21 x 11 grid at 10 m in Vulcan's plain layout, flat at `rl`.
    fn flat_grid(rl: f32) -> Vec<u8> {
        flagged_grid(rl, None)
    }

    /// The same, flagged off from 100 m east and holding `held` there.
    fn flagged_grid(rl: f32, held: Option<f32>) -> Vec<u8> {
        let (cols, rows) = (21usize, 11usize);
        let mut out = b"SYNTHETIC SEAM GRID             ".to_vec();
        for v in [E, E + 200.0, N, N + 100.0] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        for v in [(cols * rows) as i32, 0] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        for v in [10.0f32, 10.0] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        for v in [cols as i32, rows as i32] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        for node in 0..cols * rows {
            let (value, flag) = held.filter(|_| node % cols >= 10).map_or((rl, 1i32), |held| (held, 0));
            out.extend_from_slice(&value.to_be_bytes());
            out.extend_from_slice(&flag.to_be_bytes());
        }
        out
    }

    /// A section along 200 m of a seam model built from `rows` over a parent of 20 m and its two
    /// daughters, 10 m and 5 m with a parting between. Returns the app and the folder to remove.
    fn seam_section(rows: &str) -> (AppInner, SectionPlane, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("zd-section-{}-{}", std::process::id(), zd_core::RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        for (file, rl) in [("p.srg", 120.0), ("p.sfg", 100.0), ("a.srg", 120.0), ("a.sfg", 110.0), ("b.srg", 105.0), ("b.sfg", 100.0)] {
            std::fs::write(dir.join(file), flat_grid(rl)).unwrap();
        }
        let definition = dir.join("stack.csv");
        std::fs::write(&definition, rows).unwrap();
        let mut s = Session::with_builtins();
        s.run("seam.build_model", json!({ "definition": definition.to_string_lossy(), "layer": "seams" })).unwrap();
        let plane = SectionPlane::through([E - 10.0, N + 33.0], [E + 210.0, N + 33.0]).unwrap();
        (AppInner::new(s, None), plane, dir)
    }

    #[test]
    fn a_seam_section_groups_by_material_whatever_its_case() {
        let rows = "horizon,parent,material,working,roof,floor\na,,Waste,yes,a.srg,a.sfg\nb,,COAL,yes,b.srg,b.sfg\n";
        let (mut app, plane, dir) = seam_section(rows);
        let groups = section_groups(&mut app, &plane, 1.0, true).unwrap();
        let of = |material: &str| groups.iter().find(|g| g.material == material).unwrap_or_else(|| panic!("no {material} group"));
        assert_eq!(groups.len(), 2);
        assert!((area(&of("waste").fill) - 2_000.0).abs() < 1e-6, "200 by 10: {}", area(&of("waste").fill));
        assert!((area(&of("coal").fill) - 1_000.0).abs() < 1e-6, "200 by 5: {}", area(&of("coal").fill));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_seam_model_fills_each_working_horizon_between_its_own_roof_and_floor() {
        // If the parent filled, or a roof paired with the wrong floor, the area would be wrong.
        let rows = "horizon,parent,material,working,roof,floor\np,,coal,no,p.srg,p.sfg\na,p,coal,yes,a.srg,a.sfg\nb,p,coal,yes,b.srg,b.sfg\n";
        let (mut app, plane, dir) = seam_section(rows);
        let groups = section_groups(&mut app, &plane, 1.0, true).unwrap();
        let project = app.session.project();
        let seam = group(&groups, project, "seams", "seam").expect("the model's cut");
        assert!((length(&seam.segments) - 800.0).abs() < 1e-6, "four lines of 200 m, none of the parent's: {}", length(&seam.segments));
        assert!((area(&seam.fill) - 3_000.0).abs() < 1e-6, "200 by 10 and 200 by 5: {}", area(&seam.fill));
        let lines_only = section_groups(&mut app, &plane, 1.0, false).unwrap();
        assert!(lines_only[0].fill.triangles.is_empty() && !lines_only[0].segments.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A roof flagged off from 100 m east, sitting on its floor there as a seam with no thickness does.
    #[test]
    fn a_seam_section_stops_where_the_roof_meets_the_floor() {
        let dir = std::env::temp_dir().join(format!("zd-section-{}-{}", std::process::id(), zd_core::RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("c.srg"), flagged_grid(110.0, Some(100.0))).unwrap();
        std::fs::write(dir.join("c.sfg"), flat_grid(100.0)).unwrap();
        let definition = dir.join("stack.csv");
        std::fs::write(&definition, "horizon,parent,material,working,roof,floor\nc,,coal,yes,c.srg,c.sfg\n").unwrap();
        let mut s = Session::with_builtins();
        s.run("seam.build_model", json!({ "definition": definition.to_string_lossy(), "layer": "seams" })).unwrap();
        let mut app = AppInner::new(s, None);
        let plane = SectionPlane::through([E - 10.0, N + 33.0], [E + 210.0, N + 33.0]).unwrap();
        let groups = section_groups(&mut app, &plane, 1.0, true).unwrap();
        let seam = group(&groups, app.session.project(), "seams", "seam").expect("the model's cut");
        // Nodes to 90 m east are coal, so roof and floor each run 90 m, and they fill 90 by 10.
        assert!((length(&seam.segments) - 180.0).abs() < 1e-6, "{}", length(&seam.segments));
        assert!((area(&seam.fill) - 900.0).abs() < 1e-6, "{}", area(&seam.fill));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn what_a_section_cuts_keeps_up_with_the_project() {
        let mut s = Session::with_builtins();
        s.run("solid.from_mesh", cube("a", "coal", 0.0)).unwrap();
        let layer = s.project().layers().iter().find(|l| l.name == "coal").unwrap().id;
        let mut app = AppInner::new(s, None);
        let plane = SectionPlane::through([E - 10.0, N + 5.0], [E + 60.0, N + 5.0]).unwrap();
        let cut = |app: &mut AppInner| section_groups(app, &plane, 1.0, true).unwrap().iter().map(|g| length(&g.segments)).sum::<f64>();
        assert!((cut(&mut app) - 40.0).abs() < 1e-9);
        let kept = app.section_inputs.clone().unwrap();
        cut(&mut app);
        assert!(Arc::ptr_eq(&kept, app.section_inputs.as_ref().unwrap()), "a step finds nothing again");

        app.session.set_layer_visible(layer, false);
        assert_eq!(cut(&mut app), 0.0, "a layer hidden");
        app.session.set_layer_visible(layer, true);
        assert!((cut(&mut app) - 40.0).abs() < 1e-9, "and shown");
        let added = app.session.run("solid.from_mesh", cube("b", "coal", 20.0)).unwrap().outputs[0].id;
        assert!((cut(&mut app) - 80.0).abs() < 1e-9, "a solid added");
        app.session.set_objects_visible(&[added], false);
        assert!((cut(&mut app) - 40.0).abs() < 1e-9, "one solid hidden, the other still cut");
        app.session.set_objects_visible(&[added], true);
        assert!((cut(&mut app) - 80.0).abs() < 1e-9, "and shown");
        app.session.undo().unwrap();
        assert!((cut(&mut app) - 40.0).abs() < 1e-9, "and undone");
    }

    #[test]
    fn a_cursor_snaps_to_the_last_cut_in_mine_grid_coordinates() {
        let mut s = Session::with_builtins();
        s.run("solid.from_mesh", cube("a", "coal", 0.0)).unwrap();
        let mut app = AppInner::new(s, None);
        let plane = SectionPlane::through([E - 10.0, N + 5.0], [E + 60.0, N + 5.0]).unwrap();
        assert!(snap_on_section(&app, [15.0, 110.0], 1.0, 1.0, None).is_none(), "nothing cut yet");
        let groups = section_groups(&mut app, &plane, 1.0, true).unwrap();
        app.section_cut = Some(SectionCut { plane, groups });

        // The cube's top corner: 10 m along from a start 10 m west of it, at RL 110.
        let corner = snap_on_section(&app, [10.3, 109.8], 1.0, 0.5, None).unwrap();
        assert_eq!((corner.kind, corner.label.as_str()), ("vertex", "a vertex on coal"));
        assert_eq!(corner.point, [E, N + 5.0, 110.0], "exact");
        // Clear of the cube's corner, and of the vertex where the top face's diagonal crosses the plane.
        let line = snap_on_section(&app, [12.5, 109.8], 1.0, 0.5, None).unwrap();
        assert_eq!((line.kind, line.label.as_str()), ("segment", "the line of coal"));
        assert!((line.point[0] - (E + 2.5)).abs() < 1e-9 && line.point[1] == N + 5.0 && line.point[2] == 110.0, "{:?}", line.point);
        assert!(snap_on_section(&app, [12.5, 130.0], 1.0, 0.5, None).is_none(), "nothing there");
    }

    #[test]
    fn fills_are_asked_for() {
        let mut s = Session::with_builtins();
        s.run("solid.from_mesh", cube("a", "coal", 0.0)).unwrap();
        let mut app = AppInner::new(s, None);
        let plane = SectionPlane::through([E - 10.0, N + 5.0], [E + 60.0, N + 5.0]).unwrap();
        let groups = section_groups(&mut app, &plane, 1.0, false).unwrap();
        assert!(groups[0].fill.triangles.is_empty());
        assert!(!groups[0].segments.is_empty(), "the lines still cut");
    }

    #[test]
    fn stepping_reuses_what_it_made_until_the_geometry_changes() {
        let mut s = Session::with_builtins();
        s.run("solid.from_mesh", cube("a", "coal", 0.0)).unwrap();
        let mut app = AppInner::new(s, None);
        let plane = SectionPlane::through([E - 10.0, N + 5.0], [E + 60.0, N + 5.0]).unwrap();
        section_groups(&mut app, &plane, 1.0, true).unwrap();
        let first = app.section_meshes.values().next().unwrap().1.clone();
        section_groups(&mut app, &plane.stepped(1.0), 1.0, true).unwrap();
        assert!(Arc::ptr_eq(&first, &app.section_meshes.values().next().unwrap().1), "kept between steps");

        app.session.run("solid.from_mesh", cube("b", "coal", 20.0)).unwrap();
        let groups = section_groups(&mut app, &plane, 1.0, true).unwrap();
        assert!(!Arc::ptr_eq(&first, &app.section_meshes.values().next().unwrap().1), "made again when a solid is added");
        assert!((length(&groups[0].segments) - 80.0).abs() < 1e-9);
    }
}
