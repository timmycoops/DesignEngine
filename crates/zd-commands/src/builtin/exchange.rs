//! Strings in from DXF and CSV, and objects out to DXF R12 and CSV (decision 0009). Exports write a
//! file and change nothing: their run record lists the object revisions written.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, Object, ObjectId, Polyline};
use zd_io::{ARC_SAGITTA, CsvLog, DxfDrawing, DxfEntity, read_csv_strings, read_dxf, write_csv_strings, write_dxf};

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// `string.import`: strings from a DXF file (POLYLINE, LWPOLYLINE and LINE) or a CSV file.
pub struct ImportStrings;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportStringsParams {
    pub path: String,
    /// One layer for every string. Blank: each string goes on a layer named after its DXF layer or
    /// its CSV `layer` cell, else on `strings`.
    #[serde(default)]
    pub layer: Option<String>,
}

impl Command for ImportStrings {
    type Params = ImportStringsParams;
    const NAME: &'static str = "string.import";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Import strings from DXF or CSV",
            inputs: vec![],
            params: vec![
                Field::new("path", FieldType::Path, ".dxf, or .csv with x, y and z columns"),
                Field::new("layer", FieldType::Layer, "Optional; by default each string keeps its DXF or CSV layer"),
            ],
            outputs: vec![Field::new("strings", FieldType::Objects, "The imported strings")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let path = Path::new(&p.path);
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "string".into());
        let read_error = |e: zd_io::IoError| CommandError::Io(format!("{}: {e}", p.path));
        let (strings, message): (Vec<Read>, String) = match extension(path).as_str() {
            "dxf" => {
                let drawing = read_dxf(path).map_err(read_error)?;
                let message = describe_dxf_strings(&drawing);
                (drawing.strings.into_iter().map(|s| (Some(s.layer), None, s.line)).collect(), message)
            }
            "csv" | "txt" => {
                let csv = read_csv_strings(path).map_err(read_error)?;
                let message = describe_csv(&csv.log);
                (csv.strings.into_iter().map(|s| (s.layer, s.name, s.line)).collect(), message)
            }
            ext => return Err(CommandError::Io(format!("no string reader for '.{ext}' files; use .dxf or .csv"))),
        };
        if strings.is_empty() {
            return Err(CommandError::Invalid(format!("{} holds no strings ({message})", p.path)));
        }

        let chosen = p.layer.as_deref().map(str::trim).filter(|l| !l.is_empty());
        let mut numbers: BTreeMap<String, usize> = BTreeMap::new();
        let changes = strings
            .into_iter()
            .map(|(source, name, line)| {
                let base = source.clone().unwrap_or_else(|| stem.clone());
                let n = numbers.entry(base.clone()).or_default();
                *n += 1;
                Change::Create {
                    name: name.unwrap_or_else(|| format!("{base} {n}")),
                    layer: chosen.map(str::to_string).or(source).unwrap_or_else(|| "strings".into()),
                    attributes: Attributes::new(),
                    body: Body::String(line),
                }
            })
            .collect();
        Ok(Outcome::new(changes).message(message))
    }
}

/// A string as read: the layer it came from and its name, if the file gives them.
type Read = (Option<String>, Option<String>, Polyline);

fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// What a DXF reader skipped, for the run record: entity types by count.
fn describe_skipped(skipped: &BTreeMap<String, usize>) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }
    let mut by_count: Vec<(&String, &usize)> = skipped.iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    Some(format!("passed over {}", by_count.iter().map(|(kind, n)| format!("{n} {kind}")).collect::<Vec<_>>().join(", ")))
}

