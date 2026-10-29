//! Finds real pit data under `testdata/golden/<pit>/`: either files laid out by name, or a `pit.json`
//! naming the files where they are (see that folder's README).

use std::path::{Path, PathBuf};

use zd_core::TriMesh;
use zd_geom::triangulate_grid;
use zd_io::{SurfaceImport, read_surface};

pub struct GoldenSeam {
    pub name: String,
    pub roof: PathBuf,
    pub floor: PathBuf,
}

pub struct GoldenPit {
    pub name: String,
    pub dir: PathBuf,
    pub topo: Option<PathBuf>,
    /// `design.*` if present, else the base of weathering, else the topography.
    pub design: Option<PathBuf>,
    pub seams: Vec<GoldenSeam>,
}

const SURFACE_EXTENSIONS: [&str; 5] = ["dxf", "xyz", "csv", "txt", "asc"];

pub fn golden_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("testdata").join("golden")
}

/// Every pit folder with at least one roof/floor pair.
pub fn discover_pits() -> Vec<GoldenPit> {
    let Ok(entries) = std::fs::read_dir(golden_root()) else { return Vec::new() };
    let mut pits: Vec<GoldenPit> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| pit_in(&e.path()))
        .collect();
    pits.sort_by(|a, b| a.name.cmp(&b.name));
    pits
}

fn pit_in(dir: &Path) -> Option<GoldenPit> {
    if dir.join("pit.json").exists() {
        return pit_from_json(dir);
    }
    let files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| SURFACE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .collect();
    let stem = |p: &PathBuf| p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    let find = |name: &str| files.iter().find(|p| stem(p) == name).cloned();

    let mut seams = Vec::new();
    for roof in files.iter().filter(|p| stem(p).ends_with("_roof")) {
        let seam = stem(roof).trim_end_matches("_roof").to_string();
        if let Some(floor) = find(&format!("{seam}_floor")) {
            seams.push(GoldenSeam { name: seam, roof: roof.clone(), floor });
        }
    }
    if seams.is_empty() {
        return None;
    }
    seams.sort_by(|a, b| a.name.cmp(&b.name));
    let topo = find("topo");
    Some(GoldenPit {
        name: dir.file_name()?.to_string_lossy().into_owned(),
        dir: dir.to_path_buf(),
        design: find("design").or_else(|| find("bow")).or_else(|| topo.clone()),
        topo,
        seams,
    })
}

/// A pit described by `pit.json`: its topography, an optional design surface to cut with, and its
/// seams' roof and floor files, all named where they are.
fn pit_from_json(dir: &Path) -> Option<GoldenPit> {
    let file = dir.join("pit.json");
    let pit: serde_json::Value = match std::fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())) {
        Ok(pit) => pit,
        Err(e) => {
            eprintln!("{}: {e}", file.display());
            return None;
        }
    };
    let path = |key: &str| pit.get(key).and_then(|p| p.as_str()).map(PathBuf::from);
    let seams: Vec<GoldenSeam> = pit
        .get("seams")?
        .as_array()?
        .iter()
        .filter_map(|s| Some(GoldenSeam { name: s.get("name")?.as_str()?.to_string(), roof: s.get("roof")?.as_str()?.into(), floor: s.get("floor")?.as_str()?.into() }))
        .collect();
    if seams.is_empty() {
        return None;
    }
    let topo = path("topography");
    Some(GoldenPit {
        name: pit.get("name").and_then(|n| n.as_str()).map(str::to_string).or_else(|| Some(dir.file_name()?.to_string_lossy().into_owned()))?,
        dir: dir.to_path_buf(),
        design: path("design").or_else(|| topo.clone()),
        topo,
        seams,
    })
}

/// Reads any supported surface file as a triangulation (grids are triangulated).
pub fn load_triangulation(path: &Path) -> Result<TriMesh, String> {
    match read_surface(path).map_err(|e| format!("{}: {e}", path.display()))? {
        SurfaceImport::Triangulation(import) => Ok(import.mesh),
        SurfaceImport::Vulcan(import) => Ok(import.mesh),
        SurfaceImport::VulcanGrid(import) => Ok(triangulate_grid(&import.grid)),
        SurfaceImport::Grid(import) => Ok(triangulate_grid(&import.grid)),
    }
}
