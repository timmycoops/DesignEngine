//! Block polygons through the registry, as the app calls them (decision 0062): lines across the strips every 100 m and
//! two lines closing them make a polygon of each area, named west to east; ends that stop short are joined or listed.

use serde_json::json;
use zd_commands::builtin::{CutByPolygonsParams, check_cut_by_polygons};
use zd_commands::{Code, RunRecord, Session, Severity};
use zd_core::{AttrValue, Body, ChangeKind, ObjectId};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

fn created(record: &RunRecord) -> Vec<ObjectId> {
    record.outputs.iter().filter(|o| o.change == ChangeKind::Created).map(|o| o.id).collect()
}

fn line(s: &mut Session, name: &str, points: &[(f64, f64)]) -> ObjectId {
    let points: Vec<[f64; 3]> = points.iter().map(|&(x, y)| [E + x, N + y, 0.0]).collect();
    created(&s.run("string.create", json!({ "name": name, "layer": "block lines", "points": points })).unwrap())[0]
}

/// Plan area of a closed string, by the shoelace.
fn area(s: &Session, id: ObjectId) -> f64 {
    let Body::String(polyline) = &s.project().object(id).unwrap().body else { panic!("a string") };
    assert!(polyline.closed);
    let v = &polyline.vertices;
    let n = v.len();
    (0..n).map(|k| (v[k][0] - E) * (v[(k + 1) % n][1] - N) - (v[(k + 1) % n][0] - E) * (v[k][1] - N)).sum::<f64>() / 2.0
}

/// A south and a north line 360 m apart, and five lines across them 100 m apart running 50 m past: four blocks of
/// 100 × 360 m, named west to east, in one undo.
#[test]
fn lines_across_and_two_closing_lines_make_named_blocks() {
    let mut s = Session::with_builtins();
    let mut lines = vec![line(&mut s, "south", &[(-20.0, 0.0), (420.0, 0.0)]), line(&mut s, "north", &[(-20.0, 360.0), (420.0, 360.0)])];
    for k in 0..5 {
        let x = 100.0 * k as f64;
        lines.push(line(&mut s, &format!("across {k}"), &[(x, -50.0), (x, 410.0)]));
    }
    let before = s.project().objects().count();
    let record = s.run("string.polygons_from_lines", json!({ "strings": lines, "layer": "blocks", "name": "Block #" })).unwrap();
    let made = created(&record);
    assert_eq!(made.len(), 4);
    for (k, &id) in made.iter().enumerate() {
        let object = s.project().object(id).unwrap();
        assert_eq!(object.name, format!("Block {}", k + 1));
        assert!((area(&s, id) - 36_000.0).abs() < 0.001, "{}", area(&s, id));
    }
    assert!(record.notes.iter().all(|n| n.severity != Severity::Warning), "{:?}", record.notes);
    assert!(record.messages.iter().any(|m| m.contains("14 ends run on past")), "{:?}", record.messages);
    s.undo().unwrap();
    assert_eq!(s.project().objects().count(), before);
}

/// The middle line across stops 5 mm short of the north line: joined at 1 cm and said; at the 1 mm default, listed
/// with its gap and place, and the two blocks either side of it are one.
#[test]
fn an_end_stopping_short_is_joined_within_the_tolerance_or_listed() {
    let mut s = Session::with_builtins();
    let mut lines = vec![line(&mut s, "south", &[(0.0, 0.0), (200.0, 0.0)]), line(&mut s, "north", &[(0.0, 360.0), (200.0, 360.0)])];
    lines.push(line(&mut s, "west", &[(0.0, 0.0), (0.0, 360.0)]));
    lines.push(line(&mut s, "middle", &[(100.0, 0.0), (100.0, 359.995)]));
    lines.push(line(&mut s, "east", &[(200.0, 0.0), (200.0, 360.0)]));

    let joined = s.run("string.polygons_from_lines", json!({ "strings": lines, "layer": "blocks", "tolerance": 0.01 })).unwrap();
    assert_eq!(created(&joined).len(), 2);
    let note = joined.notes.iter().find(|n| n.code == Some(Code::StringEndJoined)).expect("the join is said");
    assert!(note.text.contains("'middle'") && note.text.contains("5.0 mm"), "{}", note.text);

    let open = s.run("string.polygons_from_lines", json!({ "strings": lines, "layer": "blocks" })).unwrap();
    let made = created(&open);
    assert_eq!(made.len(), 1);
    assert!((area(&s, made[0]) - 72_000.0).abs() < 0.001);
    let short = open.notes.iter().find(|n| n.code == Some(Code::StringEndShort)).expect("the short end is listed");
    assert_eq!(short.severity, Severity::Warning);
    assert!(short.text.contains("'middle' ends 0.005 m short"), "{}", short.text);
    assert!((short.value.as_ref().unwrap().value - 0.005).abs() < 1e-6);
}

