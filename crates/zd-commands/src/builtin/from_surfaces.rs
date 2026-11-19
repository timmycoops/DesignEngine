//! Strings from surfaces (String tools plan, workstream 5; decision 0083): `string.where_surfaces_meet`,
//! `string.contours`, `string.outline` and `string.from_section`. Each makes new strings on a layer and leaves what it
//! read as it was; a line is open or closed as it runs, and pieces whose ends meet are one string.

use std::borrow::Cow;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, Object, ObjectId, ObjectKind, Polyline, TriMesh};
use zd_geom::{SectionPlane, SliceLine, contour_levels, contours, slice_mesh, solid_outline, surface_outline, triangulate_grid, where_surfaces_meet};

use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

/// A surface's or grid's triangles, a grid's made as every projection makes them (engine rule 6); a solid's as it is.
fn mesh_of<'a>(object: &'a Object, solids_too: bool) -> Result<Cow<'a, TriMesh>, CommandError> {
    match &object.body {
        Body::Surface(s) => Ok(Cow::Borrowed(s.mesh())),
        Body::Grid(g) => Ok(Cow::Owned(triangulate_grid(g))),
        Body::Solid(s) if solids_too => Ok(Cow::Borrowed(s.mesh())),
        other => Err(CommandError::WrongKind { id: object.id, expected: ObjectKind::Surface, found: other.kind() }),
    }
}

fn string_of(line: &SliceLine) -> Polyline {
    Polyline { vertices: line.points.clone(), closed: line.closed }
}

/// `name`, then `name 2`, `name 3` and on for the strings after the first.
fn numbered(name: &str, k: usize) -> String {
    if k == 0 { name.to_string() } else { format!("{name} {}", k + 1) }
}

fn created(name: String, layer: &str, attributes: Attributes, line: &SliceLine) -> Change {
    Change::Create { name, layer: layer.to_string(), attributes, body: Body::String(string_of(line)) }
}

fn points_in(lines: &[SliceLine]) -> usize {
    lines.iter().map(|l| l.points.len()).sum()
}

