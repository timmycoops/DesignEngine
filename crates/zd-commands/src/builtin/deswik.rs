//! Deswik drawings in (decision 0036): the strings and meshes of the layers picked from a `.dcf`, each
//! object on a project layer named after its Deswik layer, with its Deswik attributes.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, Surface};
use zd_geom::repair_mesh;
use zd_io::{DESWIK_LEVELS, DeswikLog, DeswikMesh, read_deswik_layers};

use super::in_parallel;
use super::solid::{Cleaning, ImportTally, ImportedSolid, default_fill_limit, solid_from_welded};
use super::surface::{check_crossings, default_tolerance, describe_repair};
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// A solid's volume and Deswik's own figure for it differ when they are more than this share apart.
const VOLUME_AGREES: f64 = 1e-6;
/// Run records give a line to each mesh when there are no more than this many.
const LINES_PER_MESH_UP_TO: usize = 10;

/// `deswik.import`: strings, surfaces and solids from the layers picked in a Deswik `.dcf`. A string
/// keeps its vertices and attributes (projection details among them). A mesh goes through the repair
/// step; by default one closed after it becomes a solid by the steps of `solid.import`, and one open
/// a surface by those of `surface.import`. A closed mesh refused as a solid is kept as a surface and
/// named with why. Where Deswik stored a volume for a mesh it is kept as `deswik_volume_m3`, and a
/// solid whose volume differs from it is reported.
pub struct ImportDeswik;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportDeswikParams {
    pub path: String,
    /// The Deswik layers to read, named as Deswik shows them (levels joined by `\`); every layer if none.
    #[serde(default)]
    pub layers: Vec<String>,
    /// One layer for everything. Blank: each object goes on a layer named after its Deswik layer.
    #[serde(default)]
    pub layer: Option<String>,
    /// What meshes become.
    #[serde(default)]
    pub meshes: MeshesAs,
    /// Vertices closer than this (m) are welded, and crossings by less than it are not counted.
    #[serde(default = "default_tolerance")]
    pub weld_tolerance: f64,
    /// For solids: close the mesh first, fins off, cracks joined, holes filled (decision 0013).
    #[serde(default)]
    pub repair: bool,
    /// With `repair`: the most hole area to fill, as a share of a solid's surface (0.1 = 10 %).
    #[serde(default = "default_fill_limit")]
    pub fill_limit: f64,
}

/// What a mesh becomes: a solid where it is closed after the weld and a surface where it is open,
/// every mesh a solid (so `repair` can close them), every mesh a surface, or nothing (strings only).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshesAs {
    #[default]
    Auto,
    Solids,
    Surfaces,
    None,
}