/// Lines that close nothing are refused, saying why; a line closing nothing beside ones that do is named.
#[test]
fn lines_closing_nothing_are_refused_or_named() {
    let mut s = Session::with_builtins();
    let a = line(&mut s, "a", &[(0.0, 0.0), (100.0, 0.0)]);
    let b = line(&mut s, "b", &[(0.0, 50.0), (100.0, 50.0)]);
    let refused = s.run("string.polygons_from_lines", json!({ "strings": [a, b], "layer": "blocks" })).unwrap_err();
    assert!(refused.to_string().contains("close no area"), "{refused}");

    let square = created(&s.run("string.create", json!({ "name": "square", "layer": "l", "closed": true,
        "points": [[E, N + 100.0, 0.0], [E + 50.0, N + 100.0, 0.0], [E + 50.0, N + 150.0, 0.0], [E, N + 150.0, 0.0]] })).unwrap())[0];
    let record = s.run("string.polygons_from_lines", json!({ "strings": [a, square], "layer": "blocks" })).unwrap();
    assert_eq!(created(&record).len(), 1);
    let unused = record.notes.iter().find(|n| n.code == Some(Code::StringClosesNothing)).unwrap();
    assert!(unused.text.contains("'a' closes no area"));
}

/// A box at mine coordinates, `x` east and `y` north from the origin, RL 0 to `high`: wound outward.
fn box_solid(s: &mut Session, name: &str, x: (f64, f64), y: (f64, f64), high: f64) -> ObjectId {
    let vertices: Vec<[f64; 3]> = (0..8)
        .map(|i| [E + if i & 1 == 1 { x.1 } else { x.0 }, N + if i >> 1 & 1 == 1 { y.1 } else { y.0 }, if i >> 2 & 1 == 1 { high } else { 0.0 }])
        .collect();
    let triangles = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    created(&s.run("solid.from_mesh", json!({ "name": name, "layer": "strips", "vertices": vertices, "triangles": triangles })).unwrap())[0]
}

fn volume(s: &Session, id: ObjectId) -> f64 {
    let Body::Solid(solid) = &s.project().object(id).unwrap().body else { panic!("a solid") };
    solid.mesh().signed_volume()
}

fn layer_of(s: &Session, id: ObjectId) -> String {
    s.project().layer(s.project().object(id).unwrap().layer).unwrap().name.clone()
}

/// Block polygons over a strip 1,500 m long: a south and a north line past its sides and 16 lines across every 100 m.
fn block_polygons(s: &mut Session) -> Vec<ObjectId> {
    let mut lines = vec![line(s, "south", &[(-20.0, -50.0), (1520.0, -50.0)]), line(s, "north", &[(-20.0, 110.0), (1520.0, 110.0)])];
    for k in 0..16 {
        let x = 100.0 * k as f64;
        lines.push(line(s, &format!("across {k}"), &[(x, -80.0), (x, 140.0)]));
    }
    created(&s.run("string.polygons_from_lines", json!({ "strings": lines, "layer": "block polygons", "name": "B#" })).unwrap())
}

