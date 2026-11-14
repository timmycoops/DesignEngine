//! `solid.cut_by_seams` through the registry, as the app calls it (decision 0062, workstream 3): solids cut by a seam
//! model's working section in one run, each piece carrying its seam, material and horizon. A made-up model of two seams
//! on a 25 m grid, read from Vulcan grid files where the model links them. Volumes to 0.001 m³.

use std::path::{Path, PathBuf};

use serde_json::json;
use zd_commands::builtin::{CutBySeamsParams, check_cut_by_seams};
use zd_commands::{Code, Note, RunRecord, Session, Severity};
use zd_core::{AttrValue, Body, ChangeKind, ObjectId, RunId};

const E: f64 = 500_000.0;
const N: f64 = 7_000_000.0;
/// The grids' 25 m lattice: 13 by 9 nodes, 300 by 200 m.
const STEP: f64 = 25.0;
const COLS: usize = 13;
const ROWS: usize = 9;

/// A fresh, empty folder under the temp directory; removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("zd-cut-by-seams-{tag}-{}-{}", std::process::id(), RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A grid in Vulcan's plain layout on the 25 m lattice; `f` takes x in metres from the lattice's origin.
fn vulcan_grid(f: impl Fn(f64) -> f32) -> Vec<u8> {
    let mut out = b"SYNTHETIC SEAM GRID             ".to_vec();
    for v in [E, E + (COLS - 1) as f64 * STEP, N, N + (ROWS - 1) as f64 * STEP] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [(COLS * ROWS) as i32, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [STEP as f32, STEP as f32] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [COLS as i32, ROWS as i32] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for _ in 0..ROWS {
        for col in 0..COLS {
            out.extend_from_slice(&f(col as f64 * STEP).to_be_bytes());
            out.extend_from_slice(&1i32.to_be_bytes());
        }
    }
    out
}

/// Two seams with a mine code `grn` in front: `cu` from RL 96 up to `cu_roof`, and below it `cl` from 90 to 94.
fn write_model(dir: &Path, cu_roof: impl Fn(f64) -> f32) {
    let files: [(&str, Vec<u8>); 6] = [
        ("grncu.srg", vulcan_grid(cu_roof)),
        ("grncu.sfg", vulcan_grid(|_| 96.0)),
        ("grncu.rdg", vulcan_grid(|_| 1.4)),
        ("grncl.srg", vulcan_grid(|_| 94.0)),
        ("grncl.sfg", vulcan_grid(|_| 90.0)),
        ("grncl.rdg", vulcan_grid(|_| 1.5)),
    ];
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    std::fs::write(
        dir.join("stack.csv"),
        "horizon,parent,material,working,roof,floor,rd\ncu,,coal,yes,grncu.srg,grncu.sfg,grncu.rdg\ncl,,coal,yes,grncl.srg,grncl.sfg,grncl.rdg\n",
    )
    .unwrap();
}

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

/// A session with the model built from `dir`.
fn session(dir: &Path) -> (Session, ObjectId) {
    let mut s = Session::with_builtins();
    let model = created(&s.run("seam.build_model", json!({ "definition": dir.join("stack.csv").to_string_lossy(), "layer": "geology" })).unwrap())[0];
    (s, model)
}

/// A box `x` east and `y` north of the lattice's origin, from RL `z0` to `z1`, wound outward.
fn box_solid(s: &mut Session, name: &str, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> ObjectId {
    let vertices: Vec<[f64; 3]> = (0..8)
        .map(|i| [E + if i & 1 == 1 { x.1 } else { x.0 }, N + if i >> 1 & 1 == 1 { y.1 } else { y.0 }, if i >> 2 & 1 == 1 { z.1 } else { z.0 }])
        .collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    created(&s.run("solid.from_mesh", json!({ "name": name, "layer": "strips", "vertices": vertices, "triangles": triangles })).unwrap())[0]
}

fn volume(s: &Session, id: ObjectId) -> f64 {
    let Body::Solid(solid) = &s.project().object(id).unwrap().body else { panic!("a solid") };
    solid.mesh().signed_volume()
}

fn text(s: &Session, id: ObjectId, key: &str) -> Option<String> {
    match s.project().object(id).unwrap().attributes.get(key) {
        Some(AttrValue::Text(t)) => Some(t.clone()),
        Some(AttrValue::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn layer_of(s: &Session, id: ObjectId) -> String {
    s.project().layer(s.project().object(id).unwrap().layer).unwrap().name.clone()
}

/// A strip 100 × 60 m from RL 80 to 110 across both seams: overburden 10 m, cu 4, cu-cl 2, cl 4 and 10 m below, each
/// times 6,000 m², named for the strip and the layer, each carrying its seam, material and horizon, in one undo. Waste
/// goes with the seam below it; what's below the last floor with none.
#[test]
fn a_strip_cut_by_two_seams_into_five_named_layers() {
    let t = TempDir::new("five");
    write_model(&t.0, |_| 100.0);
    let (mut s, model) = session(&t.0);
    let strip = box_solid(&mut s, "Strip 5", (100.0, 200.0), (50.0, 110.0), (80.0, 110.0));
    let before = s.project().objects().count();
    let record = s.run("solid.cut_by_seams", json!({ "solids": [strip], "model": model, "layer": "layers" })).unwrap();
    println!("{}", record.messages.join("\n"));
    let made = created(&record);
    let want = [
        ("Strip 5 OB", "OB", "waste", Some("cu"), 60_000.0),
        ("Strip 5 cu", "cu", "coal", Some("cu"), 24_000.0),
        ("Strip 5 cu-cl", "cu-cl", "waste", Some("cl"), 12_000.0),
        ("Strip 5 cl", "cl", "coal", Some("cl"), 24_000.0),
        ("Strip 5 below cl", "below cl", "waste", None, 60_000.0),
    ];
    assert_eq!(made.len(), want.len());
    for (&id, (name, horizon, material, seam, v)) in made.iter().zip(want) {
        assert_eq!(s.project().object(id).unwrap().name, name);
        assert_eq!(layer_of(&s, id), "layers");
        assert_eq!(text(&s, id, "horizon").as_deref(), Some(horizon));
        assert_eq!(text(&s, id, "material").as_deref(), Some(material));
        assert_eq!(text(&s, id, "seam").as_deref(), seam, "{name}");
        assert!((volume(&s, id) - v).abs() < 0.001, "{name}: {}", volume(&s, id));
    }
    assert!(record.messages.iter().any(|m| m.contains("add back to every solid exactly")), "{:?}", record.messages);
    assert!(record.notes.iter().all(|n| n.code != Some(Code::SolidOffGrids) && n.code != Some(Code::SeamThinLeftOut)), "{:?}", record.notes);
    s.undo().unwrap();
    assert_eq!(s.project().objects().count(), before);
}

/// cu thins from 4 m at x = 125 to nothing at x = 150 and lies on its floor east of it. The strip from 100 to 200 takes
/// coal to where it's 0.01 m thick, 24.9375 m into that cell: 60 (25 * 4 + 4 * 24.9375 - 0.08 * 24.9375²) =
/// 8,999.98125 m³. The last 0.0625 m, under 0.01 m thick, is left out and said: 0.5 * 0.01 * 0.0625 * 60 = 0.01875 m³.
/// Nothing is refused.
#[test]
fn a_seam_running_out_leaves_coal_only_where_it_is_and_says_what_was_too_thin() {
    let t = TempDir::new("thin");
    write_model(&t.0, |x| if x <= 125.0 { 100.0 } else { 96.0 });
    let (mut s, model) = session(&t.0);
    let strip = box_solid(&mut s, "Strip 5", (100.0, 200.0), (50.0, 110.0), (80.0, 110.0));
    let record = s.run("solid.cut_by_seams", json!({ "solids": [strip], "model": model, "layer": "layers" })).unwrap();
    let coal: Vec<ObjectId> = created(&record).into_iter().filter(|&id| text(&s, id, "horizon").as_deref() == Some("cu")).collect();
    assert_eq!(coal.len(), 1);
    assert!((volume(&s, coal[0]) - 8_999.98125).abs() < 0.001, "{}", volume(&s, coal[0]));
    let thin = record.notes.iter().find(|n| n.code == Some(Code::SeamThinLeftOut)).expect("what's too thin is said");
    assert!((thin.value.as_ref().unwrap().value - 0.01875).abs() < 0.001, "{}", thin.text);
    let total: f64 = created(&record).iter().map(|&id| volume(&s, id)).sum();
    assert!((total + 0.01875 - 180_000.0).abs() < 0.001);
}

/// What the Cut solids panel reads before a cut by seams.
fn check(s: &Session, solids: &[ObjectId], model: ObjectId) -> Vec<Note> {
    let params: CutBySeamsParams = serde_json::from_value(json!({ "solids": solids, "model": model, "layer": "layers" })).unwrap();
    check_cut_by_seams(s.project(), &params).unwrap()
}

/// A strip reaching 50 m west of the grids (decision 0063): found by the check, 50 × 60 m past their edge about its
/// middle, and refused by the cut with nothing made. Kept to the grids, it's checked and cut.
#[test]
fn a_strip_past_the_grids_edge_is_found_first_and_refused() {
    let t = TempDir::new("off");
    write_model(&t.0, |_| 100.0);
    let (mut s, model) = session(&t.0);
    let strip = box_solid(&mut s, "Strip 5", (-50.0, 100.0), (50.0, 110.0), (80.0, 110.0));
    let notes = check(&s, &[strip], model);
    let off: Vec<&Note> = notes.iter().filter(|n| n.severity == Severity::Warning).collect();
    assert_eq!(off.len(), 1, "{notes:?}");
    assert_eq!(off[0].code, Some(Code::SolidOffGrids));
    assert_eq!(off[0].text, "'Strip 5' reaches past the edge of the grids by 3000.0 m² about 499975.000 7000080.000");
    assert_eq!(off[0].object, Some(strip));

    let before = s.project().objects().count();
    let refused = s.run("solid.cut_by_seams", json!({ "solids": [strip], "model": model, "layer": "layers" })).unwrap_err().to_string();
    let want = "the grids of 'stack' don't reach over a solid with no hole under it, so nothing is cut: 'Strip 5' reaches past their edge by 3000.0 m² about 499975.000 7000080.000. Fix the geology model and import the grids again, or keep the solids to where the grids are, then cut";
    assert!(refused.contains(want), "{refused}");
    assert_eq!(s.project().objects().count(), before, "nothing made");

    let kept = box_solid(&mut s, "Strip 6", (0.0, 100.0), (50.0, 110.0), (80.0, 110.0));
    let notes = check(&s, &[kept], model);
    assert!(notes.iter().all(|n| n.severity != Severity::Warning), "{notes:?}");
    assert!(notes.iter().any(|n| n.text == "the grids reach over all 1 solids with no hole under them"), "{notes:?}");
    let record = s.run("solid.cut_by_seams", json!({ "solids": [kept], "model": model, "layer": "layers" })).unwrap();
    let total: f64 = created(&record).iter().map(|&id| volume(&s, id)).sum();
    assert!((total - 180_000.0).abs() < 0.001, "{total}");
    assert!(created(&record).iter().all(|&id| layer_of(&s, id) == "layers"), "no layer off the grids");
}

/// A hole in cl's roof and floor all along x = 250 m, nine nodes holding no height (decision 0063): said on the model,
/// never filled; a strip over it found by the check, the two 25 m cells either side under it (50 × 60 m), and refused;
/// a strip clear of it cut as before.
#[test]
fn a_hole_in_the_grids_under_a_strip_is_found_first_and_refused() {
    let t = TempDir::new("hole");
    write_model(&t.0, |_| 100.0);
    let holed = |z: f32| vulcan_grid(move |x| if x == 250.0 { f32::NAN } else { z });
    std::fs::write(t.0.join("grncl.srg"), holed(94.0)).unwrap();
    std::fs::write(t.0.join("grncl.sfg"), holed(90.0)).unwrap();
    let mut s = Session::with_builtins();
    let built = s.run("seam.build_model", json!({ "definition": t.0.join("stack.csv").to_string_lossy(), "layer": "geology" })).unwrap();
    let model = created(&built)[0];
    let holes = built.notes.iter().find(|n| n.code == Some(Code::SeamHoles)).expect("the holes said on the model");
    assert!(holes.text.starts_with("warning: cl roof or floor holds no height at 9 nodes, the largest hole at E 500250.0 N 7000100.0"), "{}", holes.text);

    let over = box_solid(&mut s, "Strip 7", (200.0, 300.0), (50.0, 110.0), (80.0, 110.0));
    let notes = check(&s, &[over], model);
    let found: Vec<&Note> = notes.iter().filter(|n| n.severity == Severity::Warning).collect();
    assert_eq!(found.len(), 1, "{notes:?}");
    assert_eq!(found[0].code, Some(Code::SolidOverGridHoles));
    assert_eq!(found[0].text, "'Strip 7' lies over a hole in the grids by 3000.0 m² about 500250.000 7000080.000");
    let refused = s.run("solid.cut_by_seams", json!({ "solids": [over], "model": model, "layer": "layers" })).unwrap_err().to_string();
    assert!(refused.contains("'Strip 7' reaches over holes in them by 3000.0 m² about 500250.000 7000080.000"), "{refused}");

    let clear = box_solid(&mut s, "Strip 5", (100.0, 200.0), (50.0, 110.0), (80.0, 110.0));
    assert!(check(&s, &[clear], model).iter().all(|n| n.severity != Severity::Warning));
    let record = s.run("solid.cut_by_seams", json!({ "solids": [clear], "model": model, "layer": "layers" })).unwrap();
    let total: f64 = created(&record).iter().map(|&id| volume(&s, id)).sum();
    assert!((total - 180_000.0).abs() < 0.001, "{total}");
    // Both strips at once: refused for the one over the hole, nothing made.
    let before = s.project().objects().count();
    assert!(s.run("solid.cut_by_seams", json!({ "solids": [clear, over], "model": model, "layer": "layers" })).is_err());
    assert_eq!(s.project().objects().count(), before);
}

/// Seams then blocks, Tim's order: the pieces cut by block polygons are named block and horizon (no strip number here),
/// unique though the overburden carries the same seam as the coal under it, and the coal is interrogated by its `seam`
/// with no seam attribute to name.
#[test]
fn seams_then_blocks_give_unique_names_and_interrogate_by_seam() {
    let t = TempDir::new("blocks");
    write_model(&t.0, |_| 100.0);
    let (mut s, model) = session(&t.0);
    let strip = box_solid(&mut s, "Strip 5", (0.0, 300.0), (50.0, 110.0), (80.0, 110.0));
    let layers = created(&s.run("solid.cut_by_seams", json!({ "solids": [strip], "model": model, "layer": "layers" })).unwrap());
    let polygons: Vec<ObjectId> = (0..3)
        .map(|k| {
            let (x0, x1) = (100.0 * k as f64, 100.0 * (k + 1) as f64);
            let points = [[E + x0, N, 0.0], [E + x1, N, 0.0], [E + x1, N + 200.0, 0.0], [E + x0, N + 200.0, 0.0]];
            created(&s.run("string.create", json!({ "name": format!("B{}", k + 1), "layer": "block polygons", "closed": true, "points": points })).unwrap())[0]
        })
        .collect();
    let record = s.run("solid.cut_by_polygons", json!({ "solids": layers, "polygons": polygons, "layer": "blocks" })).unwrap();
    let blocks = created(&record);
    assert_eq!(blocks.len(), 15);
    assert!(record.notes.iter().all(|n| n.code != Some(Code::BlockNameRepeated)), "{:?}", record.notes);
    let names: Vec<String> = blocks.iter().map(|&id| s.project().object(id).unwrap().name.clone()).collect();
    for name in ["B2_OB", "B2_cu", "B2_cu-cl", "B2_cl", "B2_below cl"] {
        assert!(names.contains(&name.to_string()), "{name} in {names:?}");
    }
    let coal: Vec<ObjectId> = blocks.iter().copied().filter(|&id| text(&s, id, "material").as_deref() == Some("coal")).collect();
    assert_eq!(coal.len(), 6);
    let interrogated = s.run("solid.interrogate", json!({ "solids": coal, "model": model })).unwrap();
    assert!(interrogated.notes.iter().all(|n| n.code != Some(Code::InterrogateNoSeam) && n.code != Some(Code::InterrogateUnknownSeam)), "{:?}", interrogated.notes);
    for &id in &coal {
        let rd = match s.project().object(id).unwrap().attributes.get("rd") {
            Some(AttrValue::Number(n)) => *n,
            other => panic!("no rd: {other:?}"),
        };
        let want = if text(&s, id, "seam").as_deref() == Some("cu") { 1.4f32 } else { 1.5f32 };
        assert!((rd - f64::from(want)).abs() < 1e-9);
    }
}

#[test]
fn a_solid_off_the_grids_or_no_thickness_is_refused() {
    let t = TempDir::new("refused");
    write_model(&t.0, |_| 100.0);
    let (mut s, model) = session(&t.0);
    let far = box_solid(&mut s, "Far", (1000.0, 1100.0), (50.0, 110.0), (80.0, 110.0));
    let refused = s.run("solid.cut_by_seams", json!({ "solids": [far], "model": model, "layer": "layers" })).unwrap_err();
    assert!(refused.to_string().contains("'Far' reaches past their edge by 6000.0 m²"), "{refused}");
    let strip = box_solid(&mut s, "Strip", (0.0, 100.0), (50.0, 110.0), (80.0, 110.0));
    let refused = s.run("solid.cut_by_seams", json!({ "solids": [strip], "model": model, "layer": "layers", "min_thickness": 0 })).unwrap_err();
    assert!(refused.to_string().contains("minimum thickness"), "{refused}");
}

/// A strip whose top lies 4 mm over cu's roof (RL 100): the overburden in it is a flap 4 mm × 100 × 60 m = 24 m³,
/// taken off and said by default (decision 0064); with no flap thickness it's a piece of its own.
#[test]
fn a_film_over_the_roof_is_taken_off_as_a_flap_and_said() {
    let t = TempDir::new("flap");
    write_model(&t.0, |_| 100.0);
    let (mut s, model) = session(&t.0);
    let strip = box_solid(&mut s, "Strip 5", (0.0, 100.0), (50.0, 110.0), (80.0, 100.004));
    let record = s.run("solid.cut_by_seams", json!({ "solids": [strip], "model": model, "layer": "layers" })).unwrap();
    let flaps = record.notes.iter().find(|n| n.code == Some(Code::SeamFlapsLeftOut)).expect("the flap said");
    assert_eq!(flaps.text, "24.000 m³ in flaps thinner than 0.01 m taken off 1 pieces, left out");
    assert!((flaps.value.as_ref().map(|m| m.value).unwrap() - 24.0).abs() < 1e-6);
    let total: f64 = created(&record).iter().map(|&id| volume(&s, id)).sum();
    assert!((total - 100.0 * 60.0 * 20.0).abs() < 1e-6, "the strip less its flap: {total}");

    let record = s.run("solid.cut_by_seams", json!({ "solids": [strip], "model": model, "layer": "kept", "flap_thickness": 0.0 })).unwrap();
    assert!(record.notes.iter().all(|n| n.code != Some(Code::SeamFlapsLeftOut)));
    let total: f64 = created(&record).iter().map(|&id| volume(&s, id)).sum();
    assert!((total - 100.0 * 60.0 * 20.004).abs() < 1e-6, "{total}");
}
