//! The project store: exact round trips, incremental saves, crash leftovers, versions.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use std::collections::BTreeMap;

use zd_core::{
    AttrValue, Attributes, Body, Change, Fingerprint, Grid, GridLink, Horizon, IssueKind, Lattice, Object, Polyline, Project,
    RunId, SeamModel, Solid, StackIssue, Surface, TriMesh,
};
use zd_geom::triangulate_grid;
use zd_io::{ProjectStore, StoreError};

const E: f64 = 650_000.123_456_789;
const N: f64 = 7_450_000.987_654_321;

/// A fresh, empty folder under the temp directory; removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("zd-store-{tag}-{}-{}", std::process::id(), RunId::new()));
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn create(name: &str, layer: &str, body: Body) -> Change {
    Change::Create { name: name.into(), layer: layer.into(), attributes: Attributes::new(), body }
}

fn box_mesh(min: [f64; 3], max: [f64; 3]) -> TriMesh {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    TriMesh::new(
        vec![[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]],
    )
}

/// A seam model linking to grid files, with a non-blocking issue at a mine grid location.
fn seam_model() -> SeamModel {
    let link = |file: &str, checksum: u64| GridLink {
        file: file.into(),
        fingerprint: Fingerprint { bytes: 2_128_336, modified_unix_ms: 1_700_000_000_123, checksum },
    };
    let horizon = |code: &str, parent: Option<&str>, working: bool| Horizon {
        code: code.into(),
        parent: parent.map(Into::into),
        material: "coal".into(),
        working,
        roof: link(&format!("pit{code}.srg"), u64::MAX),
        floor: link(&format!("pit{code}.sfg"), 0x8594_4171_f739_67e8),
        grids: BTreeMap::from([("rd".to_string(), link(&format!("pit{code}.rdg"), 7))]),
    };
    SeamModel {
        folder: "D:\\models\\pit\\grids".into(),
        density: "rd".into(),
        tolerance: 0.001,
        lattice: Lattice { origin: [E, N], cell_size: [10.0, 10.0], cols: 501, rows: 321 },
        no_seam: Some(160.0),
        z_range: Some([-12.375, 301.5]),
        horizons: vec![horizon("a", None, false), horizon("au", Some("a"), true), horizon("al", Some("a"), true)],
        issues: vec![StackIssue {
            kind: IssueKind::Crossing,
            horizon: "al".into(),
            other: Some("au".into()),
            nodes: 4,
            worst: 0.000_7,
            at: Some([E + 30.0, N + 1_230.0]),
            blocking: false,
        }],
    }
}

/// One of everything: strings (one closed, with attributes, at revision 3), a surface, a solid, a
/// grid with nulls, a seam model, a deleted object, and a hidden layer with its own colour.
fn sample_project() -> Project {
    let mut p = Project::new();
    p.apply(RunId::new(), vec![Change::CreateLayer { name: "geology".into(), colour: Some([10, 20, 30]) }]).unwrap();
    let topo = Grid::from_fn([E, N], [5.0, 5.0], 31, 21, |x, y| 200.0 + ((x - E) / 37.0).sin() + ((y - N) / 23.0).cos() / 3.0);
    let mut roof = Grid::from_fn([E, N], [5.0, 5.0], 31, 21, |x, y| 150.0 - 0.04 * (x - E) + (y - N) * 1e-7);
    roof.z[17] = f64::NAN;
    roof.z[400] = f64::NAN;
    p.apply(
        RunId::new(),
        vec![
            create("topography", "geology", Body::Surface(Surface::new(triangulate_grid(&topo)))),
            create("seam A roof", "geology", Body::Grid(Arc::new(roof))),
            create("seam stack", "geology", Body::SeamModel(Arc::new(seam_model()))),
            create("seam A", "solids", Body::Solid(Solid::new(box_mesh([E, N, 100.25], [E + 10.5, N + 20.75, 110.125])).unwrap())),
            create("toe", "design", Body::String(Polyline { vertices: vec![[E, N, 120.5], [E + 33.3, N + 0.1, 121.7]], closed: false })),
            create("scratch", "design", Body::String(Polyline { vertices: vec![[E, N, 0.0], [E + 1.0, N, 0.0]], closed: false })),
        ],
    )
    .unwrap();

    let mut attributes = Attributes::new();
    attributes.insert("bench".into(), AttrValue::Number(3.0));
    attributes.insert("status".into(), AttrValue::Text("draft".into()));
    attributes.insert("final".into(), AttrValue::Bool(false));
    p.apply(
        RunId::new(),
        vec![Change::Create {
            name: "crest".into(),
            layer: "design".into(),
            attributes,
            body: Body::String(Polyline { vertices: vec![[E, N, 1.0], [E + 5.0, N, 2.0], [E + 5.0, N + 5.0, 3.0]], closed: true }),
        }],
    )
    .unwrap();

    let toe = named(&p, "toe");
    let moved = Body::String(Polyline { vertices: vec![[E, N, 120.5], [E + 44.4, N + 0.2, 121.9]], closed: false });
    p.apply(RunId::new(), vec![Change::revise_body(&toe, moved)]).unwrap();
    let toe = named(&p, "toe");
    p.apply(RunId::new(), vec![Change::Revise { id: toe.id, base: toe.revision, name: Some("toe 1".into()), layer: None, attributes: None, body: None }]).unwrap();
    let scratch = named(&p, "scratch");
    p.apply(RunId::new(), vec![Change::Delete { id: scratch.id, base: scratch.revision }]).unwrap();
    let geology = p.layer_named("geology").unwrap().id;
    p.set_layer_visible(geology, false);
    p
}

fn named(p: &Project, name: &str) -> Arc<Object> {
    p.objects().find(|o| o.name == name).unwrap_or_else(|| panic!("no object '{name}'")).clone()
}

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