/// The DXF reader's log as the string import sees it, in one line.
pub(crate) fn describe_dxf_strings(drawing: &DxfDrawing) -> String {
    let log = &drawing.log;
    let layers: HashSet<&str> = drawing.strings.iter().map(|s| s.layer.as_str()).collect();
    let mut parts = vec![format!("DXF: {} read on {}", plural(log.strings_read, "string", "strings"), plural(layers.len(), "layer", "layers"))];
    if log.arcs_to_chords > 0 {
        parts.push(format!("{} replaced by chords within {ARC_SAGITTA} m", plural(log.arcs_to_chords, "arc", "arcs")));
    }
    if log.tilted_moved > 0 {
        parts.push(format!("{} drawn in a tilted plane moved to world coordinates", plural(log.tilted_moved, "string", "strings")));
    }
    if log.short_strings > 0 {
        parts.push(format!("{} with one vertex passed over", plural(log.short_strings, "string", "strings")));
    }
    if log.closed_too_short > 0 {
        parts.push(format!("{} with two vertices read as open", plural(log.closed_too_short, "closed string", "closed strings")));
    }
    let faces = drawing.mesh.triangle_count();
    if faces > 0 {
        parts.push(format!("{} not read (import the file as a surface)", plural(faces, "triangle", "triangles")));
    }
    parts.extend(describe_skipped(&log.skipped));
    parts.join("; ")
}

/// The DXF reader's log as the surface import sees it, in one line.
pub(crate) fn describe_dxf_faces(drawing: &DxfDrawing) -> String {
    let log = &drawing.log;
    let mut parts = vec![format!(
        "DXF: {} faces read ({} polyface meshes), {} quads split, {} degenerate and {} duplicate faces dropped",
        log.faces_read, log.polyface_meshes, log.quads_split, log.degenerate_dropped, log.duplicates_dropped
    )];
    if !drawing.strings.is_empty() {
        parts.push(format!("{} not read (import the file as strings)", plural(drawing.strings.len(), "string", "strings")));
    }
    parts.extend(describe_skipped(&log.skipped));
    parts.join("; ")
}

fn describe_csv(log: &CsvLog) -> String {
    let mut parts = vec![format!("CSV: {} read from {}", plural(log.strings_read, "string", "strings"), plural(log.rows_read, "row", "rows"))];
    if log.closing_repeats_dropped > 0 {
        parts.push(format!(
            "{} ending on its first vertex read as closed and the repeat dropped",
            plural(log.closing_repeats_dropped, "string", "strings")
        ));
    }
    if log.short_strings > 0 {
        parts.push(format!("{} with one vertex passed over", plural(log.short_strings, "string", "strings")));
    }
    if log.closed_too_short > 0 {
        parts.push(format!("{} with two vertices read as open", plural(log.closed_too_short, "closed string", "closed strings")));
    }
    if !log.ignored_columns.is_empty() {
        parts.push(format!("columns not read: {}", log.ignored_columns.join(", ")));
    }
    parts.join("; ")
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportParams {
    pub objects: Vec<ObjectId>,
    pub path: String,
    /// Replace the file if it exists; by default an existing file is never overwritten.
    #[serde(default)]
    pub overwrite: bool,
}

fn export_params() -> Vec<Field> {
    vec![Field::new("path", FieldType::Path, "The file to write"), Field::new("overwrite", FieldType::Bool, "Replace an existing file")]
}

/// The objects to export, each once, in the order given; refused if the list is empty or the file
/// exists and is not to be replaced.
fn export_objects<'a>(inputs: &Inputs<'a>, p: &ExportParams) -> Result<Vec<&'a Arc<Object>>, CommandError> {
    if p.objects.is_empty() {
        return Err(CommandError::Invalid("no objects to export".into()));
    }
    if Path::new(&p.path).exists() && !p.overwrite {
        return Err(CommandError::Invalid(format!("{} exists; set overwrite to replace it", p.path)));
    }
    let mut seen = HashSet::new();
    p.objects.iter().filter(|id| seen.insert(**id)).map(|&id| inputs.object(id)).collect()
}

