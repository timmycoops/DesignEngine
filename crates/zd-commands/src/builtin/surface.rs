use std::path::Path;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, MeshValidity, ObjectKind, Surface, TriMesh};
use zd_geom::triangulate_grid;
use zd_io::{SurfaceImport, read_surface};

use super::finite;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// `surface.import`: a surface from a DXF 3DFACE file or an XYZ/CSV grid (grids are triangulated,
/// engine rule 6). The reader's log of what it cleaned up goes into the run record.
pub struct ImportSurface;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportSurfaceParams {
    pub path: String,
    pub layer: String,
    /// Defaults to the file name without its extension.
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for ImportSurface {
    type Params = ImportSurfaceParams;
    const NAME: &'static str = "surface.import";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Import a surface from DXF or an XYZ/CSV grid",
            inputs: vec![],
            params: vec![
                Field::new("path", FieldType::Path, ".dxf, .xyz, .csv, .txt or .asc"),
                Field::new("layer", FieldType::Layer, "Layer for the surface"),
                Field::new("name", FieldType::Text, "Optional; the file name by default"),
            ],
            outputs: vec![Field::object("surface", ObjectKind::Surface, "The imported surface")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let path = Path::new(&p.path);
        let (mesh, read_log) = match read_surface(path).map_err(|e| CommandError::Io(format!("{}: {e}", p.path)))? {
            SurfaceImport::Triangulation(dxf) => {
                let log = &dxf.log;
                let message = format!(
                    "DXF: {} faces read, {} quads split, {} degenerate and {} duplicate faces dropped",
                    log.faces_read, log.quads_split, log.degenerate_dropped, log.duplicates_dropped
                );
                (dxf.mesh, message)
            }
            SurfaceImport::Grid(xyz) => {
                let (g, log) = (&xyz.grid, &xyz.log);
                let message = format!(
                    "XYZ grid {} x {} nodes at {} x {} m: {} points, {} header lines, {} null points, {} duplicate points",
                    g.cols, g.rows, g.cell_size[0], g.cell_size[1], log.points_read, log.header_lines, log.null_points,
                    log.duplicate_points
                );
                (triangulate_grid(g), message)
            }
        };
        if mesh.triangles.is_empty() {
            return Err(CommandError::Invalid(format!("{} holds no triangles", p.path)));
        }
        let name = p.name.clone().unwrap_or_else(|| {
            path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "surface".into())
        });
        let surface = Surface::new(mesh);
        let flags = describe(&surface.validity(), surface.mesh());
        Ok(Outcome::new(vec![Change::Create {
            name,
            layer: p.layer.clone(),
            attributes: Attributes::new(),
            body: Body::Surface(surface),
        }])
        .message(read_log)
        .message(flags))
    }
}

/// A mesh given inline: for scripts, tests and small objects. Large meshes come from files.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshParams {
    pub name: String,
    pub layer: String,
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl MeshParams {
    /// The mesh, if every index is in range and every coordinate finite.
    pub(crate) fn mesh(&self) -> Result<TriMesh, CommandError> {
        if self.triangles.is_empty() {
            return Err(CommandError::Invalid("a mesh needs at least one triangle".into()));
        }
        self.vertices.iter().try_for_each(finite)?;
        let n = self.vertices.len();
        if let Some(t) = self.triangles.iter().find(|t| t.iter().any(|&i| i as usize >= n)) {
            return Err(CommandError::Invalid(format!("triangle {t:?} refers past the {n} vertices")));
        }
        Ok(TriMesh::new(self.vertices.clone(), self.triangles.clone()))
    }
}

pub(crate) fn mesh_params(kind: &'static str) -> Vec<Field> {
    vec![
        Field::new("name", FieldType::Text, kind),
        Field::new("layer", FieldType::Layer, "Layer for the new object"),
        Field::new("vertices", FieldType::Points, "Vertex coordinates"),
        Field::new("triangles", FieldType::Text, "Vertex index triples, counter-clockwise about the outward normal"),
    ]
}

/// `surface.from_mesh`: a surface from vertices and triangles given inline.
pub struct SurfaceFromMesh;

impl Command for SurfaceFromMesh {
    type Params = MeshParams;
    const NAME: &'static str = "surface.from_mesh";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "New surface from inline vertices and triangles",
            inputs: vec![],
            params: mesh_params("Name of the new surface"),
            outputs: vec![Field::object("surface", ObjectKind::Surface, "The new surface")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let surface = Surface::new(p.mesh()?);
        let flags = describe(&surface.validity(), surface.mesh());
        Ok(Outcome::new(vec![Change::Create {
            name: p.name.clone(),
            layer: p.layer.clone(),
            attributes: Attributes::new(),
            body: Body::Surface(surface),
        }])
        .message(flags))
    }
}

/// The validity flags in one line, for the run record.
pub(crate) fn describe(v: &MeshValidity, mesh: &TriMesh) -> String {
    format!(
        "{} triangles, {} vertices; {} boundary, {} non-manifold, {} inconsistently wound edges; {} degenerate triangles",
        mesh.triangle_count(),
        mesh.vertices.len(),
        v.boundary_edges,
        v.non_manifold_edges,
        v.inconsistent_edges,
        v.degenerate_triangles
    )
}
