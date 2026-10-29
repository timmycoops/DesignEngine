use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, CrossingCheck, Crossings, ObjectId, ObjectKind, Solid, TriMesh};
use zd_geom::{CloseLog, close_solid, cut_solid_by_surface, repair_mesh, self_union, separate_touching_edges, split_patches};

use std::path::Path;

use zd_io::{SurfaceImport, read_surface};

use super::exchange::describe_dxf_faces;
use super::surface::{MeshParams, check_crossings, default_tolerance, describe, describe_repair, describe_vulcan, mesh_params, mm, repaired, site};
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// `solid.from_mesh`: a solid from vertices and triangles given inline; refused unless watertight
/// and free of crossings (decision 0010).
pub struct SolidFromMesh;

impl Command for SolidFromMesh {
    type Params = MeshParams;
    const NAME: &'static str = "solid.from_mesh";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "New solid from inline vertices and triangles",
            inputs: vec![],
            params: mesh_params("Name of the new solid"),
            outputs: vec![Field::object("solid", ObjectKind::Solid, "The new solid")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let solid = Solid::new(p.mesh()?).map_err(CommandError::NotWatertight)?;
        let (crossings, checked) = check_crossings(solid.mesh(), p.tolerance);
        refuse_crossings("solid", &p.name, &crossings)?;
        let solid = solid.with_crossings(crossings);
        let flags = describe(&solid.validity(), solid.mesh());
        let volume = format!("volume {:.3} m3", solid.volume());
        Ok(Outcome::new(vec![Change::Create {
            name: p.name.clone(),
            layer: p.layer.clone(),
            attributes: Attributes::new(),
            body: Body::Solid(solid),
        }])
        .message(flags)
        .message(checked)
        .message(volume))
    }
}

/// `solid.import`: a solid from a closed triangulation in a Vulcan `.00t` or DXF file, through the
/// repair step and the crossing check (decision 0010). Edges where closed pieces, or two parts of
/// one, touch are taken apart (vertices copied, no coordinate moved), and separate pieces become one
/// solid each (`<name> part 1`, ...). With `repair`, it first closes the mesh (decision 0013): fins
/// off, cracks joined, holes filled up to `fill_limit` of the surface, and a surface that crosses
/// itself reduced to the solid it encloses, each noted on the solid.
/// Refused if any piece is open or the mesh crosses itself; the run record says why.
pub struct ImportSolid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportSolidParams {
    pub path: String,
    pub layer: String,
    /// Defaults to the file name without its extension.
    #[serde(default)]
    pub name: Option<String>,
    /// Vertices closer than this (m) are welded, and crossings by less than it are not counted.
    #[serde(default = "default_tolerance")]
    pub weld_tolerance: f64,
    /// Close the mesh first: fins off, cracks joined, holes filled (decision 0013). Off by default.
    #[serde(default)]
    pub repair: bool,
    /// With `repair`: the most hole area to fill, as a share of the solid's surface (0.1 = 10 %).
    #[serde(default = "default_fill_limit")]
    pub fill_limit: f64,
}

fn default_fill_limit() -> f64 {
    0.10
}