/// A strip 1,500 × 60 × 30 m cut by 15 polygons across it: 15 blocks of 180,000 m³, each named for its polygon and
/// carrying it as `block`, nothing outside, all in one undo.
#[test]
fn a_strip_cut_by_block_polygons_into_named_blocks() {
    let mut s = Session::with_builtins();
    let strip = box_solid(&mut s, "Strip 5", (0.0, 1500.0), (0.0, 60.0), 30.0);
    let polygons = block_polygons(&mut s);
    assert_eq!(polygons.len(), 15);
    let before = s.project().objects().count();
    let record = s.run("solid.cut_by_polygons", json!({ "solids": [strip], "polygons": polygons, "layer": "blocks" })).unwrap();
    let made = created(&record);
    println!("{}", record.messages.join("\n"));
    assert_eq!(made.len(), 15);
    for (k, &id) in made.iter().enumerate() {
        let object = s.project().object(id).unwrap();
        assert_eq!(object.name, format!("B{}", k + 1), "no strip or seam yet, so the block alone");
        assert_eq!(object.attributes.get("block"), Some(&AttrValue::Text(format!("B{}", k + 1))));
        assert!((volume(&s, id) - 180_000.0).abs() < 0.18, "{}", volume(&s, id));
    }
    assert!(record.notes.iter().all(|n| n.severity != Severity::Warning), "{:?}", record.notes);
    assert!(record.messages.iter().any(|m| m.contains("into 15 blocks")));
    s.undo().unwrap();
    assert_eq!(s.project().objects().count(), before);
}

/// The strip's own attributes stay and win over a polygon's of the same name; the polygon's others come with it; the
/// name pattern takes both.
#[test]
fn a_block_keeps_its_solids_attributes_and_takes_the_polygons_others() {
    let mut s = Session::with_builtins();
    let solid = box_solid(&mut s, "Strip", (0.0, 1500.0), (0.0, 60.0), 30.0);
    // Cut at RL 20: the part below carries cut_side = below.
    let below = created(&s.run("solid.cut_by_surface", json!({ "solid": solid, "rl": 20.0, "keep": "below", "layer": "strips" })).unwrap())[0];
    let polygons = block_polygons(&mut s);
    // The polygons numbered across as strip, which the solid hasn't, then as cut_side, which it has (the last
    // numbering names them).
    let across = |s: &mut Session, attribute: &str, start: f64| {
        s.run("string.number_across", json!({ "strings": polygons, "from": [E - 50.0, N + 30.0, 0.0], "to": [E + 1550.0, N + 30.0, 0.0], "start": start, "attribute": attribute, "name": "B#" })).unwrap();
    };
    across(&mut s, "strip", 701.0);
    across(&mut s, "cut_side", 1.0);
    let record = s.run("solid.cut_by_polygons", json!({ "solids": [below], "polygons": polygons, "layer": "blocks", "name": "S{strip} {block} {cut_side}" })).unwrap();
    let first = s.project().object(created(&record)[0]).unwrap();
    assert_eq!(first.attributes.get("cut_side"), Some(&AttrValue::Text("below".into())), "the solid's own wins");
    assert_eq!(first.attributes.get("strip"), Some(&AttrValue::Number(701.0)), "the polygon's comes with it");
    assert_eq!(first.name, "S701 B1 below");
    assert!((volume(&s, first.id) - 120_000.0).abs() < 0.12);
}

