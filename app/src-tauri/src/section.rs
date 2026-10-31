//! The section view's lines: where a plane cuts everything on the visible layers, by layer and kind.
//! A section is view state (decision 4 of the Phase 2 plan): nothing here is a command or changes
//! the project. Solids cut as one mesh per layer, as they draw, and seam models as each working
//! horizon's roof and floor, which is what lets them fill. What takes work to make is kept until its
//! geometry changes, and the list of what to cut until the project changes, so stepping along a
//! section costs only the cut: with tens of thousands of solids, finding them all again took twice
//! as long as cutting them.

use std::collections::HashMap;
use std::sync::Arc;

use zd_core::{Body, LayerId, Object, ObjectKind, TriMesh};
use zd_geom::{SectionFill, SectionPlane, SliceIndex, SliceLine, fill_between_lines, fill_section_loops, slice_indexed, string_near_plane, triangulate_grid};
use zd_io::{StackGrids, read_stack_grids};

use crate::state::{AppInner, geometry_key, is_drawn_surface, joined_meshes, layer_solids, solid_layer_keys};

/// Two ends of a segment, in mine grid coordinates.
pub type Segment = [[f64; 3]; 2];

/// One layer's lines of one kind.
pub struct SectionGroup {
    pub layer: LayerId,
    pub kind: &'static str,
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

/// Everything a section cuts, as the project stood at `generation`.
pub struct SectionInputs {
    generation: u64,
    meshes: Vec<(LayerId, &'static str, Arc<SectionMesh>)>,
    seams: Vec<(LayerId, Arc<SectionMesh>, Arc<SectionMesh>)>,
    strings: Vec<Arc<Object>>,
}

/// The lines where `plane` cuts every visible layer, and with `fill` what solids and seams fill.
/// Strings within `width` of the plane draw flat on it.
pub fn section_groups(app: &mut AppInner, plane: &SectionPlane, width: f64, fill: bool) -> Result<Vec<SectionGroup>, String> {
    let inputs = section_inputs(app)?;
    let mut groups: Vec<SectionGroup> = Vec::new();
    for (layer, kind, section) in &inputs.meshes {
        let lines = slice_indexed(&section.mesh, &section.index, plane);
        let filled = if fill && *kind == "solid" { fill_section_loops(&lines, plane) } else { SectionFill::default() };
        add(&mut groups, *layer, *kind, segments_of(lines), filled);
    }
    // A model has a roof and floor for every horizon, and each pair stands alone.
    let cut: Vec<(LayerId, Vec<Segment>, SectionFill)> = std::thread::scope(|scope| {
        let horizons = inputs.seams.iter().map(|(layer, roof, floor)| {
            scope.spawn(move || {
                let (top, base) = (slice_indexed(&roof.mesh, &roof.index, plane), slice_indexed(&floor.mesh, &floor.index, plane));
                let filled = if fill { fill_between_lines(&top, &base, plane) } else { SectionFill::default() };
                (*layer, segments_of(top.into_iter().chain(base)), filled)
            })
        });
        horizons.collect::<Vec<_>>().into_iter().map(|horizon| horizon.join().expect("seam worker")).collect()
    });
    for (layer, segments, filled) in cut {
        add(&mut groups, layer, "seam", segments, filled);
    }
    for object in &inputs.strings {
        if let Body::String(line) = &object.body {
            add(&mut groups, object.layer, "string", string_near_plane(&line.vertices, line.closed, plane, width), SectionFill::default());
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
    let mut meshes: Vec<(LayerId, &'static str, Arc<SectionMesh>)> = Vec::new();
    let mut seams: Vec<(LayerId, Arc<SectionMesh>, Arc<SectionMesh>)> = Vec::new();
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
                // Every grid reads at once, and only if one of them has to be made.
                let mut read: Option<StackGrids> = None;
                for i in model.horizons.iter().enumerate().filter(|(_, h)| h.working).map(|(i, _)| i) {
                    let mut surface = |pick: fn(&StackGrids, usize) -> &zd_core::Grid| -> Result<Arc<TriMesh>, String> {
                        if read.is_none() {
                            read = Some(read_stack_grids(model).map_err(|e| e.to_string())?);
                        }
                        Ok(Arc::new(triangulate_grid(pick(read.as_ref().expect("read above"), i))))
                    };
                    let roof = keep(format!("{} roof {i}", object.id), stamp.clone(), &mut || surface(|g, i| &g.roofs[i]))?;
                    let floor = keep(format!("{} floor {i}", object.id), stamp.clone(), &mut || surface(|g, i| &g.floors[i]))?;
                    seams.push((object.layer, roof, floor));
                }
            }
            _ => {}
        }
    }
    let strings = project.objects().filter(|o| o.kind() == ObjectKind::String && visible(o.layer)).cloned().collect();
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

/// Adds segments to the layer's group of that kind.
fn add(groups: &mut Vec<SectionGroup>, layer: LayerId, kind: &'static str, segments: Vec<Segment>, fill: SectionFill) {
    if segments.is_empty() {
        return;
    }
    match groups.iter_mut().find(|g| g.layer == layer && g.kind == kind) {
        Some(group) => {
            group.segments.extend(segments);
            group.fill.triangles.extend(fill.triangles);
            group.fill.unfilled += fill.unfilled;
        }
        None => groups.push(SectionGroup { layer, kind, segments, fill }),
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
        for _ in 0..cols * rows {
            out.extend_from_slice(&rl.to_be_bytes());
            out.extend_from_slice(&1i32.to_be_bytes());
        }
        out
    }

    #[test]
    fn a_seam_model_fills_each_working_horizon_between_its_own_roof_and_floor() {
        // A parent of 20 m over its two daughters, 10 m and 5 m with a parting between. If the parent
        // filled, or a roof paired with the wrong floor, the area would be wrong.
        let dir = std::env::temp_dir().join(format!("zd-section-{}-{}", std::process::id(), zd_core::RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        for (file, rl) in [("p.srg", 120.0), ("p.sfg", 100.0), ("a.srg", 120.0), ("a.sfg", 110.0), ("b.srg", 105.0), ("b.sfg", 100.0)] {
            std::fs::write(dir.join(file), flat_grid(rl)).unwrap();
        }
        let definition = dir.join("stack.csv");
        let rows = "horizon,parent,material,working,roof,floor\np,,coal,no,p.srg,p.sfg\na,p,coal,yes,a.srg,a.sfg\nb,p,coal,yes,b.srg,b.sfg\n";
        std::fs::write(&definition, rows).unwrap();
        let mut s = Session::with_builtins();
        s.run("seam.build_model", json!({ "definition": definition.to_string_lossy(), "layer": "seams" })).unwrap();
        let mut app = AppInner::new(s, None);
        let plane = SectionPlane::through([E - 10.0, N + 33.0], [E + 210.0, N + 33.0]).unwrap();
        let groups = section_groups(&mut app, &plane, 1.0, true).unwrap();
        let project = app.session.project();
        let seam = group(&groups, project, "seams", "seam").expect("the model's cut");
        assert!((length(&seam.segments) - 800.0).abs() < 1e-6, "four lines of 200 m, none of the parent's: {}", length(&seam.segments));
        assert!((area(&seam.fill) - 3_000.0).abs() < 1e-6, "200 by 10 and 200 by 5: {}", area(&seam.fill));
        let lines_only = section_groups(&mut app, &plane, 1.0, false).unwrap();
        assert!(lines_only[0].fill.triangles.is_empty() && !lines_only[0].segments.is_empty());
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
        app.session.run("solid.from_mesh", cube("b", "coal", 20.0)).unwrap();
        assert!((cut(&mut app) - 80.0).abs() < 1e-9, "a solid added");
        app.session.undo().unwrap();
        assert!((cut(&mut app) - 40.0).abs() < 1e-9, "and undone");
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
