//! The `.zdp` file (the Store plan's stage 2): one zip, saved beside then swapped in, the same bytes every time,
//! nothing written when nothing changed, and a lock note that refuses a save over someone else's open file.

use zd_core::{Change, Polyline, RunId};
use zd_io::{LockInfo, Snapshot, StoreError, ZdpStore, project_differences};

mod common;
use common::*;

#[test]
fn a_project_saves_as_one_file_and_comes_back_the_same() {
    let dir = TempDir::new("zdpfile-roundtrip");
    let path = dir.0.join("pit.zdp");
    let project = sample_project();
    let mut store = ZdpStore::create(&path);
    let saved = store.save(&Snapshot::of(&project), "[]", true).unwrap();
    assert!(!saved.unchanged && saved.bytes == std::fs::metadata(&path).unwrap().len());
    let names: Vec<String> = std::fs::read_dir(&dir.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names.len(), 2, "the file and its lock note, nothing left beside it: {names:?}");
    drop(store);
    assert!(!dir.0.join("pit.zdp.lock").exists(), "our lock goes with the store");

    let (_store, read) = ZdpStore::open(&path).unwrap();
    assert_eq!(project_differences(&project, &read.project), Vec::<String>::new());
    assert_eq!(read.run_log.as_deref(), Some("[]"));
}

#[test]
fn the_same_project_saves_to_the_same_bytes() {
    let dir = TempDir::new("zdpfile-same");
    let project = sample_project();
    for name in ["a.zdp", "b.zdp"] {
        ZdpStore::create(dir.0.join(name)).save(&Snapshot::of(&project), "[]", true).unwrap();
    }
    assert!(std::fs::read(dir.0.join("a.zdp")).unwrap() == std::fs::read(dir.0.join("b.zdp")).unwrap());
}

#[test]
fn nothing_is_written_when_nothing_has_changed() {
    let dir = TempDir::new("zdpfile-unchanged");
    let path = dir.0.join("pit.zdp");
    let mut project = sample_project();
    let mut store = ZdpStore::create(&path);
    store.save(&Snapshot::of(&project), "[]", true).unwrap();
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    let again = store.save(&Snapshot::of(&project), "[]", false).unwrap();
    assert!(again.unchanged && again.encoded == 2, "only the objects and strings tables are encoded to tell: {again:?}");
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before, "the file untouched");
    assert!(!store.save(&Snapshot::of(&project), "[1]", true).unwrap().unchanged, "a new run in the log is saved");

    // A look changes the file though nothing is logged.
    let topo = named(&project, "topography").id;
    project.set_looks(&[topo], None, Some(0.5));
    assert!(!store.save(&Snapshot::of(&project), "[1]", false).unwrap().unchanged);

    // An opened file has nothing new to save, and a change re-encodes only its own groups.
    drop(store);
    let (mut store, read) = ZdpStore::open(&path).unwrap();
    assert!(store.save(&Snapshot::of(&read.project), "[1]", false).unwrap().unchanged);
    let mut project = read.project;
    let crest = named(&project, "crest");
    let edited = zd_core::Body::String(Polyline { vertices: vec![[E, N, 9.0], [E + 1.0, N, 9.0], [E, N + 1.0, 9.0]], closed: true });
    project.apply(RunId::new(), vec![Change::revise_body(&crest, edited)]).unwrap();
    let saved = store.save(&Snapshot::of(&project), "[1]", true).unwrap();
    assert_eq!(saved.encoded, 3, "objects, strings and the crest's attribute group; meshes and grids kept");

    // A rename keeps the geometry, so its mesh isn't encoded again.
    let topo = named(&project, "topography");
    project.apply(RunId::new(), vec![Change::Revise { id: topo.id, base: topo.revision, name: Some("topo Nov".into()), layer: None, attributes: None, body: None }]).unwrap();
    let renamed = store.save(&Snapshot::of(&project), "[1]", true).unwrap();
    assert_eq!(renamed.encoded, 3, "objects, strings and the attribute group; the topography's mesh kept");
    drop(store);
    let (_store, back) = ZdpStore::open(&path).unwrap();
    assert_eq!(project_differences(&project, &back.project), Vec::<String>::new());
}

