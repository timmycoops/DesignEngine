//! The `.zdp` tables (the Store plan's stage 1): exact round trips, groups, large meshes, the same bytes every
//! time, and damaged or newer files refused by name.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use zd_core::{AttrValue, Attributes, Body, Change, Colouring, FaceDesign, Grid, Project, RunId, Solid, Surface};
use zd_geom::triangulate_grid;
use zd_io::{FolderContainer, ProjectStore, StoreError, encode_project, project_differences, read_project, write_project};

mod common;
use common::*;

/// The sample project, plus what it lacks: a face design, a NaN and an empty-text attribute, two objects sharing
/// one mesh, a hidden object, and looks.
fn fuller_project() -> Project {
    let mut p = sample_project();
    let topo = named(&p, "topography");
    let Body::Surface(surface) = &topo.body else { panic!("topography is a surface") };
    let mut odd = Attributes::new();
    odd.insert("ash".into(), AttrValue::Number(f64::NAN));
    odd.insert("note".into(), AttrValue::Text(String::new()));
    odd.insert("tonnes".into(), AttrValue::Number(-0.0));
    let design = FaceDesign { layer: "faces".into(), lines: Vec::new(), profiles: Vec::new(), rules: Vec::new() };
    p.apply(
        RunId::new(),
        vec![
            Change::Create { name: "topography copy".into(), layer: "geology".into(), attributes: odd, body: Body::Surface(surface.clone()) },
            Change::Create { name: "faces".into(), layer: "design".into(), attributes: Attributes::new(), body: Body::FaceDesign(Arc::new(design)) },
        ],
    )
    .unwrap();
    let solid = named(&p, "seam A").id;
    p.set_objects_visible(&[solid], false);
    p.set_looks(&[topo.id], Some(Colouring::Layer), Some(0.4));
    p
}

fn round_trip(project: &Project, dir: &Path) -> Project {
    let mut folder = FolderContainer::new(dir);
    write_project(project, "[{\"event\":\"test\"}]", &mut folder).unwrap();
    let read = read_project(&folder).unwrap();
    assert_eq!(read.run_log.as_deref(), Some("[{\"event\":\"test\"}]"));
    read.project
}

fn entry_names(project: &Project) -> Vec<String> {
    encode_project(project, "[]").unwrap().into_iter().map(|(name, _)| name).collect()
}

#[test]
fn a_project_comes_back_exactly_through_the_tables() {
    let dir = TempDir::new("zdp-roundtrip");
    let project = fuller_project();
    let back = round_trip(&project, &dir.0);
    assert_eq!(project_differences(&project, &back), Vec::<String>::new());
    assert!(back.objects().all(|o| o.name != "scratch"), "deleted objects are not saved");
    assert_eq!(named(&back, "toe 1").revision, 3);
    assert!(back.is_hidden(named(&back, "seam A").id));
    assert!(!back.can_undo(), "undo history does not survive closing");
    let AttrValue::Number(ash) = named(&back, "topography copy").attributes["ash"] else { panic!("a number") };
    assert!(ash.is_nan());
    assert_eq!(
        entry_names(&project),
        ["objects.parquet", "strings.parquet", "attributes/g00001.parquet", "meshes/g00001-vertices.parquet", "meshes/g00001-triangles.parquet"]
            .into_iter()
            .map(String::from)
            .chain(project.objects().filter(|o| matches!(o.body, Body::Grid(_))).map(|o| format!("grids/{}-r{}.parquet", o.id, o.revision)))
            .chain(["runlog.json", "manifest.json"].map(String::from))
            .collect::<Vec<_>>(),
        "a fixed order, the manifest last"
    );
}

#[test]
fn two_objects_sharing_a_mesh_write_it_once() {
    let shared = fuller_project();
    let mut copied = sample_project();
    let topo = named(&copied, "topography");
    let Body::Surface(surface) = &topo.body else { panic!("a surface") };
    let surface = surface.clone();
    let own = Surface::new(surface.mesh().clone());
    copied.apply(RunId::new(), vec![Change::Create { name: "topography copy".into(), layer: "geology".into(), attributes: Attributes::new(), body: Body::Surface(own) }]).unwrap();
    let rows = |p: &Project| {
        let bytes = encode_project(p, "[]").unwrap().into_iter().find(|(n, _)| n == "meshes/g00001-vertices.parquet").unwrap().1;
        parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(bytes)).unwrap().metadata().file_metadata().num_rows() as usize
    };
    let (topo_vertices, box_vertices) = (surface.mesh().vertices.len(), 8);
    assert_eq!(rows(&shared), topo_vertices + box_vertices, "the shared topography once");
    assert_eq!(rows(&copied), 2 * topo_vertices + box_vertices, "a copy is a mesh of its own");
}

/// 2,500 solids with attributes, so meshes and attributes each fill three groups of 1,000.
#[test]
fn thousands_of_solids_fill_groups_of_a_thousand() {
    let mut p = Project::new();
    let changes = (0..2_500)
        .map(|i| {
            let (x, y) = (E + (i % 50) as f64 * 20.0, N + (i / 50) as f64 * 20.0);
            let mut attributes = Attributes::new();
            attributes.insert("block".into(), AttrValue::Number(i as f64));
            attributes.insert("seam".into(), AttrValue::Text(["A", "B", "C"][i % 3].into()));
            attributes.insert("mined".into(), AttrValue::Bool(i % 2 == 0));
            Change::Create { name: format!("block {i}"), layer: "blocks".into(), attributes, body: Body::Solid(Solid::new(box_mesh([x, y, 100.0], [x + 10.0, y + 10.0, 105.5])).unwrap()) }
        })
        .collect();
    p.apply(RunId::new(), changes).unwrap();
    let names = entry_names(&p);
    for group in ["g00001", "g00002", "g00003"] {
        assert!(names.contains(&format!("attributes/{group}.parquet")) && names.contains(&format!("meshes/{group}-vertices.parquet")), "{group} in {names:?}");
    }
    assert!(!names.iter().any(|n| n.contains("g00004")));
    let dir = TempDir::new("zdp-groups");
    let back = round_trip(&p, &dir.0);
    assert_eq!(project_differences(&p, &back), Vec::<String>::new());
    assert_eq!(back.objects().map(|o| o.mesh_volume()).sum::<f64>(), 2_500.0 * 550.0, "every block 10 x 10 x 5.5 m");
}

