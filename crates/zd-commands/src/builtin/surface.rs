use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, Crossings, MeshValidity, ObjectKind, Surface, TriMesh};
use zd_geom::{RepairLog, find_crossings, find_solid_crossings, repair_mesh, triangulate_grid};
use zd_io::{SurfaceImport, VulcanTriangulation, read_surface};

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

/// `surface.import`: a surface from the 3DFACEs and polyface meshes of a DXF file, a Vulcan
/// triangulation (`.00t`), a Vulcan elevation grid (`.srg`, `.sfg`, `.tpg`) or an XYZ/CSV grid (grids
/// are triangulated, engine rule 6). A DXF or
/// Vulcan mesh passes through the repair step and the crossing check (decision 0010); a grid needs
/// neither. Every change goes into the run record.
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
            summary: "Import a surface from DXF, a Vulcan triangulation or grid, or an XYZ/CSV grid",
            inputs: vec![],
            params: vec![
                Field::new("path", FieldType::Path, ".dxf, .00t, .srg, .sfg, .tpg, .xyz, .csv, .txt or .asc"),
                Field::new("layer", FieldType::Layer, "Layer for the surface"),
                Field::new("name", FieldType::Text, "Optional; the file name by default"),
                Field::new("weld_tolerance", FieldType::Number { unit: "m" }, "DXF and Vulcan meshes: weld vertices closer than this; 1 mm by default"),
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
                let (mesh, messages, crossings) = repaired(&dxf.mesh, read, &p.path, p.weld_tolerance, check_crossings)?;
                (Surface::new(mesh).with_crossings(crossings), messages)
            }
            SurfaceImport::Vulcan(vulcan) => {
                let read = describe_vulcan(&vulcan);
                let (mesh, messages, crossings) = repaired(&vulcan.mesh, read, &p.path, p.weld_tolerance, check_crossings)?;
                (Surface::new(mesh).with_crossings(crossings), messages)
            }
            SurfaceImport::VulcanGrid(vulcan) => {
                let (g, log) = (&vulcan.grid, &vulcan.log);
                let read = format!(
                    "Vulcan grid {} x {} nodes at {} x {} m: {} live, {} null{}",
                    g.cols,
                    g.rows,
                    g.cell_size[0],
                    g.cell_size[1],
                    log.live,
                    log.nulls,
                    if log.compressed_blocks > 0 { format!(" (compressed, {} blocks)", log.compressed_blocks) } else { String::new() }
                );
                let mesh = triangulate_grid(g);
                if mesh.triangles.is_empty() {
                    return Err(CommandError::Invalid(format!("{} holds no live cells", p.path)));
                }
                let lattice = "a grid has nothing to weld or orient, and cannot cross itself: one elevation per node on a regular plan lattice";
                (Surface::new(mesh).with_crossings(Crossings::lattice()), vec![read, lattice.to_string()])
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

/// A mesh read from a file, through the repair step and the crossing check (`check`: by the surface
/// or the solid rules): the mesh, the run record's lines (what the reader did, the repair, the
/// check) and the check's result.
pub(crate) fn repaired(read: &TriMesh, read_log: String, path: &str, tolerance: f64, check: CrossingCheckFn) -> Result<(TriMesh, Vec<String>, Crossings), CommandError> {
    let (mesh, log) = repair_mesh(read, tolerance);
    if mesh.triangles.is_empty() {
        return Err(CommandError::Invalid(format!("{path} holds no triangles ({read_log})")));
    }
    let repair = describe_repair(&log, &mesh);
    let (crossings, checked) = check(&mesh, tolerance);
    Ok((mesh, vec![read_log, repair, checked], crossings))
}

/// What the Vulcan reader found, in one line.
pub(crate) fn describe_vulcan(vulcan: &VulcanTriangulation) -> String {
    let log = &vulcan.log;
    let mut line = format!("Vulcan triangulation: {} vertices, {} triangles", log.vertices, log.triangles);
    if log.compressed_blocks > 0 {
        line += &format!(" (compressed, {} blocks)", log.compressed_blocks);
    }
    if log.triangle_extras_set > 0 {
        line += &format!("; {} triangles carry extra words, ignored", log.triangle_extras_set);
    }
    if log.property_bytes > 0 {
        line += &format!("; {} bytes of display properties not read", log.property_bytes);
    }
    line
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

/// A crossing check with its result in one line for the run record.
pub(crate) type CrossingCheckFn = fn(&TriMesh, f64) -> (Crossings, String);

/// Searches a mesh for crossings by the surface rules, with the result in one line for the run
/// record.
pub(crate) fn check_crossings(mesh: &TriMesh, tolerance: f64) -> (Crossings, String) {
    timed_check(mesh, tolerance, find_crossings)
}

/// The same by the solid rules (decision 0014): thin pairs, a knife edge or a hairline gap, are
/// counted and kept as they are, never refused.
pub(crate) fn check_solid_crossings(mesh: &TriMesh, tolerance: f64) -> (Crossings, String) {
    timed_check(mesh, tolerance, find_solid_crossings)
}

fn timed_check(mesh: &TriMesh, tolerance: f64, search: fn(&TriMesh, f64) -> Crossings) -> (Crossings, String) {
    let start = Instant::now();
    let crossings = search(mesh, tolerance);
    let ms = start.elapsed().as_secs_f64() * 1e3;
    let thin = match crossings.thin_at.first() {
        None => String::new(),
        Some(first) => format!(
            "; {} thin pairs, faces within {} of each other that do not cross (knife edges, hairline gaps or thin lips), the first at {}: kept as they are",
            crossings.thin,
            mm(tolerance),
            site(first)
        ),
    };
    let line = match crossings.at.first() {
        None => format!("crossings: none by more than {}{thin} ({} triangles, {ms:.0} ms)", mm(tolerance), mesh.triangle_count()),
        Some(first) => format!(
            "crossings: {} pairs of triangles cross by more than {}, the first at {}; cuts refuse this mesh until it is fixed at source{thin} ({ms:.0} ms)",
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
