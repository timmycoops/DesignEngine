//! Import and export for the Zovlyn Pit Design Engine.
//!
//! Reads triangulations and strings from DXF, Vulcan triangulations (one by one, or by the thousand
//! from folders and zips), strings from CSV, regular grids from XYZ/CSV text and Vulcan grids, and
//! writes DXF R12 and CSV strings. Saves and opens projects
//! as `<name>.zdproj/` folders of Parquet files. Coordinates stay f64 from file to engine and back
//! out. Every clean-up or conversion done on the way in (welding, dropped faces, arcs to chords,
//! nulls) is counted in the import log; nothing is fixed silently.

mod dxf;
mod error;
mod mesh_files;
mod seam;
mod store;
mod string_csv;
mod text;
mod vulcan;
mod vulcan_tri;
mod xyz;

use std::path::Path;

pub use dxf::{ARC_SAGITTA, DxfDrawing, DxfEntity, DxfLog, DxfString, format_dxf, parse_dxf, read_dxf, write_dxf, write_dxf_3dfaces};
pub use error::IoError;
pub use mesh_files::{MeshFile, MeshSelection, find_mesh_files};
pub use seam::{
    DefinedHorizon, LinkedGrid, SeamDefinition, StackGrids, fingerprint_file, format_seam_definition, parse_seam_definition,
    read_linked_grid, read_seam_definition, read_stack_grids, write_seam_definition,
};
pub use store::{Opened, ProjectStore, SaveReport, StoreError};
pub use string_csv::{CsvLog, CsvString, CsvStrings, format_csv_strings, parse_csv_strings, read_csv_strings, write_csv_strings};
pub use vulcan::{VulcanGrid, VulcanLog, VulcanSeam, parse_vulcan_grid, read_vulcan_grid, read_vulcan_seam, vulcan_grid_name};
pub use vulcan_tri::{VulcanTriLog, VulcanTriangulation, parse_vulcan_triangulation, plain_triangulation_bytes, read_vulcan_triangulation};
pub use xyz::{GridImport, XyzLog, XyzOptions, parse_xyz_grid, read_xyz_grid, write_xyz_grid};

/// What a surface file turned out to hold.
#[derive(Debug, Clone)]
pub enum SurfaceImport {
    Triangulation(DxfDrawing),
    Vulcan(VulcanTriangulation),
    /// A Vulcan elevation grid (roof, floor or topography).
    VulcanGrid(VulcanGrid),
    Grid(GridImport),
}

/// Reads a surface by file extension: `.dxf` as the triangulation of its 3DFACEs and polyface
/// meshes; `.00t` as a Vulcan triangulation; `.srg`, `.sfg` and `.tpg` as a Vulcan elevation grid (roof,
/// floor, topography); `.xyz`, `.csv`, `.txt` and `.asc` as a regular grid.
pub fn read_surface(path: &Path) -> Result<SurfaceImport, IoError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "dxf" => Ok(SurfaceImport::Triangulation(read_dxf(path)?)),
        "00t" => Ok(SurfaceImport::Vulcan(read_vulcan_triangulation(path)?)),
        "srg" | "sfg" | "tpg" => Ok(SurfaceImport::VulcanGrid(read_vulcan_grid(path)?)),
        "xyz" | "csv" | "txt" | "asc" => Ok(SurfaceImport::Grid(read_xyz_grid(path, &XyzOptions::default())?)),
        _ => Err(IoError::Unsupported(format!("no surface reader for '.{ext}' files"))),
    }
}