trait Volume {
    fn mesh_volume(&self) -> f64;
}

impl Volume for zd_core::Object {
    fn mesh_volume(&self) -> f64 {
        match &self.body {
            Body::Solid(s) => s.volume(),
            _ => 0.0,
        }
    }
}

#[test]
fn a_mesh_over_a_million_triangles_goes_in_entries_of_its_own() {
    let mut p = Project::new();
    let grid = Grid::from_fn([E, N], [2.0, 2.0], 742, 742, |x, y| 200.0 + ((x - E) / 50.0).sin() + ((y - N) / 70.0).cos());
    let mesh = triangulate_grid(&grid);
    assert!(mesh.triangles.len() > 1_000_000);
    p.apply(RunId::new(), vec![Change::Create { name: "topography".into(), layer: "geology".into(), attributes: Attributes::new(), body: Body::Surface(Surface::new(mesh)) }]).unwrap();
    let topo = named(&p, "topography");
    let names = entry_names(&p);
    assert!(names.contains(&format!("meshes/{}-r1-vertices.parquet", topo.id)), "{names:?}");
    assert!(!names.iter().any(|n| n.starts_with("meshes/g")));
    let dir = TempDir::new("zdp-large");
    assert_eq!(project_differences(&p, &round_trip(&p, &dir.0)), Vec::<String>::new());
}

#[test]
fn the_same_project_encodes_to_the_same_bytes() {
    let project = fuller_project();
    assert!(encode_project(&project, "[]").unwrap() == encode_project(&project, "[]").unwrap());
}

#[test]
fn a_damaged_entry_is_refused_by_name() {
    let dir = TempDir::new("zdp-damaged");
    let project = fuller_project();
    write_project(&project, "[]", &mut FolderContainer::new(&dir.0)).unwrap();
    let file = dir.0.join("meshes").join("g00001-vertices.parquet");
    let mut bytes = std::fs::read(&file).unwrap();
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xff;
    std::fs::write(&file, bytes).unwrap();
    let Err(e) = read_project(&FolderContainer::new(&dir.0)) else { panic!("a damaged file must be refused") };
    assert!(e.to_string().contains("meshes/g00001-vertices.parquet is not as saved"), "{e}");
}

#[test]
fn a_newer_file_or_table_is_refused() {
    let dir = TempDir::new("zdp-newer");
    write_project(&fuller_project(), "[]", &mut FolderContainer::new(&dir.0)).unwrap();
    let manifest = std::fs::read_to_string(dir.0.join("manifest.json")).unwrap();
    std::fs::write(dir.0.join("manifest.json"), manifest.replacen("\"objects\": 1", "\"objects\": 2", 1)).unwrap();
    let Err(e) = read_project(&FolderContainer::new(&dir.0)) else { panic!("a newer table must be refused") };
    assert!(e.to_string().contains("the objects table is version 2"), "{e}");
    std::fs::write(dir.0.join("manifest.json"), manifest.replacen("\"version\": 1", "\"version\": 2", 1)).unwrap();
    assert!(matches!(read_project(&FolderContainer::new(&dir.0)), Err(StoreError::Version { version: 2, .. })));
}

/// Every project saved in today's folders under `ZD_STORE_PROJECTS`, opened, written as tables and read back,
/// must come back the same. Opt-in, as the projects are client data:
/// `cargo test --release -p zd-io --test zdp -- --ignored --nocapture`
#[test]
#[ignore]
fn every_saved_project_comes_back_the_same_through_the_tables() {
    let root = std::env::var("ZD_STORE_PROJECTS").unwrap_or_else(|_| panic!("ZD_STORE_PROJECTS is needed: this test reads real data, and passing without it would say nothing"));
    let mut projects = Vec::new();
    find_projects(Path::new(&root), &mut projects);
    projects.sort();
    assert!(!projects.is_empty(), "no .zdproj folders under {root}");
    let mut failed = Vec::new();
    for path in &projects {
        let start = Instant::now();
        let opened = ProjectStore::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let v1_ms = start.elapsed().as_secs_f64() * 1e3;
        let dir = TempDir::new("zdp-golden");
        let mut folder = FolderContainer::new(&dir.0);
        let written = write_project(&opened.project, opened.run_log.as_deref().unwrap_or(""), &mut folder).unwrap();
        let read = read_project(&folder).unwrap();
        let differences = project_differences(&opened.project, &read.project);
        println!(
            "{}: {} objects; today's open {v1_ms:.0} ms; tables {} entries, {:.1} MB, encoded in {:.0} ms, written in {:.0} ms, read in {:.0} ms; {} differences",
            path.file_name().unwrap().to_string_lossy(),
            opened.project.objects().count(),
            written.entries,
            written.bytes as f64 / 1e6,
            written.encode_ms,
            written.write_ms,
            read.ms,
            differences.len()
        );
        if !differences.is_empty() {
            failed.push((path.clone(), differences.into_iter().take(5).collect::<Vec<_>>()));
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}

fn find_projects(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.extension().is_some_and(|e| e == "zdproj") && path.join("manifest.json").exists() {
            out.push(path);
        } else {
            find_projects(&path, out);
        }
    }
}