#[test]
fn someone_elses_lock_is_reported_and_refuses_a_save() {
    let dir = TempDir::new("zdpfile-lock");
    let path = dir.0.join("pit.zdp");
    let project = sample_project();
    ZdpStore::create(&path).save(&Snapshot::of(&project), "[]", true).unwrap();
    let theirs = LockInfo { user: "someone-else".into(), machine: "OTHER-PC".into(), pid: 4242, since_unix_ms: 1_794_387_180_000 };
    std::fs::write(dir.0.join("pit.zdp.lock"), serde_json::to_vec(&theirs).unwrap()).unwrap();

    let (mut store, read) = ZdpStore::open(&path).unwrap();
    assert_eq!(store.locked_by(), Some(theirs.clone()), "opened, and who has it said");
    assert_eq!(theirs.describe(), "someone-else on OTHER-PC since 11 Nov 2026 08:53 UTC");
    let leap_day = LockInfo { since_unix_ms: 1_709_251_140_000, ..theirs.clone() };
    assert!(leap_day.describe().ends_with("29 Feb 2024 23:59 UTC"), "{}", leap_day.describe());
    let Err(StoreError::Locked(message)) = store.save(&Snapshot::of(&read.project), "[2]", true) else { panic!("a save over their lock must be refused") };
    assert!(message.starts_with("someone-else on OTHER-PC since") && message.contains("pit.zdp.lock"), "{message}");
    drop(store);
    assert!(dir.0.join("pit.zdp.lock").exists(), "their lock isn't ours to remove");

    // Once they've closed it, the save goes through and the lock is ours.
    std::fs::remove_file(dir.0.join("pit.zdp.lock")).unwrap();
    let (mut store, read) = ZdpStore::open(&path).unwrap();
    assert_eq!(store.locked_by(), None);
    assert!(store.save(&Snapshot::of(&read.project), "[2]", true).is_ok());
}

#[test]
fn a_save_cut_short_leaves_the_last_one_whole() {
    let dir = TempDir::new("zdpfile-leftover");
    let path = dir.0.join("pit.zdp");
    let project = sample_project();
    ZdpStore::create(&path).save(&Snapshot::of(&project), "[]", true).unwrap();
    // What a crash mid-save leaves: a part-written file beside the project.
    std::fs::write(dir.0.join("pit.zdp.saving"), b"PK partial").unwrap();
    let (mut store, read) = ZdpStore::open(&path).unwrap();
    assert_eq!(project_differences(&project, &read.project), Vec::<String>::new(), "the last save opens whole");
    store.save(&Snapshot::of(&read.project), "[3]", true).unwrap();
    assert!(!dir.0.join("pit.zdp.saving").exists(), "the next save writes over the leftover and swaps it in");
}

#[test]
fn a_file_that_is_not_a_project_is_refused_by_name() {
    let dir = TempDir::new("zdpfile-not");
    std::fs::create_dir_all(&dir.0).unwrap();
    let path = dir.0.join("notes.zdp");
    std::fs::write(&path, b"not a zip").unwrap();
    let Err(e) = ZdpStore::open(&path) else { panic!("refused") };
    assert!(e.to_string().contains("not a project file"), "{e}");
}

/// The targets of decision 0058 on every project saved under `ZD_STORE_PROJECTS` in today's folders: saved as a
/// `.zdp`, opened, one solid renamed and saved, then saved with nothing changed. Opt-in, as the projects are client
/// data: `cargo test --release -p zd-io --test zdp_file -- --ignored --nocapture`
#[test]
#[ignore]
fn targets_on_every_saved_project() {
    use std::time::Instant;
    let root = std::env::var("ZD_STORE_PROJECTS").unwrap_or_else(|_| panic!("ZD_STORE_PROJECTS is needed: this test reads real data, and passing without it would say nothing"));
    let mut projects = Vec::new();
    find_projects(std::path::Path::new(&root), &mut projects);
    projects.sort();
    assert!(!projects.is_empty(), "no .zdproj folders under {root}");
    println!("| Project | Objects | Folder open | File | First save | Open | Save after one change | Save, nothing changed |");
    println!("|---|---|---|---|---|---|---|---|");
    for folder in &projects {
        let t = Instant::now();
        let opened = zd_io::ProjectStore::open(folder).unwrap();
        let folder_ms = t.elapsed().as_secs_f64() * 1e3;
        let dir = TempDir::new("zdpfile-targets");
        let path = dir.0.join("project.zdp");
        let first = ZdpStore::create(&path).save(&Snapshot::of(&opened.project), opened.run_log.as_deref().unwrap_or(""), true).unwrap();
        drop(opened);
        let t = Instant::now();
        let (mut store, read) = ZdpStore::open(&path).unwrap();
        let open_ms = t.elapsed().as_secs_f64() * 1e3;
        let mut project = read.project;
        let one = project.objects().find(|o| matches!(o.body, zd_core::Body::Solid(_) | zd_core::Body::Surface(_))).or_else(|| project.objects().next()).cloned();
        let changed = match one {
            Some(o) => {
                project.apply(RunId::new(), vec![Change::Revise { id: o.id, base: o.revision, name: Some(format!("{} moved", o.name)), layer: None, attributes: None, body: None }]).unwrap();
                let saved = store.save(&Snapshot::of(&project), "[1]", true).unwrap();
                format!("{:.0} ms ({} of {} entries)", saved.ms, saved.encoded, saved.entries)
            }
            None => "empty".into(),
        };
        let unchanged = store.save(&Snapshot::of(&project), "[1]", false).unwrap();
        assert!(unchanged.unchanged);
        println!(
            "| {} | {} | {folder_ms:.0} ms | {:.1} MB | {:.0} ms | {open_ms:.0} ms | {changed} | {:.0} ms |",
            folder.file_name().unwrap().to_string_lossy(),
            project.objects().count(),
            first.bytes as f64 / 1e6,
            first.ms,
            unchanged.ms
        );
    }
}

