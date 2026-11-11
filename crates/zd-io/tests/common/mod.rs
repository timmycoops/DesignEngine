//! What the store tests share: a temporary folder and a sample project with one of everything.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use zd_core::{
    AttrValue, Attributes, Body, Change, Fingerprint, Grid, GridLink, Horizon, IssueKind, Lattice, Object, Polyline, Project, RunId, SeamModel, Solid,
    StackIssue, Surface, TriMesh,
};
use zd_geom::triangulate_grid;

pub const E: f64 = 650_000.123_456_789;
pub const N: f64 = 7_450_000.987_654_321;

/// A fresh, empty folder under the temp directory; removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("zd-store-{tag}-{}-{}", std::process::id(), RunId::new()));
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn create(name: &str, layer: &str, body: Body) -> Change {
    Change::Create { name: name.into(), layer: layer.into(), attributes: Attributes::new(), body }
}

pub fn box_mesh(min: [f64; 3], max: [f64; 3]) -> TriMesh {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    TriMesh::new(
        vec![[x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
        vec![[0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7], [0, 1, 5], [0, 5, 4], [1, 2, 6], [1, 6, 5], [2, 3, 7], [2, 7, 6], [3, 0, 4], [3, 4, 7]],
    )
}

/// A seam model linking to grid files, with a non-blocking issue at a mine grid location.
pub fn seam_model() -> SeamModel {
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
pub fn sample_project() -> Project {
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

pub fn named(p: &Project, name: &str) -> Arc<Object> {
    p.objects().find(|o| o.name == name).unwrap_or_else(|| panic!("no object '{name}'")).clone()
}