/// `export.dxf`: strings as 3D polylines and surfaces and solids as 3DFACEs, each on a DXF layer
/// named after its project layer, in one DXF R12 file.
pub struct ExportDxf;

impl Command for ExportDxf {
    type Params = ExportParams;
    const NAME: &'static str = "export.dxf";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Export strings, surfaces and solids to DXF R12",
            inputs: vec![Field::new("objects", FieldType::Objects, "Strings (as 3D polylines), surfaces and solids (as 3DFACEs)")],
            params: export_params(),
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let objects = export_objects(inputs, p)?;
        let mut entities = Vec::with_capacity(objects.len());
        let mut layers = Vec::with_capacity(objects.len());
        let mut refused = Vec::new();
        let (mut strings, mut vertices, mut meshes, mut triangles) = (0, 0, 0, 0);
        for object in &objects {
            match &object.body {
                Body::String(line) => {
                    strings += 1;
                    vertices += line.vertices.len();
                    entities.push(DxfEntity::String(line));
                }
                Body::Surface(surface) => {
                    meshes += 1;
                    triangles += surface.mesh().triangle_count();
                    entities.push(DxfEntity::Mesh(surface.mesh()));
                }
                Body::Solid(solid) => {
                    meshes += 1;
                    triangles += solid.mesh().triangle_count();
                    entities.push(DxfEntity::Mesh(solid.mesh()));
                }
                Body::Grid(_) | Body::SeamModel(_) | Body::FaceDesign(_) => {
                    refused.push(object.name.as_str());
                    continue;
                }
            }
            layers.push(inputs.layer_name(object));
        }
        if !refused.is_empty() {
            return Err(CommandError::Invalid(format!("grids and seam models are not exported to DXF: {}", refused.join(", "))));
        }
        let entities: Vec<(&str, DxfEntity<'_>)> = layers.iter().map(String::as_str).zip(entities).collect();
        write_dxf(Path::new(&p.path), &entities).map_err(|e| CommandError::Io(format!("{}: {e}", p.path)))?;
        let mut wrote = Vec::new();
        if strings > 0 {
            wrote.push(format!("{} ({vertices} vertices)", plural(strings, "string", "strings")));
        }
        if meshes > 0 {
            wrote.push(format!("{} ({triangles} triangles)", plural(meshes, "surface or solid", "surfaces and solids")));
        }
        Ok(Outcome::new(vec![]).message(format!("wrote {} to {} as DXF R12", wrote.join(" and "), p.path)))
    }
}

/// `export.csv`: strings as CSV, one row per vertex (`string,name,layer,closed,x,y,z`).
pub struct ExportCsv;

impl Command for ExportCsv {
    type Params = ExportParams;
    const NAME: &'static str = "export.csv";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Export strings to CSV",
            inputs: vec![Field::new("objects", FieldType::Objects, "Strings")],
            params: export_params(),
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let objects = export_objects(inputs, p)?;
        let layers: Vec<String> = objects.iter().map(|o| inputs.layer_name(o)).collect();
        let mut rows: Vec<(&str, &str, &Polyline)> = Vec::with_capacity(objects.len());
        let mut refused = Vec::new();
        for (object, layer) in objects.iter().zip(&layers) {
            match &object.body {
                Body::String(line) => rows.push((object.name.as_str(), layer.as_str(), line)),
                _ => refused.push(object.name.as_str()),
            }
        }
        if !refused.is_empty() {
            return Err(CommandError::Invalid(format!("only strings are exported to CSV, not {}", refused.join(", "))));
        }
        write_csv_strings(Path::new(&p.path), &rows).map_err(|e| CommandError::Io(format!("{}: {e}", p.path)))?;
        let vertices: usize = rows.iter().map(|r| r.2.vertices.len()).sum();
        Ok(Outcome::new(vec![]).message(format!("wrote {} ({} vertices) to {} as CSV", plural(rows.len(), "string", "strings"), vertices, p.path)))
    }
}
