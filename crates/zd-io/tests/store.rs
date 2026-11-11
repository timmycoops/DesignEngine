//! The project store: exact round trips, incremental saves, crash leftovers, versions.

use std::time::Instant;

use zd_core::{Body, Change, Colouring, Contours, Display, Grid, Look, Polyline, Project, RunId, Surface};
use zd_geom::triangulate_grid;
use zd_io::{ProjectStore, StoreError};

mod common;
use common::*;

/// Every saved fact must come back identically, coordinates bit for bit.
fn assert_same(a: &Project, b: &Project) {
    assert_eq!(a.layers(), b.layers());
    let (oa, ob): (Vec<_>, Vec<_>) = (a.objects().collect(), b.objects().collect());
    assert_eq!(oa.len(), ob.len());
    for (x, y) in oa.iter().zip(&ob) {
        assert_eq!((x.id, x.revision, &x.name, x.layer, x.provenance, &x.attributes), (y.id, y.revision, &y.name, y.layer, y.provenance, &y.attributes));
        match (&x.body, &y.body) {
            (Body::String(p), Body::String(q)) => assert_eq!(p, q),
            (Body::Surface(p), Body::Surface(q)) => {
                assert_eq!(p.mesh(), q.mesh());
                assert_eq!(p.validity(), q.validity());
            }
            (Body::Solid(p), Body::Solid(q)) => assert_eq!(p.mesh(), q.mesh()),
            (Body::Grid(p), Body::Grid(q)) => {
                assert_eq!((p.origin, p.cell_size, p.cols, p.rows), (q.origin, q.cell_size, q.cols, q.rows));
                assert!(p.z.iter().zip(&q.z).all(|(u, v)| u.to_bits() == v.to_bits()), "grid z bit for bit, nulls included");
            }
            (Body::SeamModel(p), Body::SeamModel(q)) => assert_eq!(p, q),
            _ => panic!("'{}' changed kind", x.name),
        }
    }
}

#[test]
fn save_then_open_gives_back_exactly_the_same_project() {
    let dir = TempDir::new("roundtrip");
    let project = sample_project();
    let mut store = ProjectStore::create(dir.0.join("pit.zdproj")).unwrap();
    let report = store.save(&project, "[{\"event\":\"test\"}]").unwrap();
    assert_eq!(report.object_files_written, 5, "surface 2 + solid 2 + grid 1");

    let opened = ProjectStore::open(dir.0.join("pit.zdproj")).unwrap();
    assert_same(&project, &opened.project);
    assert_eq!(opened.run_log.as_deref(), Some("[{\"event\":\"test\"}]"));
    assert!(opened.project.objects().all(|o| o.name != "scratch"), "deleted objects are not saved");
    assert_eq!(named(&opened.project, "toe 1").revision, 3);
    assert!(!opened.project.can_undo(), "undo history does not survive closing");
}

#[test]
fn looks_save_with_the_project_and_one_saved_without_them_opens_with_the_defaults() {
    let dir = TempDir::new("looks");
    let mut project = sample_project();
    let mut store = ProjectStore::create(dir.0.join("pit.zdproj")).unwrap();
    store.save(&project, "[]").unwrap();
    let manifest = std::fs::read_to_string(dir.0.join("pit.zdproj").join("manifest.json")).unwrap();
    assert!(!manifest.contains("\"display\""), "nothing set, nothing written: older builds read it as before");
    assert_eq!(*ProjectStore::open(dir.0.join("pit.zdproj")).unwrap().project.display(), Display::default());

    let (topo, roof, toe) = (named(&project, "topography").id, named(&project, "seam A roof").id, named(&project, "toe 1"));
    let geology = project.layer_named("geology").unwrap().id;
    project.set_looks(&[topo], Some(Colouring::Layer), Some(0.4));
    project.set_looks(&[roof], None, Some(0.75));
    project.set_looks(&[toe.id], None, Some(0.5));
    project.set_layer_opacity(geology, 0.6);
    project.set_contours(Contours { interval: Some(2.0), edge_lines: true, shared: true });
    // A look on an object deleted since stays in the session for an undo, not in the file.
    project.apply(RunId::new(), vec![Change::Delete { id: toe.id, base: toe.revision }]).unwrap();
    store.save(&project, "[]").unwrap();

    let opened = ProjectStore::open(dir.0.join("pit.zdproj")).unwrap().project;
    let display = opened.display();
    assert_eq!(display.look(topo), Look { colouring: Colouring::Layer, opacity: 0.4 });
    assert_eq!(display.look(roof), Look { colouring: Colouring::ByRl, opacity: 0.75 });
    assert_eq!(display.objects.len(), 2, "the deleted string's look isn't saved");
    assert_eq!(display.layer_opacity(geology), 0.6);
    assert_eq!(display.contours, Contours { interval: Some(2.0), edge_lines: true, shared: true });
}

/// Contour lines were on by default until 2026-11-09, and a project with any look set wrote `lines: true`
/// whether anyone chose them or not. That key is passed over, so such a project opens with them off, and its
/// interval and shared range as they were.
#[test]
fn a_project_saved_while_contour_lines_were_on_by_default_opens_with_them_off() {
    let dir = TempDir::new("old-contours");
    let path = dir.0.join("pit.zdproj");
    let mut project = sample_project();
    let topo = named(&project, "topography").id;
    project.set_looks(&[topo], None, Some(0.5));
    project.set_contours(Contours { interval: Some(5.0), edge_lines: false, shared: true });
    ProjectStore::create(&path).unwrap().save(&project, "[]").unwrap();
    let manifest = std::fs::read_to_string(path.join("manifest.json")).unwrap();
    let new_key = "\"edge_lines\": false";
    assert!(manifest.contains(new_key), "saved under its new key: {manifest}");
    std::fs::write(path.join("manifest.json"), manifest.replace(new_key, "\"lines\": true")).unwrap();

    let opened = ProjectStore::open(&path).unwrap().project;
    assert_eq!(opened.display().contours, Contours { interval: Some(5.0), edge_lines: false, shared: true });
    assert_eq!(opened.display().look(topo).opacity, 0.5, "the rest of its looks as saved");
    assert!(!Contours::default().edge_lines, "off for a new project too");
}

