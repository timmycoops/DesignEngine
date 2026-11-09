//! Spry projects in (decision 0039): the block solids of a table's Solid fields, named by their node
//! paths with the values Spry holds beside them, and the strings and solids of design layers with
//! their attributes, each on a project layer named after where it was in Spry.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change};
use zd_io::{SPRY_LEVELS, SpryLog, SpryRead, read_spry};

use super::deswik::{Made, MeshesAs, SurfaceTally, import_mesh};
use super::in_parallel;
use super::solid::{Cleaning, ImportTally, NOT_A_SOLID, VOLUME_AGREES, default_fill_limit};
use super::surface::default_tolerance;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// Run records give a line to each mesh when there are no more than this many, and name this many of
/// the solids that differ most.
const LINES_PER_MESH_UP_TO: usize = 10;

/// `spry.import`: the block solids of the Solid fields picked in a Spry project, each named by its
/// node path (`Pit/Strip/Block/Bench`) and carrying its position on each level and the values of the
/// fields beside its Solid field, and the strings and solids of the design layers picked, with their
/// attributes. A mesh goes through the repair step and, by default, becomes a solid where it is closed
/// after it and a surface where it is open, as `deswik.import` does. Spry's own volume of a block is
/// kept as `spry_volume_m3`, and a solid whose volume differs from it is reported.
pub struct ImportSpry;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportSpryParams {
    pub path: String,
    /// The Solid fields to read, named by table, folders and field joined by `\`, as
    /// `Deposit\Coal\Solid`. With none here and no layers, everything is read.
    #[serde(default)]
    pub solid_fields: Vec<String>,
    /// The design layers to read, named by folders and layer joined by `\`.
    #[serde(default)]
    pub layers: Vec<String>,
    /// One layer for everything. Blank: each object goes on a layer named after its Solid field or
    /// design layer.
    #[serde(default)]
    pub layer: Option<String>,
    /// What meshes become.
    #[serde(default)]
    pub meshes: MeshesAs,
    /// The field beside each Solid field holding Spry's volume of its solids.
    #[serde(default = "default_volume_field")]
    pub volume_field: String,
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

fn default_volume_field() -> String {
    "Volume".into()
}

impl Command for ImportSpry {
    type Params = ImportSpryParams;
    const NAME: &'static str = "spry.import";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Import block solids, strings and their attributes from a Spry project",
            inputs: vec![],
            params: vec![
                Field::new("path", FieldType::Path, "A Spry .spry"),
                Field::new("solid_fields", FieldType::Texts, "The Solid fields to read, as Deposit\\Coal\\Solid; with none here and no layers, everything is read"),
                Field::new("layers", FieldType::Texts, "The design layers to read, folders and layer joined by \\"),
                Field::new("layer", FieldType::Layer, "Optional; by default each object goes on a layer named after its Solid field or design layer"),
                Field::new("meshes", FieldType::Text, "auto (closed meshes solids, open ones surfaces), solids, surfaces or none (strings only); auto by default"),
                Field::new("volume_field", FieldType::Text, "The field beside each Solid field holding Spry's volume; Volume by default"),
                Field::new("weld_tolerance", FieldType::Number { unit: "m" }, "The most a corner may move to close a solid; each is welded only as far as it needs. 1 mm by default"),
                Field::new("repair", FieldType::Bool, "Solids: close the mesh first, fins off, cracks joined, holes filled; off by default"),
                Field::new("fill_limit", FieldType::Number { unit: "share" }, "With repair: the most hole area to fill, as a share of the surface; 0.1 by default"),
            ],
            outputs: vec![Field::new("objects", FieldType::Objects, "The strings, surfaces and solids made")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let cleaning = Cleaning { weld_tolerance: p.weld_tolerance, repair: p.repair, fill_limit: p.fill_limit }.checked()?;
        let start = Instant::now();
        let read = SpryRead { solid_fields: p.solid_fields.clone(), layers: p.layers.clone(), volume_field: p.volume_field.clone() };
        let project = read_spry(Path::new(&p.path), &read).map_err(|e| CommandError::Io(format!("{}: {e}", p.path)))?;
        if project.strings.is_empty() && project.meshes.is_empty() {
            return Err(CommandError::Invalid(format!("{} holds no strings or solids in what was read ({})", p.path, describe_read(&project.log))));
        }
        let read_ms = start.elapsed().as_secs_f64() * 1e3;
        let chosen = p.layer.as_deref().map(str::trim).filter(|l| !l.is_empty());
        let mut names = Names::default();
        let mut changes = Vec::new();
        let mut on_layer = |from: &str, name: String, mut attributes: Attributes, body: Body| {
            if chosen.is_some() {
                attributes.insert("spry_layer".into(), AttrValue::Text(from.to_string()));
            }
            changes.push(Change::Create { name, layer: chosen.unwrap_or(from).to_string(), attributes, body });
        };

        let mut outcome_lines = vec![format!("{} in {read_ms:.0} ms", describe_read(&project.log))];
        for s in project.strings {
            let name = names.next(&s.layer, None);
            on_layer(&s.layer, name, s.attributes, Body::String(s.line));
        }

        let start = Instant::now();
        let meshes = if p.meshes == MeshesAs::None {
            outcome_lines.push(format!("{} meshes left out, as asked", project.meshes.len()));
            Vec::new()
        } else {
            project.meshes
        };
        let made = in_parallel(&meshes, |m| import_mesh(&m.mesh, &m.layer, "Spry", p.meshes, cleaning, m.node.is_some()));
        let few = meshes.len() <= LINES_PER_MESH_UP_TO;
        let mut tally = ImportTally::default();
        let mut surfaces = SurfaceTally::default();
        let (mut kept_as_surfaces, mut refused, mut differ) = (Vec::new(), Vec::new(), Vec::new());
        for (mesh, made) in meshes.into_iter().zip(made) {
            let name = names.next(&mesh.layer, mesh.node.as_deref());
            let mut attributes = mesh.attributes;
            if let Some(volume) = mesh.spry_volume {
                attributes.insert("spry_volume_m3".into(), AttrValue::Number(volume));
            }
            match made {
                Made::Solid(imported) => {
                    tally.add(&imported);
                    if few {
                        outcome_lines.extend(imported.messages.iter().map(|m| format!("{name}: {m}")));
                    }
                    if let Some(theirs) = mesh.spry_volume {
                        let ours = imported.solid.volume();
                        if (ours - theirs).abs() > VOLUME_AGREES * theirs.abs() {
                            differ.push((format!("{name} ({})", mesh.layer), ours, theirs));
                        }
                    }
                    attributes.extend(imported.attributes);
                    on_layer(&mesh.layer, name, attributes, Body::Solid(imported.solid));
                }
                Made::Surface { surface, messages, refused_as_solid } => {
                    surfaces.add(&surface);
                    if few {
                        outcome_lines.extend(messages.iter().map(|m| format!("{name}: {m}")));
                    }
                    if let Some(why) = refused_as_solid {
                        kept_as_surfaces.push(format!("{name} ({}): {why}", mesh.layer));
                        attributes.insert(NOT_A_SOLID.into(), AttrValue::Text(why));
                    }
                    on_layer(&mesh.layer, name, attributes, Body::Surface(surface));
                }
                Made::Nothing(why) => refused.push(format!("{name} ({}): {why}", mesh.layer)),
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
            outcome = outcome.message(format!(
                "{} solids whose volume differs from Spry's own ({}) by more than a millionth, {total:+.3} m3 in all, the largest first:",
                differ.len(),
                p.volume_field.trim()
            ));
            outcome.messages.extend(differ.iter().take(LINES_PER_MESH_UP_TO).map(|(name, ours, theirs)| {
                format!("{name}: {ours:.3} m3 here, {theirs:.3} m3 in Spry ({:+.4} %)", 100.0 * (ours - theirs) / theirs)
            }));
        }
        if !kept_as_surfaces.is_empty() {
            outcome = outcome.message(format!("{} meshes kept as surfaces, as they could not be solids, each flagged not_a_solid:", kept_as_surfaces.len()));
            outcome.messages.extend(kept_as_surfaces.into_iter().take(LINES_PER_MESH_UP_TO));
        }
        if !refused.is_empty() {
            outcome = outcome.message(format!("{} meshes refused, nothing made of them:", refused.len()));
            outcome.messages.extend(refused.into_iter().take(LINES_PER_MESH_UP_TO));
        }
        Ok(outcome)
    }
}

/// Object names: a block by its node path, and anything on a design layer by the layer's last level
/// and a count on that layer, as `TOES 12`. A node path met twice on one layer takes a count after it.
#[derive(Default)]
struct Names(BTreeMap<String, usize>);

impl Names {
    fn next(&mut self, layer: &str, node: Option<&str>) -> String {
        match node {
            Some(node) => {
                let n = self.0.entry(format!("{layer}\n{node}")).or_default();
                *n += 1;
                if *n == 1 { node.to_string() } else { format!("{node} ({n})") }
            }
            None => {
                let n = self.0.entry(layer.to_string()).or_default();
                *n += 1;
                format!("{} {n}", layer.rsplit(SPRY_LEVELS).next().unwrap_or(layer))
            }
        }
    }
}

/// What the reader did, in one line.
fn describe_read(log: &SpryLog) -> String {
    let mut parts = vec![format!(
        "Spry (database version {}): {} solid fields read, {} block solids; {} layers read, {} solids and {} strings; {} triangles",
        log.database_version, log.solid_fields_read, log.node_solids_read, log.layers_read, log.layer_solids_read, log.strings_read, log.triangles_read
    )];
    if log.node_solids_without_triangles > 0 {
        parts.push(format!("{} block solids with faces but no triangles (Spry's flat plot polygons) passed over", log.node_solids_without_triangles));
    }
    if !log.solid_fields_without_volume.is_empty() {
        parts.push(format!("no volume field beside {}", log.solid_fields_without_volume.join(", ")));
    }
    if log.node_solids_without_volume > 0 {
        parts.push(format!("{} block solids with no volume stored", log.node_solids_without_volume));
    }
    if log.short_strings > 0 {
        parts.push(format!("{} strings with one vertex passed over", log.short_strings));
    }
    if log.closed_too_short > 0 {
        parts.push(format!("{} closed strings with two vertices read as open", log.closed_too_short));
    }
    if !log.values_passed_over.is_empty() {
        let skipped: Vec<String> = log.values_passed_over.iter().map(|(kind, n)| format!("{n} {kind}")).collect();
        parts.push(format!("fields passed over by type: {}", skipped.join(", ")));
    }
    if !log.fields_unread.is_empty() {
        parts.push(format!("{} fields that could not be read: {}", log.fields_unread.len(), log.fields_unread.join("; ")));
    }
    if !log.attributes_unread.is_empty() {
        parts.push(format!("attributes not read on {} layers: {}", log.attributes_unread.len(), log.attributes_unread.join("; ")));
    }
    if log.layers_with_text > 0 {
        parts.push(format!("text on {} layers passed over", log.layers_with_text));
    }
    parts.join("; ")
}