/// Blocks over the west 1,000 m leave the east 500 m of the strip, 30,000 m² in plan, and a strip away from every
/// polygon is left whole, 6,000 m²: the check finds both on their solids, and the cut is refused naming them, nothing
/// made. With all 15 polygons over the strip alone the check passes and the cut goes ahead.
#[test]
fn solids_the_polygons_dont_cover_are_found_first_and_refused() {
    let mut s = Session::with_builtins();
    let strip = box_solid(&mut s, "Strip 1", (0.0, 1500.0), (0.0, 60.0), 30.0);
    let away = box_solid(&mut s, "Away", (5_000.0, 5_100.0), (0.0, 60.0), 30.0);
    let polygons = block_polygons(&mut s);
    let short = json!({ "solids": [strip, away], "polygons": polygons[..10], "layer": "blocks" });
    let params: CutByPolygonsParams = serde_json::from_value(short.clone()).unwrap();
    let started = std::time::Instant::now();
    let found = check_cut_by_polygons(s.project(), &params).unwrap();
    println!("checked in {:.1} ms: {found:?}", started.elapsed().as_secs_f64() * 1000.0);
    let warnings: Vec<_> = found.iter().filter(|n| n.code == Some(Code::SolidOutsidePolygons)).collect();
    assert_eq!(warnings.len(), 2);
    assert!(warnings[0].text.starts_with("'Strip 1' reaches outside the polygons by 30000.0 m² about 651250.000 7450030.000"), "{}", warnings[0].text);
    assert!(warnings[1].text.starts_with("'Away' reaches outside the polygons by 6000.0 m²"), "{}", warnings[1].text);
    assert!(warnings.iter().all(|n| n.place.is_some()));

    let before = s.project().objects().count();
    let refused = s.run("solid.cut_by_polygons", short).unwrap_err().to_string();
    assert!(refused.contains("the block polygons don't cover 2 solids, so nothing is cut: 'Strip 1' reaches outside them by 30000.0 m²"), "{refused}");
    assert!(refused.contains("'Away' reaches outside them by 6000.0 m²"), "{refused}");
    assert!(refused.ends_with("Carry the polygons on over the whole of each solid, then cut"), "{refused}");
    assert_eq!(s.project().objects().count(), before, "nothing made");

    let whole: CutByPolygonsParams = serde_json::from_value(json!({ "solids": [strip], "polygons": polygons, "layer": "blocks" })).unwrap();
    let found = check_cut_by_polygons(s.project(), &whole).unwrap();
    assert!(found.iter().all(|n| n.severity != Severity::Warning), "{found:?}");
    assert!(found.iter().any(|n| n.text == "the polygons cover all 1 solids"));
    assert_eq!(created(&s.run("solid.cut_by_polygons", json!({ "solids": [strip], "polygons": polygons, "layer": "blocks" })).unwrap()).len(), 15);
}

/// Two strips cut by the same polygons share names unless the pattern tells them apart.
#[test]
fn repeated_names_are_said() {
    let mut s = Session::with_builtins();
    let strip = box_solid(&mut s, "Strip 1", (0.0, 1500.0), (0.0, 60.0), 30.0);
    let second = box_solid(&mut s, "Strip 2", (0.0, 1500.0), (60.0, 100.0), 30.0);
    let polygons = block_polygons(&mut s);
    let shared = s.run("solid.cut_by_polygons", json!({ "solids": [strip, second], "polygons": polygons, "layer": "more blocks" })).unwrap();
    let note = shared.notes.iter().find(|n| n.code == Some(Code::BlockNameRepeated)).expect("names repeat");
    assert!(note.text.starts_with("30 blocks share 15 names"), "{}", note.text);
    let apart = s.run("solid.cut_by_polygons", json!({ "solids": [strip, second], "polygons": polygons, "layer": "named blocks", "name": "{solid} {block}" })).unwrap();
    assert!(apart.notes.iter().all(|n| n.code != Some(Code::BlockNameRepeated)));
}

#[test]
fn overlapping_polygons_are_refused() {
    let mut s = Session::with_builtins();
    let strip = box_solid(&mut s, "Strip", (0.0, 300.0), (0.0, 60.0), 30.0);
    let square = |s: &mut Session, name: &str, x0: f64| {
        let points = [[E + x0, N - 50.0, 0.0], [E + x0 + 100.0, N - 50.0, 0.0], [E + x0 + 100.0, N + 110.0, 0.0], [E + x0, N + 110.0, 0.0]];
        created(&s.run("string.create", json!({ "name": name, "layer": "p", "closed": true, "points": points })).unwrap())[0]
    };
    let (a, b) = (square(&mut s, "A", 0.0), square(&mut s, "B", 90.0));
    let refused = s.run("solid.cut_by_polygons", json!({ "solids": [strip], "polygons": [a, b], "layer": "blocks" })).unwrap_err();
    assert!(refused.to_string().contains("'A' and 'B' overlap by 1600.00 m²"), "{refused}");
}
