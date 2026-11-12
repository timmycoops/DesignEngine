//! `solid.compare` (decision 0061, workstream 4): made-up blocks carrying another package's numbers beside ours, set
//! beside each other by a match file. Counts, tolerances and the furthest block; nothing in the project changes.

use std::path::PathBuf;

use serde_json::json;
use zd_commands::{Code, Session};
use zd_core::{AttrValue, Attributes, Body, Change, ObjectId, Project, RunId, Solid, TriMesh};
use zd_io::ProjectStore;

const E: f64 = 500_000.0;
const N: f64 = 7_000_000.0;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("zd-compare-{tag}-{}-{}", std::process::id(), RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A 60 × 100 m block 4 m thick, `x` metres east: 24,000 m³.
fn block(x: f64) -> TriMesh {
    let vertices = (0..8).map(|i| [E + x + (i & 1) as f64 * 60.0, N + (i >> 1 & 1) as f64 * 100.0, if i >> 2 & 1 == 1 { 104.0 } else { 100.0 }]).collect();
    let triangles = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
    TriMesh::new(vertices, triangles)
}

/// Three blocks, each with Spry's `Volume`, `RD` and `Ash` (the last one with no `Ash`) beside our `rd` and `as`.
fn blocks(t: &TempDir) -> (Session, Vec<ObjectId>) {
    let n = |v: f64| AttrValue::Number(v);
    let rows: [(&str, f64, f64, Option<f64>, f64, f64); 3] = [
        // name, Spry's volume, RD, ash; our rd, as
        ("near", 24_010.0, 1.400, Some(12.50), 1.402, 12.52),
        ("far", 24_100.0, 1.450, Some(12.50), 1.452, 12.60),
        ("no ash", 24_000.0, 1.500, None, 1.498, 11.00),
    ];
    let mut project = Project::new();
    let changes = rows
        .iter()
        .enumerate()
        .map(|(k, (name, volume, rd, ash, our_rd, our_as))| {
            let mut a: Attributes = [("Volume", n(*volume)), ("RD", AttrValue::Text(format!("{rd}"))), ("rd", n(*our_rd)), ("as", n(*our_as))].into_iter().collect();
            if let Some(ash) = ash {
                a.insert("Ash".into(), n(*ash));
            }
            Change::Create { name: name.to_string(), layer: "blocks".into(), attributes: a, body: Body::Solid(Solid::new(block(k as f64 * 70.0)).unwrap()) }
        })
        .collect();
    project.apply(RunId::new(), changes).unwrap();
    let path = t.0.join("pit.zdproj");
    ProjectStore::create(&path).unwrap().save(&project, "[]").unwrap();
    let s = Session::open(&path).unwrap();
    let ids = s.project().objects().map(|o| o.id).collect();
    (s, ids)
}

#[test]
fn each_value_is_set_beside_theirs_and_the_furthest_named() {
    let t = TempDir::new("values");
    let (mut s, ids) = blocks(&t);
    let matches = t.0.join("match.csv");
    std::fs::write(&matches, "source,ours,tolerance\nVolume,volume,\nRD,rd,\nAsh,as,0.05\n").unwrap();
    let record = s.run("solid.compare", json!({ "solids": ids, "matches": matches.to_string_lossy(), "source": "Spry" })).unwrap();
    let lines = &record.messages;
    // Volume, 0.1 % unless given: 24,010 is within 24 m of ours, 24,100 isn't.
    assert!(lines[0].starts_with("Spry Volume against volume: 3 of 3 blocks compared, 2 within 0.1 %"), "{lines:?}");
    let volume = &record.notes[1];
    assert_eq!(volume.code, Some(Code::CompareBeyond));
    assert_eq!(volume.object, Some(ids[1]), "'far' is the furthest");
    assert!(volume.text.contains("1 block beyond 0.1 %") && volume.place.is_some(), "{}", volume.text);
    // RD, read from text as Spry may keep it, 0.005 unless given: all within.
    assert!(lines[2].starts_with("Spry RD against rd: 3 of 3 blocks compared, 3 within 0.005"), "{lines:?}");
    // Ash within 0.05: 12.52 is, 12.60 isn't, and 'no ash' has none of Spry's.
    assert!(lines[3].starts_with("Spry Ash against as: 2 of 3 blocks compared, 1 within 0.05") && lines[3].ends_with("1 with no Ash"), "{lines:?}");
    assert_eq!(record.notes[4].object, Some(ids[1]));
    assert_eq!(record.notes.len(), 5);
    // A report: nothing changed, nothing to undo.
    assert!(record.outputs.is_empty());
    assert!(ids.iter().all(|&id| s.project().object(id).unwrap().revision == 1));
    assert!(!s.project().can_undo());
}

#[test]
fn a_match_file_with_mistakes_is_refused_by_line() {
    let t = TempDir::new("mistakes");
    let (mut s, ids) = blocks(&t);
    let matches = t.0.join("match.csv");
    std::fs::write(&matches, "source,ours,tolerance\nAsh,as,close\n").unwrap();
    let refused = s.run("solid.compare", json!({ "solids": ids, "matches": matches.to_string_lossy() })).unwrap_err().to_string();
    assert!(refused.contains("line 2: 'close' isn't a tolerance"), "{refused}");
}
