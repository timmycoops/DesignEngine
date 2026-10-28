//! Import and export for the Zovlyn Pit Design Engine.
//!
//! Phase 0 reads triangulations from DXF `3DFACE` entities and regular grids from XYZ/CSV text.
//! Coordinates stay f64 from file to engine. Every clean-up done on the way in (welding,
//! dropped faces, nulls) is counted in the import log; nothing is fixed silently.

mod dxf;
mod error;
mod xyz;

use std::path::Path;

pub use dxf::{DxfImport, DxfLog, parse_dxf_3dfaces, read_dxf_3dfaces, write_dxf_3dfaces};
pub use error::IoError;
pub use xyz::{GridImport, XyzLog, XyzOptions, parse_xyz_grid, read_xyz_grid, write_xyz_grid};

/// What a surface file turned out to hold.
#[derive(Debug, Clone)]
pub enum SurfaceImport {
    Triangulation(DxfImport),
    Grid(GridImport),
}

/// Reads a surface by file extension: `.dxf` as 3DFACE triangulation; `.xyz`, `.csv`, `.txt`
/// and `.asc` as a regular grid.
pub fn read_surface(path: &Path) -> Result<SurfaceImport, IoError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "dxf" => Ok(SurfaceImport::Triangulation(read_dxf_3dfaces(path)?)),
        "xyz" | "csv" | "txt" | "asc" => Ok(SurfaceImport::Grid(read_xyz_grid(path, &XyzOptions::default())?)),
        _ => Err(IoError::Unsupported(format!("no surface reader for '.{ext}' files"))),
    }
}