fn find_projects(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
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

/// A mesh over a million triangles is named for the revision that holds it, so a rename renames its entries: their
/// bytes are kept, and the file reads back under the new names.
#[test]
fn a_large_surface_renamed_keeps_its_bytes_under_new_names() {
    let dir = TempDir::new("zdpfile-large-rename");
    let path = dir.0.join("pit.zdp");
    let mut project = zd_core::Project::new();
    let grid = zd_core::Grid::from_fn([E, N], [2.0, 2.0], 742, 742, |x, y| 200.0 + ((x - E) / 50.0).sin() + ((y - N) / 70.0).cos());
    let surface = zd_core::Surface::new(zd_geom::triangulate_grid(&grid));
    project.apply(RunId::new(), vec![Change::Create { name: "topography".into(), layer: "geology".into(), attributes: Default::default(), body: zd_core::Body::Surface(surface) }]).unwrap();
    let mut store = ZdpStore::create(&path);
    store.save(&Snapshot::of(&project), "[]", true).unwrap();
    let topo = named(&project, "topography");
    project.apply(RunId::new(), vec![Change::Revise { id: topo.id, base: topo.revision, name: Some("topo Nov".into()), layer: None, attributes: None, body: None }]).unwrap();
    let saved = store.save(&Snapshot::of(&project), "[]", true).unwrap();
    assert_eq!(saved.encoded, 2, "the objects and strings tables only: the mesh kept under its new name");
    drop(store);
    let (_store, back) = ZdpStore::open(&path).unwrap();
    assert_eq!(project_differences(&project, &back.project), Vec::<String>::new());
}

/// Stage 4: each seam model's grid folder is kept as given and relative to the file, and a file that has moved
/// records its new way to the folder at the next save, though nothing else changed.
#[test]
fn a_seam_model_folder_is_kept_both_ways() {
    let dir = TempDir::new("zdpfile-links");
    let geology = dir.0.join("geology");
    let mut model = seam_model();
    model.folder = geology.to_string_lossy().into_owned();
    let mut project = zd_core::Project::new();
    let made = project
        .apply(RunId::new(), vec![Change::Create { name: "PIT seams".into(), layer: "geology".into(), attributes: Default::default(), body: zd_core::Body::SeamModel(std::sync::Arc::new(model)) }])
        .unwrap();
    let id = made.changes[0].id;

    let path = dir.0.join("pit").join("pit.zdp");
    ZdpStore::create(&path).save(&Snapshot::of(&project), "[]", true).unwrap();
    let (_store, read) = ZdpStore::open(&path).unwrap();
    assert_eq!(read.links, [zd_io::FolderLink { object: id, folder: geology.to_string_lossy().into_owned(), from_project: Some("../geology".into()) }]);
    drop(_store);

    let moved = dir.0.join("pits").join("2026").join("pit.zdp");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&path, &moved).unwrap();
    let (mut store, read) = ZdpStore::open(&moved).unwrap();
    assert_eq!(read.links[0].from_project.as_deref(), Some("../geology"), "as saved, before it moved");
    let saved = store.save(&Snapshot::of(&read.project), "[]", false).unwrap();
    assert!(!saved.unchanged, "the way to the grids changed with the move");
    assert!(store.save(&Snapshot::of(&read.project), "[]", false).unwrap().unchanged);
    drop(store);
    assert_eq!(ZdpStore::open(&moved).unwrap().1.links[0].from_project.as_deref(), Some("../../geology"));
}
