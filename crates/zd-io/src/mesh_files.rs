//! The triangulation files in a selection of files, folders and zips (decision 0019). Spry exports
//! solids as a zip, and a pit holds tens of thousands of them: a zip is read into memory once, so
//! each file in it costs no more than the file itself, and nothing is written to disk.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zip::ZipArchive;

use crate::{IoError, SurfaceImport, parse_dxf, parse_vulcan_triangulation, read_surface};

/// The kinds of file that hold a triangulation.
const MESH_EXTENSIONS: [&str; 2] = ["00t", "dxf"];

type Zip = ZipArchive<Cursor<Arc<[u8]>>>;

/// One triangulation file in a selection, on disk or in a zip.
#[derive(Debug, Clone)]
pub struct MeshFile {
    /// The file's name without its extension.
    pub name: String,
    /// Where it is, for the run record: a path, or `<zip>!<entry>`.
    pub place: String,
    source: Source,
}

#[derive(Debug, Clone)]
enum Source {
    Disk(PathBuf),
    Zipped { zip: Box<Zip>, index: usize, extension: String },
}

/// What a selection holds.
#[derive(Debug, Clone, Default)]
pub struct MeshSelection {
    pub files: Vec<MeshFile>,
    pub folders: usize,
    pub zips: usize,
    /// Files in the selection that hold no triangulation, passed over.
    pub left_out: usize,
}

/// Every `.00t` and `.dxf` file in `paths`: files as they are, folders with all their subfolders,
/// and zips at any depth inside. A path that cannot be read stops the search and says which.
pub fn find_mesh_files(paths: &[PathBuf]) -> Result<MeshSelection, IoError> {
    let mut selection = MeshSelection::default();
    for path in paths {
        add_path(path, &mut selection)?;
    }
    Ok(selection)
}

fn add_path(path: &Path, selection: &mut MeshSelection) -> Result<(), IoError> {
    let named = |e: std::io::Error| IoError::Io(std::io::Error::new(e.kind(), format!("{}: {e}", path.display())));
    if path.is_dir() {
        selection.folders += 1;
        let mut entries = std::fs::read_dir(path).map_err(named)?.map(|e| e.map(|e| e.path())).collect::<Result<Vec<_>, _>>().map_err(named)?;
        entries.sort();
        for entry in &entries {
            add_path(entry, selection)?;
        }
        return Ok(());
    }
    match extension_of(&path.to_string_lossy()).as_str() {
        "zip" => add_zip(path, selection),
        extension if MESH_EXTENSIONS.contains(&extension) => {
            selection.files.push(MeshFile { name: stem_of(&path.to_string_lossy()), place: path.display().to_string(), source: Source::Disk(path.to_path_buf()) });
            Ok(())
        }
        _ => {
            selection.left_out += 1;
            Ok(())
        }
    }
}

fn add_zip(path: &Path, selection: &mut MeshSelection) -> Result<(), IoError> {
    let bytes: Arc<[u8]> = std::fs::read(path).map_err(|e| IoError::Io(std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))))?.into();
    let zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| IoError::Binary(format!("{}: {e}", path.display())))?;
    selection.zips += 1;
    for index in 0..zip.len() {
        let Some(Ok(entry)) = zip.name_for_index(index) else {
            selection.left_out += 1;
            continue;
        };
        let entry = entry.into_owned();
        if entry.ends_with('/') {
            continue;
        }
        let extension = extension_of(&entry);
        if !MESH_EXTENSIONS.contains(&extension.as_str()) {
            selection.left_out += 1;
            continue;
        }
        selection.files.push(MeshFile {
            name: stem_of(&entry),
            place: format!("{}!{entry}", path.display()),
            source: Source::Zipped { zip: Box::new(zip.clone()), index, extension },
        });
    }
    Ok(())
}

impl MeshFile {
    /// Reads the file, wherever it is.
    pub fn read(&self) -> Result<SurfaceImport, IoError> {
        match &self.source {
            Source::Disk(path) => read_surface(path),
            Source::Zipped { zip, index, extension } => {
                let unreadable = |e: &dyn std::fmt::Display| IoError::Binary(format!("{}: {e}", self.place));
                let mut zip = zip.clone();
                let mut entry = zip.by_index(*index).map_err(|e| unreadable(&e))?;
                let mut bytes = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut bytes).map_err(|e| unreadable(&e))?;
                match extension.as_str() {
                    "dxf" => Ok(SurfaceImport::Triangulation(parse_dxf(&bytes)?)),
                    _ => Ok(SurfaceImport::Vulcan(parse_vulcan_triangulation(&bytes).map_err(|e| unreadable(&e))?)),
                }
            }
        }
    }
}

/// A file name's extension, lower case. Zip entries use `/` and paths may use `\`.
fn extension_of(name: &str) -> String {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// A file's name without its folders or extension.
fn stem_of(name: &str) -> String {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    file.rsplit_once('.').map_or(file, |(stem, _)| stem).to_string()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zd_core::TriMesh;
    use zip::write::{SimpleFileOptions, ZipWriter};

    use super::*;
    use crate::plain_triangulation_bytes;

    fn tetrahedron() -> Vec<u8> {
        let v = vec![[650_000.0, 7_450_000.0, 100.0], [650_010.0, 7_450_000.0, 100.0], [650_000.0, 7_450_010.0, 100.0], [650_000.0, 7_450_000.0, 110.0]];
        plain_triangulation_bytes("Created External", &TriMesh::new(v, vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]]))
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zd-mesh-files-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("seam").join("4R")).unwrap();
        dir
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.add_directory("empty/", SimpleFileOptions::default()).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn a_zip_is_read_at_any_depth_and_other_files_are_left_out() {
        let dir = scratch("zip");
        let body = tetrahedron();
        let path = dir.join("relimits.zip");
        write_zip(&path, &[("top.00T", &body), ("strip/1/deep.00t", &body), ("readme.txt", b"not a mesh")]);
        let selection = find_mesh_files(std::slice::from_ref(&path)).unwrap();
        assert_eq!((selection.zips, selection.folders, selection.left_out), (1, 0, 1));
        let names: Vec<&str> = selection.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["top", "deep"]);
        for file in &selection.files {
            let SurfaceImport::Vulcan(read) = file.read().unwrap() else { panic!("a triangulation") };
            assert_eq!(read.mesh.triangle_count(), 4);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_is_walked_with_its_subfolders_and_any_zip_in_it() {
        let dir = scratch("folder");
        let body = tetrahedron();
        std::fs::write(dir.join("a.00t"), &body).unwrap();
        std::fs::write(dir.join("seam").join("4R").join("b.00t"), &body).unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        write_zip(&dir.join("seam").join("more.zip"), &[("c.00t", &body)]);
        let selection = find_mesh_files(std::slice::from_ref(&dir)).unwrap();
        assert_eq!((selection.zips, selection.folders, selection.left_out), (1, 3, 1));
        let mut names: Vec<&str> = selection.files.iter().map(|f| f.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["a", "b", "c"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_broken_zip_is_refused_and_says_which() {
        let dir = scratch("broken");
        let path = dir.join("broken.zip");
        std::fs::write(&path, b"this is no zip").unwrap();
        let error = find_mesh_files(std::slice::from_ref(&path)).unwrap_err();
        assert!(error.to_string().contains("broken.zip"), "{error}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