impl Command for ImportDeswik {
    type Params = ImportDeswikParams;
    const NAME: &'static str = "deswik.import";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Import strings, surfaces and solids from a Deswik drawing",
            inputs: vec![],
            params: vec![
                Field::new("path", FieldType::Path, "A Deswik .dcf"),
                Field::new("layers", FieldType::Texts, "The Deswik layers to read, named as Deswik shows them (levels joined by \\); every layer if none"),
                Field::new("layer", FieldType::Layer, "Optional; by default each object goes on a layer named after its Deswik layer"),
                Field::new("meshes", FieldType::Text, "auto (closed meshes solids, open ones surfaces), solids, surfaces or none (strings only); auto by default"),
                Field::new("weld_tolerance", FieldType::Number { unit: "m" }, "Weld vertices closer than this; 1 mm by default"),
                Field::new("repair", FieldType::Bool, "Solids: close the mesh first, fins off, cracks joined, holes filled; off by default"),
                Field::new("fill_limit", FieldType::Number { unit: "share" }, "With repair: the most hole area to fill, as a share of the surface; 0.1 by default"),
            ],
            outputs: vec![Field::new("objects", FieldType::Objects, "The strings, surfaces and solids made")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let cleaning = Cleaning { weld_tolerance: p.weld_tolerance, repair: p.repair, fill_limit: p.fill_limit }.checked()?;
        let start = Instant::now();
        let drawing = read_deswik_layers(Path::new(&p.path), &p.layers).map_err(|e| CommandError::Io(format!("{}: {e}", p.path)))?;
        if drawing.strings.is_empty() && drawing.meshes.is_empty() {
            return Err(CommandError::Invalid(format!("{} holds no strings or meshes on the layers read ({})", p.path, describe_read(&drawing.log))));
        }
        let read_ms = start.elapsed().as_secs_f64() * 1e3;
        let chosen = p.layer.as_deref().map(str::trim).filter(|l| !l.is_empty());
        let mut names = Names::default();
        let mut changes = Vec::new();
        let mut on_layer = |deswik: &str, mut attributes: Attributes, body: Body, names: &mut Names| {
            if chosen.is_some() {
                attributes.insert("deswik_layer".into(), AttrValue::Text(deswik.to_string()));
            }
            changes.push(Change::Create { name: names.next(deswik), layer: chosen.unwrap_or(deswik).to_string(), attributes, body });
        };

        let mut outcome_lines = vec![format!("{} in {read_ms:.0} ms", describe_read(&drawing.log))];
        for s in drawing.strings {
            on_layer(&s.layer, s.attributes, Body::String(s.line), &mut names);
        }

        let start = Instant::now();
        let meshes: &[DeswikMesh] = if p.meshes == MeshesAs::None {
            if !drawing.meshes.is_empty() {
                outcome_lines.push(format!("{} meshes left out, as asked", drawing.meshes.len()));
            }
            &[]
        } else {
            &drawing.meshes
        };
        let made = in_parallel(meshes, |m| import_mesh(m, p.meshes, cleaning));
        let few = meshes.len() <= LINES_PER_MESH_UP_TO;
        let mut tally = ImportTally::default();
        let mut surfaces = SurfaceTally::default();
        let (mut kept_as_surfaces, mut refused, mut differ) = (Vec::new(), Vec::new(), Vec::new());
        for (k, (mesh, made)) in meshes.iter().zip(made).enumerate() {
            let name = names.peek(&mesh.layer);
            let mut attributes = mesh.attributes.clone();
            if let Some(volume) = mesh.deswik_volume {
                attributes.insert("deswik_volume_m3".into(), AttrValue::Number(volume));
            }
            match made {
                Made::Solid(imported) => {
                    tally.add(&imported);
                    if few {
                        outcome_lines.extend(imported.messages.iter().map(|m| format!("{name}: {m}")));
                    }
                    if let Some(theirs) = mesh.deswik_volume {
                        let ours = imported.solid.volume();
                        if (ours - theirs).abs() > VOLUME_AGREES * theirs {
                            differ.push((name.clone(), ours, theirs));
                        }
                    }
                    attributes.extend(imported.attributes);
                    on_layer(&mesh.layer, attributes, Body::Solid(imported.solid), &mut names);
                }
                Made::Surface { surface, messages, refused_as_solid } => {
                    surfaces.add(&surface);
                    if few {
                        outcome_lines.extend(messages.iter().map(|m| format!("{name}: {m}")));
                    }
                    if let Some(why) = refused_as_solid {
                        kept_as_surfaces.push(format!("{name} ({}): {why}", mesh.layer));
                    }
                    on_layer(&mesh.layer, attributes, Body::Surface(surface), &mut names);
                }
                Made::Nothing(why) => refused.push(format!("mesh {} of the run, on {}: {why}", k + 1, mesh.layer)),
            }
        }
        let ms = start.elapsed().as_secs_f64() * 1e3;
        if changes.is_empty() {
            if !refused.is_empty() {
                outcome_lines.push(format!("{} meshes refused, nothing made of them:", refused.len()));
                outcome_lines.extend(refused);
            }
            return Err(CommandError::Invalid(format!("nothing made from {}: {}", p.path, outcome_lines.join("; "))));
        }

        let mut outcome = Outcome::new(changes);
        outcome.messages = outcome_lines;
        if tally.solids > 0 {
            outcome = outcome.message(tally.describe(cleaning.weld_tolerance, ms));
        }
        if surfaces.count > 0 {
            outcome = outcome.message(surfaces.describe(cleaning.weld_tolerance));
        }
        if !differ.is_empty() {
            // Largest in cubic metres first: a share of a solid of no size says little.
            differ.sort_by(|a, b| (b.1 - b.2).abs().total_cmp(&(a.1 - a.2).abs()));
            let total: f64 = differ.iter().map(|d| d.1 - d.2).sum();
            outcome = outcome.message(format!("{} solids whose volume differs from Deswik's own by more than a millionth, {total:+.3} m3 in all, the largest first:", differ.len()));
            outcome.messages.extend(differ.iter().take(LINES_PER_MESH_UP_TO).map(|(name, ours, theirs)| {
                format!("{name}: {ours:.3} m3 here, {theirs:.3} m3 in Deswik ({:+.4} %)", 100.0 * (ours - theirs) / theirs)
            }));
        }
        if !kept_as_surfaces.is_empty() {
            outcome = outcome.message(format!("{} closed meshes kept as surfaces, as they could not be solids:", kept_as_surfaces.len()));
            outcome.messages.extend(kept_as_surfaces);
        }
        if !refused.is_empty() {
            outcome = outcome.message(format!("{} meshes refused, nothing made of them:", refused.len()));
            outcome.messages.extend(refused);
        }
        Ok(outcome)
    }
}

/// A Deswik mesh as made: a solid, a surface (with why, if it was closed but refused as a solid), or
/// nothing.
enum Made {
    Solid(ImportedSolid),
    Surface { surface: Surface, messages: Vec<String>, refused_as_solid: Option<String> },
    Nothing(String),
}