impl Command for ImportSolid {
    type Params = ImportSolidParams;
    const NAME: &'static str = "solid.import";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Import a solid from a Vulcan triangulation or DXF",
            inputs: vec![],
            params: vec![
                Field::new("path", FieldType::Path, ".00t or .dxf holding one closed triangulation"),
                Field::new("layer", FieldType::Layer, "Layer for the solid"),
                Field::new("name", FieldType::Text, "Optional; the file name by default"),
                Field::new("weld_tolerance", FieldType::Number { unit: "m" }, "Weld vertices closer than this; 1 mm by default"),
                Field::new("repair", FieldType::Bool, "Close the mesh first: fins off, cracks joined, holes filled; off by default"),
                Field::new("fill_limit", FieldType::Number { unit: "share" }, "With repair: the most hole area to fill, as a share of the surface; 0.1 by default"),
            ],
            outputs: vec![Field::new("solids", FieldType::Objects, "The imported solid, or one per closed piece")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if !(p.weld_tolerance.is_finite() && p.weld_tolerance >= 0.0) {
            return Err(CommandError::Invalid(format!("tolerance {} must be zero or more", p.weld_tolerance)));
        }
        if !(p.fill_limit.is_finite() && (0.0..=1.0).contains(&p.fill_limit)) {
            return Err(CommandError::Invalid(format!("fill limit {} must be a share between 0 and 1", p.fill_limit)));
        }
        let path = Path::new(&p.path);
        let (read, read_log) = match read_surface(path).map_err(|e| CommandError::Io(format!("{}: {e}", p.path)))? {
            SurfaceImport::Vulcan(vulcan) => {
                let line = describe_vulcan(&vulcan);
                (vulcan.mesh, line)
            }
            SurfaceImport::Triangulation(dxf) => {
                let line = describe_dxf_faces(&dxf);
                (dxf.mesh, line)
            }
            SurfaceImport::Grid(_) | SurfaceImport::VulcanGrid(_) => {
                return Err(CommandError::Invalid(format!("{} is a grid: import it as a surface", p.path)));
            }
        };
        let name = p.name.clone().unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "solid".into()));
        let mut attributes = Attributes::new();
        let (mesh, mut messages, crossings) = if p.repair {
            let (welded, log) = repair_mesh(&read, p.weld_tolerance);
            let (closed, close) = close_solid(&welded, p.weld_tolerance);
            let share = if close.surface_area > 0.0 { close.filled_area() / close.surface_area } else { 0.0 };
            if share > p.fill_limit {
                let (at, area) = close.filled.iter().copied().fold(([0.0; 3], 0.0), |big, f| if f.1 > big.1 { f } else { big });
                return Err(CommandError::Invalid(format!(
                    "'{name}' has holes of {:.1} m² in all, {:.1} % of its surface, over the {:.0} % limit; the largest is {area:.1} m² at {}: fix it at source, or import it with a higher fill_limit",
                    close.filled_area(),
                    100.0 * share,
                    100.0 * p.fill_limit,
                    site(&at)
                )));
            }
            let (mut crossings, mut checked) = check_crossings(&closed, p.weld_tolerance);
            let mut summary = describe_close(&close);
            let mut closed = closed;
            // Crossings: the surface of the solid it encloses (slice B).
            if crossings.pairs > 0 {
                let (union, done) = self_union(&closed, p.weld_tolerance).map_err(|why| {
                    CommandError::Invalid(format!("the solid '{name}' crosses itself ({} pairs of triangles) and the crossings could not be resolved: {why}", crossings.pairs))
                })?;
                let change = done.volume_after - done.volume_before;
                let resolved = format!(
                    "{} crossings resolved ({} triangles cut, {} of {} regions kept; volume {:.3} m3 as read, {:.3} m3 enclosed, {:+.3} m3)",
                    done.pairs, done.triangles_cut, done.regions_kept, done.regions, done.volume_before, done.volume_after, change
                );
                summary = if summary == "nothing to do" { resolved } else { format!("{summary}; {resolved}") };
                attributes.insert("repair_volume_change_m3".into(), AttrValue::Number(change));
                closed = union;
                (crossings, checked) = check_crossings(&closed, p.weld_tolerance);
            }
            if close.fin_triangles + close.cracks_joined + close.filled.len() > 0 || attributes.contains_key("repair_volume_change_m3") {
                attributes.insert("repair".into(), AttrValue::Text(summary.clone()));
                if !close.filled.is_empty() {
                    attributes.insert("repair_filled_m2".into(), AttrValue::Number(close.filled_area()));
                }
            }
            let messages = vec![read_log, describe_repair(&log, &closed), format!("closing: {summary}"), checked];
            (closed, messages, crossings)
        } else {
            repaired(&read, read_log, &p.path, p.weld_tolerance)?
        };
        let validity = mesh.validity();
        // Closed pieces meeting along edges of three or more faces come apart, one solid each.
        let pieces = if validity.is_watertight() {
            vec![mesh]
        } else {
            let (apart, separation) = separate_touching_edges(&mesh);
            let pieces = if validity.boundary_edges == 0 && separation.edges > 0 { split_patches(&apart) } else { Vec::new() };
            if pieces.is_empty() || !pieces.iter().all(|piece| piece.validity().is_watertight()) {
                let hint = match first_open_edge(&mesh) {
                    Some(at) => format!(
                        "; the first open edge is at {}: if it is a surface, import it with surface.import, and if it is a solid, it has gaps to fix at source",
                        site(&at)
                    ),
                    None => String::new(),
                };
                return Err(CommandError::Invalid(format!(
                    "'{name}' is not a closed solid after repair: {} boundary, {} non-manifold and {} inconsistently wound edges{hint} ({})",
                    validity.boundary_edges,
                    validity.non_manifold_edges,
                    validity.inconsistent_edges,
                    messages[1]
                )));
            }
            let outcome = if pieces.len() == 1 { "one piece touching itself, imported whole".to_string() } else { format!("{} closed pieces, each a solid of its own", pieces.len()) };
            messages.push(format!(
                "{} edges where closed pieces touch taken apart ({} vertices copied, no coordinate moved): {outcome}",
                separation.edges, separation.vertex_copies
            ));
            pieces
        };
        // A piece's triangles are among the mesh's, so a mesh with no crossings has pieces with none.
        refuse_crossings("solid", &name, &crossings)?;
        let named = pieces.len() > 1;
        let mut changes = Vec::with_capacity(pieces.len());
        let mut volumes = Vec::with_capacity(pieces.len());
        for (k, piece) in pieces.into_iter().enumerate() {
            let solid = Solid::new(piece).map_err(CommandError::NotWatertight)?.with_crossings(crossings.clone());
            let part = if named { format!("{name} part {}", k + 1) } else { name.clone() };
            messages.push(describe(&solid.validity(), solid.mesh()));
            volumes.push((part.clone(), solid.volume()));
            changes.push(Change::Create { name: part, layer: p.layer.clone(), attributes: attributes.clone(), body: Body::Solid(solid) });
        }
        let mut outcome = Outcome::new(changes);
        outcome.messages = messages;
        let volume = match &volumes[..] {
            [(_, v)] => format!("volume {v:.3} m3"),
            many => format!("volumes: {}", many.iter().map(|(part, v)| format!("{part} {v:.3} m3")).collect::<Vec<_>>().join(", ")),
        };
        Ok(outcome.message(volume))
    }
}

