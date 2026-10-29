use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, Crossings, MeshValidity, ObjectKind, Surface, TriMesh};
use zd_geom::{RepairLog, find_crossings, repair_mesh, triangulate_grid};
use zd_io::{SurfaceImport, read_surface};

use super::exchange::describe_dxf_faces;
use super::finite;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// The weld and crossing tolerance when none is given: 1 mm (decision 0010).
pub(crate) fn default_tolerance() -> f64 {
    0.001
}

fn check_tolerance(tolerance: f64) -> Result<(), CommandError> {
    if tolerance.is_finite() && tolerance >= 0.0 {
        Ok(())
    } else {
        Err(CommandError::Invalid(format!("tolerance {tolerance} must be zero or more")))
    }
}

/// `surface.import`: a surface from the 3DFACEs and polyface meshes of a DXF file, or an XYZ/CSV
/// grid (grids are triangulated, engine rule 6). A DXF mesh passes through the repair step and the
/// crossing check (decision 0010); a grid needs neither. Every change goes into the run record.
pub struct ImportSurface;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportSurfaceParams {
    pub path: String,
    pub layer: String,
    /// Defaults to the file name without its extension.
    #[serde(default)]
    pub name: Option<String>,
    /// Vertices closer than this (m) are welded, and crossings by less than it are not counted.
    #[serde(default = "default_tolerance")]
    pub weld_tolerance: f64,
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
                Field::new("weld_tolerance", FieldType::Number { unit: "m" }, "DXF meshes: weld vertices closer than this; 1 mm by default"),
            ],
            outputs: vec![Field::object("surface", ObjectKind::Surface, "The imported surface")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        check_tolerance(p.weld_tolerance)?;
        let path = Path::new(&p.path);
        let (surface, messages) = match read_surface(path).map_err(|e| CommandError::Io(format!("{}: {e}", p.path)))? {
            SurfaceImport::Triangulation(dxf) => {
                let read = describe_dxf_faces(&dxf);
                let (mesh, log) = repair_mesh(&dxf.mesh, p.weld_tolerance);
                if mesh.triangles.is_empty() {
                    return Err(CommandError::Invalid(format!("{} holds no triangles ({read})", p.path)));
                }
                let repaired = describe_repair(&log, &mesh);
                let (crossings, checked) = check_crossings(&mesh, p.weld_tolerance);
                (Surface::new(mesh).with_crossings(crossings), vec![read, repaired, checked])
            }
            SurfaceImport::Grid(xyz) => {
                let (g, log) = (&xyz.grid, &xyz.log);
                let read = format!(
                    "XYZ grid {} x {} nodes at {} x {} m: {} points, {} header lines, {} null points, {} duplicate points",
                    g.cols, g.rows, g.cell_size[0], g.cell_size[1], log.points_read, log.header_lines, log.null_points,
                    log.duplicate_points
                );
                let mesh = triangulate_grid(g);
                if mesh.triangles.is_empty() {
                    return Err(CommandError::Invalid(format!("{} holds no triangles", p.path)));
                }
                let lattice = "a grid has nothing to weld or orient, and cannot cross itself: one elevation per node on a regular plan lattice";
                (Surface::new(mesh).with_crossings(Crossings::lattice()), vec![read, lattice.to_string()])
            }
        };
        let name = p.name.clone().unwrap_or_else(|| {
            path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "surface".into())
        });
        let flags = describe(&surface.validity(), surface.mesh());
        let mut outcome = Outcome::new(vec![Change::Create {
            name,
            layer: p.layer.clone(),
            attributes: Attributes::new(),
            body: Body::Surface(surface),
        }]);
        outcome.messages = messages;
        Ok(outcome.message(flags))
    }
}

/// A length in millimetres, to the micrometre.
pub(crate) fn mm(metres: f64) -> String {
    format!("{} mm", (metres * 1e6).round() / 1e3)
}

