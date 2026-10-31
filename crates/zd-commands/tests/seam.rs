//! Seam stack definitions and models through the registry, on synthetic Vulcan grids with known
//! answers: a seam `c` split by a parting into `cu` and `cl`, a seam `n` above and `l` below.
//!
//! The last test is opt-in, on a real folder of Vulcan grids (client data, never committed):
//! `ZD_SEAM_GRIDS=<folder> ZD_SEAM_PREFIX=<prefix> cargo test --release -p zd-commands --test seam -- --ignored --nocapture`

use std::path::{Path, PathBuf};

use serde_json::json;
use zd_commands::Session;
use zd_core::{Body, IssueKind, SeamModel};
use zd_geom::{StackHorizon, stack_issue_nodes};
use zd_io::{read_seam_definition, read_stack_grids};

const E: f64 = 500_000.0;
const N: f64 = 7_000_000.0;
const COLS: usize = 21;
const ROWS: usize = 11;

/// A fresh, empty folder under the temp directory; removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("zd-seam-{tag}-{}-{}", std::process::id(), zd_core::RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A 21 x 11 grid at 10 m in Vulcan's plain layout; `f` takes local x, y in metres.
fn vulcan_grid(cols: usize, f: impl Fn(f64, f64) -> Option<f32>) -> Vec<u8> {
    let mut out = b"SYNTHETIC SEAM GRID             ".to_vec();
    for v in [E, E + (cols - 1) as f64 * 10.0, N, N + (ROWS - 1) as f64 * 10.0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [(cols * ROWS) as i32, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [10.0f32, 10.0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for v in [cols as i32, ROWS as i32] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    for row in 0..ROWS {
        for col in 0..cols {
            let v = f(col as f64 * 10.0, row as f64 * 10.0);
            out.extend_from_slice(&v.unwrap_or(0.0).to_be_bytes());
            out.extend_from_slice(&i32::from(v.is_some()).to_be_bytes());
        }
    }
    out
}

fn flat(z: f32) -> Vec<u8> {
    vulcan_grid(COLS, |_, _| Some(z))
}

/// The folder of grids, named `pit<seam>.<variable>g`. Every horizon but the parent `c` has a
/// density grid, and `cu` has raw ash too.
fn write_pit(dir: &Path) {
    let files: [(&str, Vec<u8>); 15] = [
        ("pitn.srg", flat(120.0)),
        ("pitn.sfg", flat(117.0)),
        ("pitn.rdg", flat(1.45)),
        ("pitc.srg", flat(100.0)),
        ("pitc.sfg", flat(90.0)),
        ("pitcu.srg", flat(100.0)),
        ("pitcu.sfg", flat(96.0)),
        ("pitcu.rdg", flat(1.4)),
        ("pitcu.rag", flat(12.5)),
        ("pitcl.srg", flat(94.0)),
        ("pitcl.sfg", flat(90.0)),
        ("pitcl.rdg", flat(1.5)),
        ("pitl.srg", flat(80.0)),
        ("pitl.sfg", flat(78.0)),
        ("pitl.rdg", flat(1.55)),
    ];
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
}

fn suggest(s: &mut Session, dir: &Path, csv: &Path) -> zd_commands::RunRecord {
    s.run("seam.suggest_definition", json!({ "dir": dir.to_string_lossy(), "prefix": "PIT", "path": csv.to_string_lossy() })).unwrap()
}

fn build(s: &mut Session, csv: &Path) -> Result<zd_commands::RunRecord, zd_commands::CommandError> {
    s.run("seam.build_model", json!({ "definition": csv.to_string_lossy(), "layer": "geology" }))
}

fn model(s: &Session) -> std::sync::Arc<SeamModel> {
    let objects: Vec<_> = s.project().objects().filter_map(|o| if let Body::SeamModel(m) = &o.body { Some(m.clone()) } else { None }).collect();
    objects.last().expect("a seam model").clone()
}

#[test]
fn a_suggested_definition_nests_the_split_and_changes_nothing() {
    let t = TempDir::new("suggest");
    write_pit(&t.0);
    let csv = t.0.join("stack.csv");
    let mut s = Session::with_builtins();
    let record = suggest(&mut s, &t.0, &csv);
    assert!(!s.project().can_undo() && s.project().objects().next().is_none(), "only a file was written");
    assert_eq!(record.messages[1..6], ["n (working, 1 other grids)", "c (parent, 0 other grids)", "  cu (working, 2 other grids)", "  cl (working, 1 other grids)", "l (working, 1 other grids)"]);

    let d = read_seam_definition(&csv).unwrap();
    assert_eq!(d.folder, t.0);
    let rows: Vec<(&str, Option<&str>, bool)> = d.horizons.iter().map(|h| (h.code.as_str(), h.parent.as_deref(), h.working)).collect();
    assert_eq!(rows, [("n", None, true), ("c", None, false), ("cu", Some("c"), true), ("cl", Some("c"), true), ("l", None, true)]);
    assert_eq!((d.horizons[2].roof.as_str(), d.horizons[2].floor.as_str()), ("pitcu.srg", "pitcu.sfg"));
    assert_eq!(d.horizons[2].grids.keys().collect::<Vec<_>>(), ["ra", "rd"]);

    // An existing definition is never overwritten by accident.
    let again = s.run("seam.suggest_definition", json!({ "dir": t.0.to_string_lossy(), "prefix": "pit", "path": csv.to_string_lossy() }));
    assert!(again.unwrap_err().to_string().contains("exists"));
}

#[test]
fn a_consistent_stack_builds_with_no_issues_and_survives_a_save() {
    let t = TempDir::new("build");
    write_pit(&t.0);
    let csv = t.0.join("stack.csv");
    let mut s = Session::with_builtins();
    suggest(&mut s, &t.0, &csv);
    let record = build(&mut s, &csv).unwrap();
    assert!(record.messages[0].starts_with("5 horizons (4 working) on a 21 x 11 lattice at 10 m; 15 grids read"), "{:?}", record.messages);
    assert!(record.messages.contains(&"cu: in c, working; coal at 231 of 231 nodes, up to 4.00 m thick (mean 4.00 m)".to_string()));
    assert_eq!(record.messages.last().unwrap(), "No blocking issues: the stack can be used for solids and reserves");

    let m = model(&s);
    assert!(m.issues.is_empty() && m.is_usable(), "{:?}", m.issues);
    assert_eq!(m.z_range, Some([78.0, 120.0]), "l floor to n roof");
    assert_eq!(m.working_section().map(|h| h.code.as_str()).collect::<Vec<_>>(), ["n", "cu", "cl", "l"]);
    let roof = &m.horizon("cu").unwrap().roof;
    assert_eq!(roof.file, "pitcu.srg");
    assert_eq!(roof.fingerprint.bytes, 88 + 8 * 231);
    assert_eq!(m.path(roof), t.0.join("pitcu.srg"), "linked in place, not copied");

    let path = t.0.join("pit.zdproj");
    s.save(&path).unwrap();
    assert_eq!(*model(&Session::open(&path).unwrap()), *m);
    assert!(!path.join("objects").read_dir().unwrap().any(|_| true), "no grid data written into the project");
}

#[test]
fn a_crossing_is_flagged_where_it_is_and_blocks_but_nothing_is_changed() {
    let t = TempDir::new("crossing");
    write_pit(&t.0);
    // cl's roof rises to 97.5 on the west five columns (x = 0..40), 1.5 m above cu's floor at 96.
    let bad = vulcan_grid(COLS, |x, _| Some(if x < 45.0 { 97.5 } else { 94.0 }));
    std::fs::write(t.0.join("pitcl.srg"), &bad).unwrap();
    let csv = t.0.join("stack.csv");
    let mut s = Session::with_builtins();
    suggest(&mut s, &t.0, &csv);
    let record = build(&mut s, &csv).unwrap();

    let m = model(&s);
    assert!(!m.is_usable());
    let crossing = m.issues.iter().find(|i| i.kind == IssueKind::Crossing).unwrap();
    assert_eq!((crossing.horizon.as_str(), crossing.other.as_deref(), crossing.nodes), ("cl", Some("cu"), 5 * ROWS));
    assert_eq!((crossing.worst, crossing.at, crossing.blocking), (1.5, Some([E, N]), true));
    assert!(record.messages.contains(&"BLOCKING: cl roof above the floor of cu at 55 nodes, worst 1.500 m at E 500000.0 N 7000000.0".to_string()));
    assert_eq!(m.blocking_issues().count(), 1, "{:?}", m.issues);
    assert_eq!(std::fs::read(t.0.join("pitcl.srg")).unwrap(), bad, "the grid is left as it is");
}

#[test]
fn the_viewport_reads_the_stack_in_place_and_sees_a_changed_file() {
    let t = TempDir::new("stack");
    write_pit(&t.0);
    let csv = t.0.join("stack.csv");
    let mut s = Session::with_builtins();
    suggest(&mut s, &t.0, &csv);
    build(&mut s, &csv).unwrap();
    let m = model(&s);
    let grids = read_stack_grids(&m).unwrap();
    assert_eq!((grids.roofs.len(), grids.floors.len()), (5, 5));
    assert_eq!((grids.roofs[2].z[0], grids.floors[2].z[0]), (100.0, 96.0), "cu, top first");
    assert!(grids.changed.is_empty());

    // Remodelled after the build: the model is out of date, and says which file.
    std::fs::write(t.0.join("pitcl.sfg"), flat(89.5)).unwrap();
    assert_eq!(read_stack_grids(&m).unwrap().changed, ["pitcl.sfg"]);
    std::fs::write(t.0.join("pitcl.sfg"), flat(90.0)).unwrap();

    // The nodes the viewport marks: cl's roof raised into cu on the west five columns.
    let parents: Vec<Option<usize>> = m.horizons.iter().map(|h| h.parent.as_ref().map(|p| m.horizons.iter().position(|x| &x.code == p).unwrap())).collect();
    let bad = vulcan_grid(COLS, |x, _| Some(if x < 45.0 { 97.5 } else { 94.0 }));
    std::fs::write(t.0.join("pitcl.srg"), &bad).unwrap();
    let grids = read_stack_grids(&m).unwrap();
    let stack: Vec<StackHorizon<'_>> = m
        .horizons
        .iter()
        .enumerate()
        .map(|(i, h)| StackHorizon { code: &h.code, parent: parents[i], working: h.working, roof: &grids.roofs[i], floor: &grids.floors[i] })
        .collect();
    let flags = stack_issue_nodes(&stack, m.tolerance).unwrap();
    assert_eq!(flags[3].iter().filter(|&&f| f).count(), 5 * ROWS, "cl's crossing with cu");
    assert!(flags.iter().enumerate().all(|(h, f)| h == 3 || f.iter().all(|&x| !x)));
}

#[test]
fn a_no_seam_rl_takes_the_false_coal_out_of_the_model() {
    let t = TempDir::new("no-seam");
    // Two seams, 3 m and 4 m, in the east of the pit. West of x = 100 m both roofs sit at the mark,
    // RL 160, over floors that carry on as real elevations.
    let roof = |z: f32| vulcan_grid(COLS, move |x, _| Some(if x < 100.0 { 160.0 } else { z }));
    let files: [(&str, Vec<u8>); 4] = [("pitu.srg", roof(120.0)), ("pitu.sfg", flat(117.0)), ("pitl.srg", roof(100.0)), ("pitl.sfg", flat(96.0))];
    for (name, bytes) in &files {
        std::fs::write(t.0.join(name), bytes).unwrap();
    }
    let csv = t.0.join("stack.csv");
    let mut s = Session::with_builtins();
    let record = suggest(&mut s, &t.0, &csv);
    assert!(record.messages.iter().any(|m| m.starts_with("every roof tops out at RL 160, flat at 48 to 48 % of its nodes")), "{:?}", record.messages);
    assert_eq!(read_seam_definition(&csv).unwrap().no_seam, Some(160.0));

    let built = build(&mut s, &csv).unwrap();
    let m = model(&s);
    assert_eq!(m.no_seam, Some(160.0));
    assert!(m.is_usable(), "{:?}", m.issues);
    // 11 columns of 11 rows hold coal; the other 10 columns of 11 rows are no seam.
    let line = "u: working; coal at 121 of 231 nodes, up to 3.00 m thick (mean 3.00 m); 110 nodes at RL 160 read as no seam";
    assert!(built.messages.iter().any(|l| l == line), "{:?}", built.messages);
    assert_eq!(m.z_range, Some([96.0, 120.0]), "the mark is not part of the working section");

    // The viewport reads the same thing, and the file is left as it was.
    let grids = read_stack_grids(&m).unwrap();
    assert!(grids.roofs[0].z[0].is_nan() && grids.floors[0].z[0].is_nan(), "west: no seam");
    assert_eq!((grids.roofs[0].z[20], grids.floors[0].z[20]), (120.0, 117.0), "east: the grids' own values");
    assert_eq!(std::fs::read(t.0.join("pitu.srg")).unwrap(), roof(120.0));

    // Delete the line and the mark is read as coal, 43 m of it, crossing the seam above.
    let text = std::fs::read_to_string(&csv).unwrap();
    let kept: Vec<&str> = text.lines().filter(|l| !l.starts_with("no_seam")).collect();
    std::fs::write(&csv, kept.join("
")).unwrap();
    let mut s = Session::with_builtins();
    let built = build(&mut s, &csv).unwrap();
    assert!(built.messages.iter().any(|l| l.starts_with("u: working; coal at 231 of 231 nodes, up to 43.00 m thick")), "{:?}", built.messages);
    assert!(!model(&s).is_usable(), "{:?}", model(&s).issues);
}

#[test]
fn a_missing_density_grid_is_a_warning() {
    let t = TempDir::new("density");
    write_pit(&t.0);
    std::fs::remove_file(t.0.join("pitl.rdg")).unwrap();
    let csv = t.0.join("stack.csv");
    let mut s = Session::with_builtins();
    suggest(&mut s, &t.0, &csv);
    let record = build(&mut s, &csv).unwrap();
    let m = model(&s);
    assert_eq!(m.issues.iter().map(|i| (i.kind, i.horizon.as_str(), i.blocking)).collect::<Vec<_>>(), [(IssueKind::NoDensity, "l", false)]);
    assert!(m.is_usable());
    assert!(record.messages.contains(&"warning: l has no density grid; tonnes will need one".to_string()));
}

#[test]
fn grids_that_cannot_be_used_are_all_listed_and_nothing_is_made() {
    let t = TempDir::new("problems");
    write_pit(&t.0);
    let csv = t.0.join("stack.csv");
    let mut s = Session::with_builtins();
    suggest(&mut s, &t.0, &csv);
    std::fs::remove_file(t.0.join("pitn.sfg")).unwrap();
    std::fs::write(t.0.join("pitcu.rag"), vulcan_grid(COLS + 1, |_, _| Some(12.5))).unwrap();
    let error = build(&mut s, &csv).unwrap_err().to_string();
    assert!(error.contains("2 grid(s) cannot be used"), "{error}");
    assert!(error.contains("horizon 'n' floor: pitn.sfg"), "{error}");
    assert!(error.contains("horizon 'cu' ra: pitcu.rag is on a 22 x 11 lattice"), "{error}");
    assert!(s.project().objects().next().is_none());
}

#[test]
fn an_edited_definition_is_what_gets_built() {
    // The planner takes c as one working unit instead of its plies.
    let t = TempDir::new("edited");
    write_pit(&t.0);
    let csv = t.0.join("stack.csv");
    std::fs::write(
        &csv,
        "folder,.\nhorizon,parent,working,roof,floor,rd\n\
         N,,yes,pitn.srg,pitn.sfg,pitn.rdg\n\
         C,,yes,pitc.srg,pitc.sfg,\n\
         CU,C,no,pitcu.srg,pitcu.sfg,pitcu.rdg\n\
         CL,C,no,pitcl.srg,pitcl.sfg,pitcl.rdg\n",
    )
    .unwrap();
    let mut s = Session::with_builtins();
    build(&mut s, &csv).unwrap();
    let m = model(&s);
    assert_eq!(m.working_section().map(|h| h.code.as_str()).collect::<Vec<_>>(), ["N", "C"]);
    assert_eq!(m.issues.iter().map(|i| (i.kind, i.horizon.as_str())).collect::<Vec<_>>(), [(IssueKind::NoDensity, "C")]);
    assert_eq!(m.folder, t.0.join(".").to_string_lossy());
}

#[test]
fn a_waste_horizon_keeps_its_material_and_wants_no_density() {
    let t = TempDir::new("waste");
    write_pit(&t.0);
    let csv = t.0.join("stack.csv");
    std::fs::write(&csv, "folder,.
horizon,parent,material,working,roof,floor
N,,Waste,yes,pitn.srg,pitn.sfg
C,,coal,yes,pitc.srg,pitc.sfg
").unwrap();
    let mut s = Session::with_builtins();
    build(&mut s, &csv).unwrap();
    let m = model(&s);
    assert_eq!(m.horizons.iter().map(|h| h.material.as_str()).collect::<Vec<_>>(), ["Waste", "coal"], "as typed");
    assert_eq!(m.issues.iter().map(|i| (i.kind, i.horizon.as_str())).collect::<Vec<_>>(), [(IssueKind::NoDensity, "C")], "the coal wants a density, the waste doesn't");
}

/// Suggests a definition for a real folder, builds the model from it and prints both run logs with
/// timings. Asserts only that both runs succeed: what the stack holds is the data's business.
#[test]
#[ignore]
fn a_real_folder_suggests_and_builds() {
    let (Ok(dir), Ok(prefix)) = (std::env::var("ZD_SEAM_GRIDS"), std::env::var("ZD_SEAM_PREFIX")) else {
        eprintln!("ZD_SEAM_GRIDS and ZD_SEAM_PREFIX are not set; skipped");
        return;
    };
    let t = TempDir::new("golden");
    let csv = t.0.join("stack.csv");
    let mut s = Session::with_builtins();
    let suggested = s.run("seam.suggest_definition", json!({ "dir": dir, "prefix": prefix, "path": csv.to_string_lossy() })).unwrap();
    println!("suggest: {:.0} ms", suggested.duration_ms);
    suggested.messages.iter().for_each(|m| println!("  {m}"));
    let built = build(&mut s, &csv).unwrap();
    println!("build: {:.0} ms", built.duration_ms);
    built.messages.iter().for_each(|m| println!("  {m}"));

    // What the viewport does to show a horizon: read every roof and floor, mark the issue nodes.
    let m = model(&s);
    let start = std::time::Instant::now();
    let grids = read_stack_grids(&m).unwrap();
    let read_ms = start.elapsed().as_secs_f64() * 1e3;
    let index = |p: &str| m.horizons.iter().position(|h| h.code == p);
    let stack: Vec<StackHorizon<'_>> = m
        .horizons
        .iter()
        .enumerate()
        .map(|(i, h)| StackHorizon { code: &h.code, parent: h.parent.as_deref().and_then(index), working: h.working, roof: &grids.roofs[i], floor: &grids.floors[i] })
        .collect();
    let flags = stack_issue_nodes(&stack, m.tolerance).unwrap();
    println!(
        "viewport: {} roofs and floors read in {read_ms:.0} ms, issue nodes marked in {:.0} ms ({} marked, {} files changed)",
        2 * grids.roofs.len(),
        start.elapsed().as_secs_f64() * 1e3 - read_ms,
        flags.iter().flatten().filter(|&&f| f).count(),
        grids.changed.len()
    );
}