/// What closing did, in one line.
fn describe_close(close: &CloseLog) -> String {
    let mut parts = Vec::new();
    if close.fin_triangles > 0 {
        parts.push(format!("{} fin triangles taken off", close.fin_triangles));
    }
    if close.cracks_joined > 0 {
        parts.push(format!("{} cracks joined", close.cracks_joined));
    }
    if !close.filled.is_empty() {
        let share = if close.surface_area > 0.0 { 100.0 * close.filled_area() / close.surface_area } else { 0.0 };
        parts.push(format!("{} holes filled ({:.1} m², {share:.1} % of the surface)", close.filled.len(), close.filled_area()));
    }
    if close.triangles_turned > 0 && !close.filled.is_empty() {
        parts.push(format!("{} triangles turned to face out once closed", close.triangles_turned));
    }
    if let Some(first) = close.unfillable.first() {
        parts.push(format!("{} holes could not be filled (their edge folds over itself), the first at {}", close.unfillable.len(), site(first)));
    }
    if parts.is_empty() { "nothing to do".to_string() } else { parts.join("; ") }
}

/// The middle of the first edge only one triangle has, in triangle order.
fn first_open_edge(mesh: &TriMesh) -> Option<[f64; 3]> {
    let mut faces: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
    for t in &mesh.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *faces.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    mesh.triangles.iter().flat_map(|t| (0..3).map(move |k| (t[k], t[(k + 1) % 3]))).find(|&(a, b)| faces[&(a.min(b), a.max(b))] == 1).map(|(a, b)| {
        let (p, q) = (mesh.vertices[a as usize], mesh.vertices[b as usize]);
        [0, 1, 2].map(|i| 0.5 * (p[i] + q[i]))
    })
}

/// Refuses a mesh that crosses itself: crossings are fixed at source, never here.
fn refuse_crossings(what: &str, name: &str, crossings: &Crossings) -> Result<(), CommandError> {
    let (Some(first), CrossingCheck::Search { tolerance }) = (crossings.at.first(), crossings.check) else {
        return Ok(());
    };
    Err(CommandError::Invalid(format!(
        "the {what} '{name}' crosses itself: {} pairs of triangles by more than {}, the first at {}; fix it at source",
        crossings.pairs,
        mm(tolerance),
        site(first)
    )))
}