fn rl_words(rl: f64) -> String {
    let text = format!("{rl:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `string.where_surfaces_meet`: the lines where two surfaces or grids meet, such as a design's crest on the
/// topography, as strings at their own RLs.
pub struct WhereSurfacesMeet;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WhereSurfacesMeetParams {
    /// A surface or elevation grid.
    pub surface: ObjectId,
    /// The surface or grid it meets.
    pub with: ObjectId,
    pub layer: String,
    /// The strings' name, numbered after the first; "<surface> meets <with>" by default.
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for WhereSurfacesMeet {
    type Params = WhereSurfacesMeetParams;
    const NAME: &'static str = "string.where_surfaces_meet";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Make strings where two surfaces or grids meet, such as a design's crest on the topography",
            inputs: vec![
                Field::object("surface", ObjectKind::Surface, "A surface or elevation grid"),
                Field::object("with", ObjectKind::Surface, "The surface or grid it meets"),
            ],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the strings"),
                Field::new("name", FieldType::Text, "The strings' name, numbered after the first; '<surface> meets <with>' by default"),
            ],
            outputs: vec![Field::object("strings", ObjectKind::String, "A string for each line where they meet, at its own RLs")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.surface == p.with {
            return Err(CommandError::Invalid("a surface meets itself everywhere: pick two different surfaces".into()));
        }
        let (a, b) = (inputs.object(p.surface)?, inputs.object(p.with)?);
        let (mesh_a, mesh_b) = (mesh_of(a, false)?, mesh_of(b, false)?);
        let started = Instant::now();
        let met = where_surfaces_meet(&mesh_a, &mesh_b);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        if met.lines.is_empty() {
            return Err(CommandError::Invalid(format!("{} and {} don't meet anywhere", a.name, b.name)));
        }
        let name = p.name.clone().unwrap_or_else(|| format!("{} meets {}", a.name, b.name));
        let changes = met.lines.iter().enumerate().map(|(k, line)| created(numbered(&name, k), &p.layer, Attributes::new(), line)).collect();
        let closed = met.lines.iter().filter(|l| l.closed).count();
        let mut outcome = Outcome::new(changes).message(format!(
            "{} strings where {} meets {} ({closed} closed, {} vertices), from {} and {} triangles in {ms:.0} ms",
            met.lines.len(),
            a.name,
            b.name,
            points_in(&met.lines),
            mesh_a.triangles.len(),
            mesh_b.triangles.len()
        ));
        if met.coplanar > 0 {
            let text = format!("{} pairs of triangles lie flat on each other where {} meets {}: no line is drawn there", met.coplanar, a.name, b.name);
            outcome.notes.push(Note::warning(Code::StringSurfacesFlat, text).on(a.id).measured(met.coplanar as f64, "pairs"));
        }
        Ok(outcome)
    }
}

/// `string.contours`: strings on a surface or grid every so many metres between two RLs, each carrying its RL.
pub struct Contours;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContoursParams {
    pub surface: ObjectId,
    /// The spacing (m).
    pub every: f64,
    /// The lowest RL; the lowest on the spacing the surface reaches if not given.
    #[serde(default)]
    pub from: Option<f64>,
    /// The highest RL; the highest on the spacing the surface reaches if not given.
    #[serde(default)]
    pub to: Option<f64>,
    pub layer: String,
}

impl Command for Contours {
    type Params = ContoursParams;
    const NAME: &'static str = "string.contours";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Make contour strings on a surface or grid every so many metres between two RLs, each carrying its RL",
            inputs: vec![Field::object("surface", ObjectKind::Surface, "A surface or elevation grid")],
            params: vec![
                Field::new("every", FieldType::Number { unit: "m" }, "The spacing"),
                Field::new("from", FieldType::Number { unit: "m" }, "The lowest RL; the surface's lowest on the spacing if left out"),
                Field::new("to", FieldType::Number { unit: "m" }, "The highest RL; the surface's highest on the spacing if left out"),
                Field::new("layer", FieldType::Layer, "Layer for the contours"),
            ],
            outputs: vec![Field::object("contours", ObjectKind::String, "A string for each line at each RL, named for its RL, with an rl attribute")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if !(p.every.is_finite() && p.every > 0.0) {
            return Err(CommandError::Invalid(format!("contours every {} m isn't a spacing", p.every)));
        }
        let surface = inputs.object(p.surface)?;
        let mesh = mesh_of(surface, false)?;
        let bounds = mesh.bounds().ok_or_else(|| CommandError::Invalid(format!("{} has no triangles", surface.name)))?;
        // On the spacing, counted from RL 0, so contours from two surfaces line up.
        let from = p.from.unwrap_or_else(|| (bounds.min[2] / p.every).ceil() * p.every);
        let to = p.to.unwrap_or_else(|| (bounds.max[2] / p.every).floor() * p.every);
        let levels = contour_levels(from, to, p.every).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let started = Instant::now();
        let made = contours(&mesh, from, to, p.every).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let mut changes = Vec::new();
        for contour in &made {
            let name = format!("Contour {}", rl_words(contour.rl));
            for (k, line) in contour.lines.iter().enumerate() {
                let mut attributes = Attributes::new();
                attributes.insert("rl".into(), AttrValue::Number(contour.rl));
                changes.push(created(numbered(&name, k), &p.layer, attributes, line));
            }
        }
        if changes.is_empty() {
            return Err(CommandError::Invalid(format!(
                "{} doesn't reach any RL from {} to {} every {} m: it runs from RL {} to {}",
                surface.name,
                rl_words(from),
                rl_words(to),
                rl_words(p.every),
                rl_words(bounds.min[2]),
                rl_words(bounds.max[2])
            )));
        }
        let with_lines = made.iter().filter(|c| !c.lines.is_empty()).count();
        Ok(Outcome::new(changes.clone()).message(format!(
            "{} contour strings on {}, RL {} to {} every {} m ({with_lines} of {} RLs with a line, {} vertices), from {} triangles in {ms:.0} ms",
            changes.len(),
            surface.name,
            rl_words(from),
            rl_words(to),
            rl_words(p.every),
            levels.len(),
            made.iter().map(|c| points_in(&c.lines)).sum::<usize>(),
            mesh.triangles.len()
        )))
    }
}

/// `string.outline`: each surface's or grid's edge, every hole its own string, at its own RLs; each solid's outline in
/// plan, laid at its lowest RL.
pub struct Outline;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlineParams {
    /// Surfaces, grids and solids.
    pub objects: Vec<ObjectId>,
    pub layer: String,
}

impl Command for Outline {
    type Params = OutlineParams;
    const NAME: &'static str = "string.outline";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Make a surface's or grid's edge, each hole its own string, or a solid's outline in plan",
            inputs: vec![Field::new("objects", FieldType::Objects, "Surfaces, grids and solids")],
            params: vec![Field::new("layer", FieldType::Layer, "Layer for the outlines")],
            outputs: vec![Field::object("outlines", ObjectKind::String, "'<name> outline', then '<name> hole 1' and on")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.objects.is_empty() {
            return Err(CommandError::Invalid("give the surfaces, grids or solids to outline".into()));
        }
        let started = Instant::now();
        let mut changes = Vec::new();
        let mut said = Vec::new();
        for &id in &p.objects {
            let object = inputs.object(id)?;
            let mesh = mesh_of(object, true)?;
            let mut lines = if matches!(object.body, Body::Solid(_)) {
                solid_outline(&mesh).map_err(|e| CommandError::Invalid(format!("{}: {e}", object.name)))?
            } else {
                surface_outline(&mesh)
            };
            if lines.is_empty() {
                return Err(CommandError::Invalid(format!("{} has no edge: it closes all round", object.name)));
            }
            // The outside first, the largest in plan; then the holes, largest first.
            lines.sort_by(|a, b| zd_geom::plan_area(b).total_cmp(&zd_geom::plan_area(a)));
            let open = lines.iter().filter(|l| !l.closed).count();
            for (k, line) in lines.iter().enumerate() {
                let name = if k == 0 { format!("{} outline", object.name) } else { format!("{} hole {k}", object.name) };
                changes.push(created(name, &p.layer, Attributes::new(), line));
            }
            let mut words = format!("{}: {} strings", object.name, lines.len());
            if open > 0 {
                words += &format!(", {open} open where its edge runs into itself");
            }
            said.push(words);
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(Outcome::new(changes.clone()).message(format!("{} outline strings from {} objects in {ms:.0} ms: {}", changes.len(), p.objects.len(), said.join("; "))))
    }
}

/// `string.from_section`: the lines a vertical section through two points cuts in surfaces, grids and solids, kept as
/// strings on the section plane.
pub struct FromSection;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FromSectionParams {
    /// The section's two points in plan, `[x, y, z]` with the RL ignored.
    pub from: [f64; 3],
    pub to: [f64; 3],
    /// Surfaces, grids and solids.
    pub objects: Vec<ObjectId>,
    pub layer: String,
}

impl Command for FromSection {
    type Params = FromSectionParams;
    const NAME: &'static str = "string.from_section";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Make strings of the lines a section cuts in surfaces, grids and solids",
            inputs: vec![Field::new("objects", FieldType::Objects, "Surfaces, grids and solids")],
            params: vec![
                Field::new("from", FieldType::Point, "The section's first point"),
                Field::new("to", FieldType::Point, "Its second point"),
                Field::new("layer", FieldType::Layer, "Layer for the strings"),
            ],
            outputs: vec![Field::object("strings", ObjectKind::String, "'<name> section', numbered after the first")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.objects.is_empty() {
            return Err(CommandError::Invalid("give the surfaces, grids or solids to cut".into()));
        }
        let plane = SectionPlane::through([p.from[0], p.from[1]], [p.to[0], p.to[1]]).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let started = Instant::now();
        let mut changes = Vec::new();
        let mut uncut = Vec::new();
        let mut triangles = 0;
        for &id in &p.objects {
            let object = inputs.object(id)?;
            let mesh = mesh_of(object, true)?;
            triangles += mesh.triangles.len();
            let lines = slice_mesh(&mesh, &plane);
            if lines.is_empty() {
                uncut.push(object.name.clone());
            }
            let name = format!("{} section", object.name);
            changes.extend(lines.iter().enumerate().map(|(k, line)| created(numbered(&name, k), &p.layer, Attributes::new(), line)));
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        if changes.is_empty() {
            return Err(CommandError::Invalid(format!("the section cuts none of the {} objects", p.objects.len())));
        }
        let mut outcome = Outcome::new(changes.clone()).message(format!("{} strings from the section through {} objects, from {triangles} triangles in {ms:.0} ms", changes.len(), p.objects.len()));
        if !uncut.is_empty() {
            outcome = outcome.message(format!("The section doesn't cut {}", uncut.join(", ")));
        }
        Ok(outcome)
    }
}