/// One mesh through the repair step, then to a solid or a surface. A panic in the geometry is this
/// mesh's refusal, not the run's.
fn import_mesh(mesh: &DeswikMesh, meshes: MeshesAs, cleaning: Cleaning) -> Made {
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let read = format!("Deswik mesh: {} vertices, {} triangles", mesh.mesh.vertices.len(), mesh.mesh.triangle_count());
        let (welded, log) = repair_mesh(&mesh.mesh, cleaning.weld_tolerance);
        if welded.triangles.is_empty() {
            return Made::Nothing(format!("no triangles left after the weld ({read})"));
        }
        let closed = welded.validity().boundary_edges == 0;
        let as_solid = match meshes {
            MeshesAs::Solids => true,
            MeshesAs::Surfaces | MeshesAs::None => false,
            MeshesAs::Auto => closed,
        };
        let mut refused_as_solid = None;
        if as_solid {
            match solid_from_welded(welded.clone(), log.clone(), read.clone(), &mesh.layer, &mesh.layer, cleaning) {
                Ok(imported) => return Made::Solid(imported),
                Err(why) if meshes == MeshesAs::Auto => refused_as_solid = Some(why.to_string()),
                Err(why) => return Made::Nothing(why.to_string()),
            }
        }
        let repair = describe_repair(&log, &welded);
        let (crossings, checked) = check_crossings(&welded, cleaning.weld_tolerance);
        Made::Surface { surface: Surface::new(welded).with_crossings(crossings), messages: vec![read, repair, checked], refused_as_solid }
    }));
    attempt.unwrap_or_else(|_| Made::Nothing("the import stopped on an error in the geometry".into()))
}

/// Object names from the last level of their Deswik layer and a count on that layer: `SOLIDS 12`.
#[derive(Default)]
struct Names(BTreeMap<String, usize>);

impl Names {
    fn base(layer: &str) -> &str {
        layer.rsplit(DESWIK_LEVELS).next().unwrap_or(layer)
    }

    fn peek(&self, layer: &str) -> String {
        format!("{} {}", Self::base(layer), self.0.get(layer).copied().unwrap_or(0) + 1)
    }

    fn next(&mut self, layer: &str) -> String {
        let n = self.0.entry(layer.to_string()).or_default();
        *n += 1;
        format!("{} {n}", Self::base(layer))
    }
}

/// What a run's surfaces came to.
#[derive(Default)]
struct SurfaceTally {
    count: usize,
    triangles: usize,
    crossings: usize,
}

impl SurfaceTally {
    fn add(&mut self, surface: &Surface) {
        self.count += 1;
        self.triangles += surface.mesh().triangle_count();
        self.crossings += surface.crossings().map_or(0, |c| c.pairs);
    }

    fn describe(&self, tolerance: f64) -> String {
        format!(
            "{} surfaces imported, {} triangles, through the repair at {} mm; {} pairs of triangles crossing, kept and flagged",
            self.count,
            self.triangles,
            tolerance * 1e3,
            self.crossings
        )
    }
}

/// What the reader did, in one line.
fn describe_read(log: &DeswikLog) -> String {
    let versions: Vec<&str> = log.versions.iter().map(String::as_str).collect();
    let mut parts = vec![format!(
        "Deswik: {} layers read (VectorDraw {}), {} strings and {} meshes of {} faces",
        log.layers_read,
        versions.join(", "),
        log.strings_read,
        log.meshes_read,
        log.faces_read
    )];
    if log.quads_split > 0 {
        parts.push(format!("{} four-cornered faces split in two", log.quads_split));
    }
    if log.degenerate_dropped > 0 {
        parts.push(format!("{} faces of fewer than three corners dropped", log.degenerate_dropped));
    }
    if log.arcs_to_chords > 0 {
        parts.push(format!("{} arcs replaced by chords within {} m", log.arcs_to_chords, zd_io::ARC_SAGITTA));
    }
    if log.short_strings > 0 {
        parts.push(format!("{} strings with one vertex passed over", log.short_strings));
    }
    if log.closed_too_short > 0 {
        parts.push(format!("{} closed strings with two vertices read as open", log.closed_too_short));
    }
    if log.empty_meshes > 0 {
        parts.push(format!("{} meshes with no whole face passed over", log.empty_meshes));
    }
    if !log.skipped.is_empty() {
        let skipped: Vec<String> = log.skipped.iter().map(|(kind, n)| format!("{n} {kind}")).collect();
        parts.push(format!("passed over {}", skipped.join(", ")));
    }
    if !log.checksum_mismatches.is_empty() {
        parts.push(format!(
            "{} layers whose zip checksum doesn't match what they hold, read as their XML runs whole: {}",
            log.checksum_mismatches.len(),
            log.checksum_mismatches.join(", ")
        ));
    }
    parts.join("; ")
}