/// A point as easting, northing and RL.
pub(crate) fn site(p: &[f64; 3]) -> String {
    format!("{:.3} {:.3} RL {:.3}", p[0], p[1], p[2])
}

/// The repair step's log in one line, naming only what it changed or found.
pub(crate) fn describe_repair(log: &RepairLog, mesh: &TriMesh) -> String {
    let mut parts = vec![format!("repair at {}", mm(log.tolerance))];
    if log.vertices_welded > 0 {
        parts.push(format!("{} vertices welded (largest move {})", log.vertices_welded, mm(log.largest_move)));
    }
    let dropped: Vec<String> = [(log.collapsed_dropped, "collapsed by the weld"), (log.degenerate_dropped, "of zero area"), (log.duplicates_dropped, "duplicated")]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, why)| format!("{n} {why}"))
        .collect();
    if !dropped.is_empty() {
        parts.push(format!("triangles dropped: {}", dropped.join(", ")));
    }
    if log.unused_dropped > 0 {
        parts.push(format!("{} vertices no triangle uses dropped", log.unused_dropped));
    }
    let o = &log.orient;
    parts.push(format!(
        "{} triangles flipped to agree with their neighbours and face up, or out where closed ({} patches, {} closed)",
        o.triangles_flipped, o.patches, o.closed_patches
    ));
    if o.non_manifold_edges > 0 {
        parts.push(format!("{} edges with three or more faces, the first at {}", o.non_manifold_edges, site(&o.non_manifold_at[0])));
    }
    if o.non_orientable_patches > 0 {
        parts.push(format!("{} one-sided patches left as read", o.non_orientable_patches));
    }
    parts.push(format!("{} triangles out, {:.0} ms", mesh.triangle_count(), log.ms));
    parts.join("; ")
}

/// Searches a mesh for crossings, with the result in one line for the run record.
pub(crate) fn check_crossings(mesh: &TriMesh, tolerance: f64) -> (Crossings, String) {
    let start = Instant::now();
    let crossings = find_crossings(mesh, tolerance);
    let ms = start.elapsed().as_secs_f64() * 1e3;
    let line = match crossings.at.first() {
        None => format!("crossings: none by more than {} ({} triangles, {ms:.0} ms)", mm(tolerance), mesh.triangle_count()),
        Some(first) => format!(
            "crossings: {} pairs of triangles cross by more than {}, the first at {}; cuts refuse this mesh until it is fixed at source ({ms:.0} ms)",
            crossings.pairs,
            mm(tolerance),
            site(first)
        ),
    };
    (crossings, line)
}

/// A mesh given inline: for scripts, tests and small objects. Large meshes come from files. It is
/// checked, never changed, so a script gets back exactly what it sent.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshParams {
    pub name: String,
    pub layer: String,
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
    /// Crossings by less than this (m) are not counted.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
}

impl MeshParams {
    /// The mesh, if every index is in range and every coordinate finite.
    pub(crate) fn mesh(&self) -> Result<TriMesh, CommandError> {
        if self.triangles.is_empty() {
            return Err(CommandError::Invalid("a mesh needs at least one triangle".into()));
        }
        check_tolerance(self.tolerance)?;
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
        Field::new("tolerance", FieldType::Number { unit: "m" }, "Crossings by less than this are not counted; 1 mm by default"),
    ]
}

/// `surface.from_mesh`: a surface from vertices and triangles given inline, checked for crossings.
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
        let mesh = p.mesh()?;
        let (crossings, checked) = check_crossings(&mesh, p.tolerance);
        let surface = Surface::new(mesh).with_crossings(crossings);
        let flags = describe(&surface.validity(), surface.mesh());
        Ok(Outcome::new(vec![Change::Create {
            name: p.name.clone(),
            layer: p.layer.clone(),
            attributes: Attributes::new(),
            body: Body::Surface(surface),
        }])
        .message(flags)
        .message(checked))
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