/// A cut's input: its saved crossing check, or a search now if it has none.
fn checked_input(what: &str, name: &str, saved: Option<&Crossings>, mesh: &TriMesh, messages: &mut Vec<String>) -> Result<(), CommandError> {
    let found = match saved {
        Some(crossings) => crossings.clone(),
        None => {
            let (crossings, line) = check_crossings(mesh, default_tolerance());
            messages.push(format!("{what} '{name}' {line}"));
            crossings
        }
    };
    refuse_crossings(what, name, &found)
}

/// `solid.cut_by_surface`: the parts of a solid below and above a design surface, as new solids
/// (engine rule 5). The input solid is left as it is. Either input crossing itself is refused, and
/// one never checked is checked first (decision 0010).
pub struct CutBySurface;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutBySurfaceParams {
    pub solid: ObjectId,
    pub surface: ObjectId,
    /// Layer for both new solids.
    pub layer: String,
    /// Default: "<solid> below <surface>".
    #[serde(default)]
    pub below_name: Option<String>,
    /// Default: "<solid> above <surface>".
    #[serde(default)]
    pub above_name: Option<String>,
}

impl Command for CutBySurface {
    type Params = CutBySurfaceParams;
    const NAME: &'static str = "solid.cut_by_surface";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Cut a solid by a surface into the parts below and above it",
            inputs: vec![
                Field::object("solid", ObjectKind::Solid, "The solid to cut"),
                Field::object("surface", ObjectKind::Surface, "The cutting surface"),
            ],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the new solids"),
                Field::new("below_name", FieldType::Text, "Optional name for the part below"),
                Field::new("above_name", FieldType::Text, "Optional name for the part above"),
            ],
            outputs: vec![
                Field::object("below", ObjectKind::Solid, "The part below the surface (left out if empty)"),
                Field::object("above", ObjectKind::Solid, "The part above the surface (left out if empty)"),
            ],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (solid_object, solid) = inputs.solid(p.solid)?;
        let (surface_object, surface) = inputs.surface(p.surface)?;
        let mut checks = Vec::new();
        checked_input("solid", &solid_object.name, solid.crossings(), solid.mesh(), &mut checks)?;
        checked_input("surface", &surface_object.name, surface.crossings(), surface.mesh(), &mut checks)?;
        let cut = cut_solid_by_surface(solid.mesh(), surface.mesh()).map_err(|e| CommandError::Geometry(e.to_string()))?;

        let t = cut.timings;
        let mut outcome = Outcome { messages: checks, ..Outcome::default() }
            .message(format!(
                "{} + {} triangles in, {} below + {} above out, {:.0} ms (close {:.0}, to Manifold {:.0}, boolean {:.0}, from Manifold {:.0}); cap {:?}",
                solid.mesh().triangle_count(),
                surface.mesh().triangle_count(),
                cut.below.triangle_count(),
                cut.above.triangle_count(),
                t.total_ms(),
                t.close_ms,
                t.to_manifold_ms,
                t.boolean_ms,
                t.from_manifold_ms,
                cut.cap_method
            ))
            .message(format!(
                "volumes: input {:.3} m3, below {:.3} m3, above {:.3} m3",
                cut.input_volume, cut.below_volume, cut.above_volume
            ));
        if !cut.design_covers_solid {
            outcome = outcome.message("warning: the surface does not cover the solid in plan; the uncovered part is counted as above");
        }

        let parts = [
            (cut.below, p.below_name.clone(), "below"),
            (cut.above, p.above_name.clone(), "above"),
        ];
        for (mesh, name, side) in parts {
            if mesh.triangles.is_empty() {
                outcome = outcome.message(format!("nothing of the solid lies {side} the surface"));
                continue;
            }
            let part = Solid::new(mesh).map_err(CommandError::NotWatertight)?;
            let name = name.unwrap_or_else(|| format!("{} {side} {}", solid_object.name, surface_object.name));
            outcome.changes.push(Change::Create { name, layer: p.layer.clone(), attributes: Attributes::new(), body: Body::Solid(part) });
        }
        Ok(outcome)
    }
}