#[test]
fn a_second_save_rewrites_only_what_changed() {
    let dir = TempDir::new("incremental");
    let path = dir.0.join("pit.zdproj");
    let mut project = sample_project();
    let mut store = ProjectStore::create(&path).unwrap();
    store.save(&project, "[]").unwrap();

    // Rename the surface (a new revision that shares its mesh) and edit a string.
    let topo = named(&project, "topography");
    project
        .apply(RunId::new(), vec![Change::Revise { id: topo.id, base: topo.revision, name: Some("topo Oct".into()), layer: None, attributes: None, body: None }])
        .unwrap();
    let crest = named(&project, "crest");
    let edited = Body::String(Polyline { vertices: vec![[E, N, 9.0], [E + 1.0, N, 9.0], [E, N + 1.0, 9.0]], closed: true });
    project.apply(RunId::new(), vec![Change::revise_body(&crest, edited)]).unwrap();

    let report = store.save(&project, "[]").unwrap();
    assert_eq!(report.object_files_written, 0, "no mesh or grid rewritten");
    assert_eq!(report.object_files_reused, 5);
    assert_eq!(report.files_deleted, 2, "the previous strings file and run log");
    assert_same(&project, &ProjectStore::open(&path).unwrap().project);

    // An opened project saves back incrementally too.
    let mut opened = ProjectStore::open(&path).unwrap();
    let again = opened.store.save(&opened.project, "[]").unwrap();
    assert_eq!((again.object_files_written, again.object_files_reused), (0, 5));
}

#[test]
fn stale_files_go_and_everything_else_stays() {
    let dir = TempDir::new("cleanup");
    let path = dir.0.join("pit.zdproj");
    let mut project = sample_project();
    let mut store = ProjectStore::create(&path).unwrap();
    store.save(&project, "[]").unwrap();

    // Leftovers of a save that died before its manifest swap, plus files that are not ours.
    std::fs::write(path.join("objects").join("deadbeef-r1-vertices.parquet"), b"partial").unwrap();
    std::fs::write(path.join("manifest.json.tmp"), b"{").unwrap();
    std::fs::write(path.join("notes.txt"), b"keep me").unwrap();
    std::fs::write(path.join("objects").join("readme.txt"), b"keep me too").unwrap();
    assert_same(&project, &ProjectStore::open(&path).unwrap().project);

    let solid = named(&project, "seam A");
    project.apply(RunId::new(), vec![Change::Delete { id: solid.id, base: solid.revision }]).unwrap();
    let report = store.save(&project, "[]").unwrap();
    // The solid's two files, the junk object file, the old strings file and run log. (The junk
    // staging manifest is overwritten by this save's own and renamed into place.)
    assert_eq!(report.files_deleted, 5);
    assert!(!path.join("objects").join("deadbeef-r1-vertices.parquet").exists());
    assert!(!path.join("manifest.json.tmp").exists());
    assert!(path.join("notes.txt").exists() && path.join("objects").join("readme.txt").exists());
    assert_same(&project, &ProjectStore::open(&path).unwrap().project);
}

#[test]
fn a_newer_format_version_is_refused() {
    let dir = TempDir::new("version");
    let path = dir.0.join("pit.zdproj");
    ProjectStore::create(&path).unwrap().save(&sample_project(), "[]").unwrap();
    let manifest = std::fs::read_to_string(path.join("manifest.json")).unwrap();
    std::fs::write(path.join("manifest.json"), manifest.replacen("\"version\": 1", "\"version\": 2", 1)).unwrap();
    assert!(matches!(ProjectStore::open(&path), Err(StoreError::Version { version: 2, .. })));
}

/// Save and open timings for an 8 M-triangle surface. Opt-in:
/// `cargo test --release -p zd-io --test store -- --ignored --nocapture store_timing`
#[test]
#[ignore]
fn store_timing() {
    let dir = TempDir::new("timing");
    let path = dir.0.join("big.zdproj");
    let grid = Grid::from_fn([E, N], [5.0, 5.0], 2001, 2001, |x, y| 200.0 + ((x - E) / 700.0).sin() * 8.0 + ((y - N) / 900.0).cos() * 6.0);
    let mut project = Project::new();
    project.apply(RunId::new(), vec![create("topography", "geology", Body::Surface(Surface::new(triangulate_grid(&grid))))]).unwrap();

    let mut store = ProjectStore::create(&path).unwrap();
    let report = store.save(&project, "[]").unwrap();
    println!("save 8,000,000 triangles: {:.0} ms, {:.0} MB", report.ms, report.bytes_written as f64 / 1e6);
    let start = Instant::now();
    let opened = ProjectStore::open(&path).unwrap();
    println!("open (read + validity flags): {:.0} ms", start.elapsed().as_secs_f64() * 1e3);
    let again = ProjectStore::create(&path).unwrap().save(&opened.project, "[]").unwrap();
    let mut reopened = opened.store;
    let incremental = reopened.save(&opened.project, "[]").unwrap();
    println!("fresh store resave: {:.0} ms; incremental resave: {:.0} ms", again.ms, incremental.ms);
    assert_same(&project, &opened.project);
}
