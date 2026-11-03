use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, ObjectId, ObjectKind, Polyline};
use zd_geom::{
    Corner, OffsetSide, drape_string, filter_string, grade_string, join_strings, offset_string, plan_crossings, smooth_string, split_string, triangulate_grid,
};

use super::finite;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

fn string_input() -> Field {
    Field::object("string", ObjectKind::String, "The string to edit")
}

/// Open strings need two vertices, closed ones three; every coordinate finite.
fn check_line(line: &Polyline) -> Result<(), CommandError> {
    let needed = if line.closed { 3 } else { 2 };
    if line.vertices.len() < needed {
        let kind = if line.closed { "closed" } else { "open" };
        return Err(CommandError::Invalid(format!("an {kind} string needs at least {needed} vertices")));
    }
    line.vertices.iter().try_for_each(finite)
}

fn vertex_index(line: &Polyline, index: usize) -> Result<(), CommandError> {
    if index < line.vertices.len() {
        Ok(())
    } else {
        Err(CommandError::Invalid(format!("vertex {index} is past the end of a {}-vertex string", line.vertices.len())))
    }
}

/// `string.create`: a new string from points (digitised, typed or imported).
pub struct CreateString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStringParams {
    pub name: String,
    pub layer: String,
    pub points: Vec<[f64; 3]>,
    #[serde(default)]
    pub closed: bool,
}

impl Command for CreateString {
    type Params = CreateStringParams;
    const NAME: &'static str = "string.create";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "New string from points",
            inputs: vec![],
            params: vec![
                Field::new("name", FieldType::Text, "Name of the new string"),
                Field::new("layer", FieldType::Layer, "Layer for the new string"),
                Field::new("points", FieldType::Points, "Vertices in order"),
                Field::new("closed", FieldType::Bool, "Join the last vertex back to the first"),
            ],
            outputs: vec![Field::object("string", ObjectKind::String, "The new string")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let line = Polyline { vertices: p.points.clone(), closed: p.closed };
        check_line(&line)?;
        Ok(Outcome::new(vec![Change::Create {
            name: p.name.clone(),
            layer: p.layer.clone(),
            attributes: Attributes::new(),
            body: Body::String(line),
        }]))
    }
}

/// `string.insert_vertex`: a vertex inserted before `index` (`index` = vertex count appends).
pub struct InsertVertex;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsertVertexParams {
    pub string: ObjectId,
    pub index: usize,
    pub point: [f64; 3],
}

impl Command for InsertVertex {
    type Params = InsertVertexParams;
    const NAME: &'static str = "string.insert_vertex";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Insert a vertex",
            inputs: vec![string_input()],
            params: vec![
                Field::new("index", FieldType::Integer, "Insert before this vertex; the vertex count appends"),
                Field::new("point", FieldType::Point, "The new vertex"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        if p.index > line.vertices.len() {
            return Err(CommandError::Invalid(format!(
                "cannot insert at {} in a {}-vertex string",
                p.index,
                line.vertices.len()
            )));
        }
        finite(&p.point)?;
        let mut edited = line.clone();
        edited.vertices.insert(p.index, p.point);
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(edited))]))
    }
}

/// `string.move_vertex`: one vertex moved to a new position (the end of a drag).
pub struct MoveVertex;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveVertexParams {
    pub string: ObjectId,
    pub index: usize,
    pub to: [f64; 3],
}

impl Command for MoveVertex {
    type Params = MoveVertexParams;
    const NAME: &'static str = "string.move_vertex";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Move a vertex",
            inputs: vec![string_input()],
            params: vec![
                Field::new("index", FieldType::Integer, "The vertex to move"),
                Field::new("to", FieldType::Point, "Its new position"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        vertex_index(line, p.index)?;
        finite(&p.to)?;
        let mut edited = line.clone();
        edited.vertices[p.index] = p.to;
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(edited))]))
    }
}

/// `string.delete_vertex`: one vertex removed; refused if it would leave too few.
pub struct DeleteVertex;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteVertexParams {
    pub string: ObjectId,
    pub index: usize,
}

impl Command for DeleteVertex {
    type Params = DeleteVertexParams;
    const NAME: &'static str = "string.delete_vertex";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Delete a vertex",
            inputs: vec![string_input()],
            params: vec![Field::new("index", FieldType::Integer, "The vertex to delete")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        vertex_index(line, p.index)?;
        let mut edited = line.clone();
        edited.vertices.remove(p.index);
        check_line(&edited)?;
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(edited))]))
    }
}

/// The vertex range `from..=to` (either direction), defaulting `to` to the last vertex.
fn vertex_range(line: &Polyline, from: usize, to: Option<usize>) -> Result<usize, CommandError> {
    vertex_index(line, from)?;
    let to = to.unwrap_or(line.vertices.len() - 1);
    vertex_index(line, to)?;
    Ok(to)
}

/// `string.set_grade`: elevations along the string at a grade. Vertex `from` takes `rl` (its own
/// elevation by default) and each vertex from there to `to` rises `grade` % per 100 m of plan length;
/// the rest keep theirs. A new revision of the string.
pub struct SetGrade;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetGradeParams {
    pub string: ObjectId,
    pub from: usize,
    /// Defaults to the last vertex; may be before `from` to grade backwards.
    #[serde(default)]
    pub to: Option<usize>,
    /// Grade in %, positive uphill in the direction from `from` to `to`.
    pub grade: f64,
    /// Elevation at `from`; its current elevation by default.
    #[serde(default)]
    pub rl: Option<f64>,
}

impl Command for SetGrade {
    type Params = SetGradeParams;
    const NAME: &'static str = "string.set_grade";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Set a grade along a string",
            inputs: vec![string_input()],
            params: vec![
                Field::new("from", FieldType::Integer, "The vertex the grade starts at"),
                Field::new("to", FieldType::Integer, "The vertex it ends at; the last by default"),
                Field::new("grade", FieldType::Number { unit: "%" }, "Positive uphill from the start, along plan length"),
                Field::new("rl", FieldType::Number { unit: "m" }, "Elevation at the start; its own by default"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        let to = vertex_range(line, p.from, p.to)?;
        let rl = p.rl.unwrap_or(line.vertices[p.from][2]);
        let graded = grade_string(line, p.from, to, rl, p.grade).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let message = format!(
            "vertices {} to {} at {} % from RL {rl:.3}: RL {:.3} at vertex {}",
            p.from + 1,
            to + 1,
            p.grade,
            graded.vertices[to][2],
            to + 1
        );
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(graded))]).message(message))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideParam {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CornerParam {
    #[default]
    Mitre,
    Round,
}

fn default_mitre_limit() -> f64 {
    4.0
}

fn once() -> u32 {
    1
}

/// `string.offset`: a new string `distance` metres in plan to the left or right of this one (looking
/// along it from above), with mitred or round outside corners. Each new vertex takes the elevation of
/// the nearest point on the source, or `rl`. The source is not changed; an offset that pinches off
/// makes one string per piece. With `times`, a string at each multiple of the distance.
pub struct OffsetString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OffsetStringParams {
    pub string: ObjectId,
    pub distance: f64,
    pub side: SideParam,
    #[serde(default)]
    pub corners: CornerParam,
    /// Mitres reaching farther than this many times the distance are bevelled.
    #[serde(default = "default_mitre_limit")]
    pub mitre_limit: f64,
    /// A fixed elevation for the new string; by default each vertex takes the source's.
    #[serde(default)]
    pub rl: Option<f64>,
    /// Defaults to the source string's layer.
    #[serde(default)]
    pub layer: Option<String>,
    /// Defaults to the source's name with the offset.
    #[serde(default)]
    pub name: Option<String>,
    /// How many strings, each `distance` further out, all measured from the source.
    #[serde(default = "once")]
    pub times: u32,
}

impl Command for OffsetString {
    type Params = OffsetStringParams;
    const NAME: &'static str = "string.offset";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Offset a string in plan",
            inputs: vec![string_input()],
            params: vec![
                Field::new("distance", FieldType::Number { unit: "m" }, "Horizontal distance"),
                Field::new("side", FieldType::Text, "left or right, looking along the string"),
                Field::new("corners", FieldType::Text, "mitre (default) or round"),
                Field::new("mitre_limit", FieldType::Number { unit: "x distance" }, "Mitres reaching farther are bevelled; 4 by default"),
                Field::new("rl", FieldType::Number { unit: "m" }, "Fixed elevation; the source's by default"),
                Field::new("layer", FieldType::Layer, "Layer for the new string; the source's by default"),
                Field::new("name", FieldType::Text, "Optional"),
                Field::new("times", FieldType::Integer, "How many strings, each the distance further out; 1 by default"),
            ],
            outputs: vec![Field::object("strings", ObjectKind::String, "The offset strings, one per piece")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        if let Some(rl) = p.rl {
            finite(&[0.0, 0.0, rl])?;
        }
        let side = match p.side {
            SideParam::Left => OffsetSide::Left,
            SideParam::Right => OffsetSide::Right,
        };
        let corner = match p.corners {
            CornerParam::Mitre => Corner::Mitre { limit: p.mitre_limit },
            CornerParam::Round => Corner::Round,
        };
        if p.times == 0 {
            return Err(CommandError::Invalid("An offset needs to be made at least once".into()));
        }
        let side_word = match p.side {
            SideParam::Left => "left",
            SideParam::Right => "right",
        };
        let layer = p.layer.clone().unwrap_or_else(|| inputs.layer_name(object));
        let mut changes = Vec::new();
        let mut vertices = 0;
        for time in 1..=p.times {
            let distance = p.distance * f64::from(time);
            let pieces = offset_string(line, distance, side, corner, p.rl).map_err(|e| CommandError::Invalid(if p.times > 1 { format!("at {distance} m: {e}") } else { e.to_string() }))?;
            let base = match &p.name {
                Some(name) if p.times > 1 => format!("{name} {time}"),
                Some(name) => name.clone(),
                None => format!("{} offset {distance} m {side_word}", object.name),
            };
            let count = pieces.len();
            vertices += pieces.iter().map(|l| l.vertices.len()).sum::<usize>();
            changes.extend(pieces.into_iter().enumerate().map(|(i, piece)| Change::Create {
                name: if count == 1 { base.clone() } else { format!("{base} ({})", i + 1) },
                layer: layer.clone(),
                attributes: Attributes::new(),
                body: Body::String(piece),
            }));
        }
        let message = format!(
            "offset {} m to the {side_word}{} with {} corners: {} string(s), {vertices} vertices",
            p.distance,
            if p.times > 1 { format!(", {} times", p.times) } else { String::new() },
            if matches!(p.corners, CornerParam::Round) { "round" } else { "mitred" },
            changes.len(),
        );
        Ok(Outcome::new(changes).message(message))
    }
}

fn default_step() -> f64 {
    1.0
}

fn default_attribute() -> String {
    "strip".into()
}

fn default_number_name() -> String {
    "Strip #".into()
}

/// `string.number_across`: strings numbered in the order a line drawn across them crosses them, as a pit's
/// strips are numbered by direction. Each takes its number as an attribute, and a name with it in. A string
/// the line doesn't cross stays as it is, and one it crosses more than once goes by the first.
pub struct NumberStrings;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumberStringsParams {
    pub strings: Vec<ObjectId>,
    /// The two ends of the line, in plan. Numbers run from `from` to `to`.
    pub from: [f64; 3],
    pub to: [f64; 3],
    pub start: f64,
    /// Negative to count down.
    #[serde(default = "default_step")]
    pub step: f64,
    /// The attribute that holds the number.
    #[serde(default = "default_attribute")]
    pub attribute: String,
    /// Each string's name, with its number in place of `#`, or on the end if there's no `#`.
    #[serde(default = "default_number_name")]
    pub name: String,
}

impl NumberStringsParams {
    fn name_of(&self, number: f64) -> String {
        if self.name.contains('#') { self.name.replace('#', &number.to_string()) } else { format!("{} {number}", self.name) }
    }
}

impl Command for NumberStrings {
    type Params = NumberStringsParams;
    const NAME: &'static str = "string.number_across";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Number strings in the order a line crosses them",
            inputs: vec![Field::new("strings", FieldType::Objects, "The strings to number")],
            params: vec![
                Field::new("from", FieldType::Point, "Where the line starts, and the numbers with it; plan only"),
                Field::new("to", FieldType::Point, "Where the line ends; plan only"),
                Field::new("start", FieldType::Number { unit: "" }, "The first number"),
                Field::new("step", FieldType::Number { unit: "" }, "Added each time, negative to count down; 1 by default"),
                Field::new("attribute", FieldType::Text, "The attribute that holds the number; strip by default"),
                Field::new("name", FieldType::Text, "Each string's name, with its number in place of #; Strip # by default"),
            ],
            outputs: vec![Field::object("strings", ObjectKind::String, "The strings numbered")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        finite(&p.from)?;
        finite(&p.to)?;
        finite(&[p.start, p.step, 0.0])?;
        if p.step == 0.0 {
            return Err(CommandError::Invalid("A step of zero would give every string the same number".into()));
        }
        if p.attribute.trim().is_empty() {
            return Err(CommandError::Invalid("The number needs an attribute to go in".into()));
        }
        let across = Polyline { vertices: vec![p.from, p.to], closed: false };
        if across.plan_length() == 0.0 {
            return Err(CommandError::Invalid("The line's two ends are the same point in plan".into()));
        }
        let mut crossed = Vec::new();
        let mut missed = Vec::new();
        let mut messages = Vec::new();
        for &id in &p.strings {
            let (object, line) = inputs.string(id)?;
            let crossings = plan_crossings(&across, line);
            match crossings.first() {
                Some(first) => crossed.push((first.along, object)),
                None => missed.push(object.name.as_str()),
            }
            if crossings.len() > 1 {
                messages.push(format!("{} crosses the line {} times: numbered where it first does", object.name, crossings.len()));
            }
        }
        if crossed.is_empty() {
            return Err(CommandError::Invalid("The line doesn't cross any of these strings".into()));
        }
        crossed.sort_by(|a, b| a.0.total_cmp(&b.0));
        let number = |k: usize| p.start + k as f64 * p.step;
        let changes = crossed.iter().enumerate().map(|(k, (_, object))| {
            let mut attributes = object.attributes.clone();
            attributes.insert(p.attribute.clone(), AttrValue::Number(number(k)));
            Change::Revise { id: object.id, base: object.revision, name: Some(p.name_of(number(k))), layer: None, attributes: Some(attributes), body: None }
        });
        let mut outcome = Outcome::new(changes.collect()).message(format!("Numbered {} strings from {} to {}", crossed.len(), number(0), number(crossed.len() - 1)));
        if !missed.is_empty() {
            outcome = outcome.message(format!("The line didn't cross {}: left as they were", missed.join(", ")));
        }
        for message in messages {
            outcome = outcome.message(message);
        }
        Ok(outcome)
    }
}

/// `string.drape`: the string laid on a surface or elevation grid. Each vertex takes the surface's
/// elevation under it; with `crossings` (the default) a vertex is added wherever the string crosses a
/// triangle edge, so it follows the surface between vertices. Vertices with no surface under them keep
/// their elevations and are counted. A new revision of the string.
pub struct DrapeString;

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrapeStringParams {
    pub string: ObjectId,
    pub surface: ObjectId,
    #[serde(default = "yes")]
    pub crossings: bool,
}

impl Command for DrapeString {
    type Params = DrapeStringParams;
    const NAME: &'static str = "string.drape";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Drape a string onto a surface",
            inputs: vec![string_input(), Field::object("surface", ObjectKind::Surface, "A surface or elevation grid")],
            params: vec![Field::new("crossings", FieldType::Bool, "Add vertices where the string crosses triangle edges")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        let target = inputs.object(p.surface)?;
        let triangulated;
        let mesh = match &target.body {
            Body::Surface(s) => s.mesh(),
            Body::Grid(g) => {
                triangulated = triangulate_grid(g);
                &triangulated
            }
            other => return Err(CommandError::WrongKind { id: p.surface, expected: ObjectKind::Surface, found: other.kind() }),
        };
        let (draped, log) = drape_string(line, mesh, p.crossings).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let mut message = format!("draped on {}: {} crossings added", target.name, log.crossings_added);
        if log.off_surface > 0 {
            message += &format!("; {} vertices have no surface under them and keep their elevations", log.off_surface);
        }
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(draped))]).message(message))
    }
}

/// `string.set_elevation`: vertices `from` to `to` (all by default) set to one elevation. A new
/// revision of the string.
pub struct SetElevation;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetElevationParams {
    pub string: ObjectId,
    pub rl: f64,
    #[serde(default)]
    pub from: Option<usize>,
    #[serde(default)]
    pub to: Option<usize>,
}

impl Command for SetElevation {
    type Params = SetElevationParams;
    const NAME: &'static str = "string.set_elevation";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Set a string's vertices to one elevation",
            inputs: vec![string_input()],
            params: vec![
                Field::new("rl", FieldType::Number { unit: "m" }, "The elevation"),
                Field::new("from", FieldType::Integer, "First vertex; the first by default"),
                Field::new("to", FieldType::Integer, "Last vertex; the last by default"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        finite(&[0.0, 0.0, p.rl])?;
        let from = p.from.unwrap_or(0);
        let to = vertex_range(line, from, p.to)?;
        let mut edited = line.clone();
        for v in &mut edited.vertices[from.min(to)..=from.max(to)] {
            v[2] = p.rl;
        }
        let message = format!("vertices {} to {} set to RL {:.3}", from.min(to) + 1, from.max(to) + 1, p.rl);
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(edited))]).message(message))
    }
}

/// `string.reverse`: the string's direction reversed (which side is left, which way a grade runs).
pub struct ReverseString;

/// `string.set_closed`: the string closed (its last vertex joined back to its first) or opened.
pub struct SetClosed;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StringParams {
    pub string: ObjectId,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetClosedParams {
    pub string: ObjectId,
    pub closed: bool,
}

impl Command for ReverseString {
    type Params = StringParams;
    const NAME: &'static str = "string.reverse";

    fn spec(&self) -> CommandSpec {
        CommandSpec { name: Self::NAME, summary: "Reverse a string's direction", inputs: vec![string_input()], params: vec![], outputs: vec![] }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        let mut edited = line.clone();
        edited.vertices.reverse();
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(edited))]))
    }
}

impl Command for SetClosed {
    type Params = SetClosedParams;
    const NAME: &'static str = "string.set_closed";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Close or open a string",
            inputs: vec![string_input()],
            params: vec![Field::new("closed", FieldType::Bool, "Join the last vertex back to the first")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        if line.closed == p.closed {
            return Err(CommandError::Invalid(format!("'{}' is already {}", object.name, if p.closed { "closed" } else { "open" })));
        }
        let edited = Polyline { vertices: line.vertices.clone(), closed: p.closed };
        check_line(&edited)?;
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(edited))]))
    }
}

/// `string.split`: an open string split at an inner vertex: it keeps the first part, and the second
/// (from that vertex on) becomes a new string on the same layer. A closed string opens there instead.
pub struct SplitString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitStringParams {
    pub string: ObjectId,
    pub at: usize,
}

impl Command for SplitString {
    type Params = SplitStringParams;
    const NAME: &'static str = "string.split";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Split a string at a vertex",
            inputs: vec![string_input()],
            params: vec![Field::new("at", FieldType::Integer, "The vertex to split at")],
            outputs: vec![Field::object("second", ObjectKind::String, "The second part (none when a closed string opens)")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        let (first, second) = split_string(line, p.at).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let mut changes = vec![Change::revise_body(object, Body::String(first.clone()))];
        let message = match second {
            None => format!("opened at vertex {}", p.at + 1),
            Some(second) => {
                let message = format!("split at vertex {}: {} and {} vertices", p.at + 1, first.vertices.len(), second.vertices.len());
                changes.push(Change::Create {
                    name: format!("{} part 2", object.name),
                    layer: inputs.layer_name(object),
                    attributes: object.attributes.clone(),
                    body: Body::String(second),
                });
                message
            }
        };
        Ok(Outcome::new(changes).message(message))
    }
}

/// `string.join`: another open string joined onto this one at their nearest ends, in this one's
/// direction. This string keeps the result and the other is deleted (undo brings it back). Ends
/// that do not meet are bridged by a segment, and the gap is reported.
pub struct JoinStrings;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinStringsParams {
    pub string: ObjectId,
    pub other: ObjectId,
}

impl Command for JoinStrings {
    type Params = JoinStringsParams;
    const NAME: &'static str = "string.join";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Join another string onto this one",
            inputs: vec![string_input(), Field::object("other", ObjectKind::String, "The string to join on; it is deleted")],
            params: vec![],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.string == p.other {
            return Err(CommandError::Invalid("a string cannot be joined to itself; close it instead".into()));
        }
        let (object, line) = inputs.string(p.string)?;
        let (other, other_line) = inputs.string(p.other)?;
        let (joined, gap) = join_strings(line, other_line).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let message = if gap == 0.0 {
            format!("joined '{}' on: {} vertices", other.name, joined.vertices.len())
        } else {
            format!("joined '{}' on, bridging a {gap:.3} m gap: {} vertices", other.name, joined.vertices.len())
        };
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(joined)), Change::Delete { id: other.id, base: other.revision }]).message(message))
    }
}

fn default_spacing() -> f64 {
    5.0
}

/// `string.smooth`: a smooth curve through every vertex (centripetal Catmull-Rom), with vertices
/// added so no step is longer than `spacing` in plan. The original vertices stay where they are. A
/// new revision of the string.
pub struct SmoothString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmoothStringParams {
    pub string: ObjectId,
    #[serde(default = "default_spacing")]
    pub spacing: f64,
}

impl Command for SmoothString {
    type Params = SmoothStringParams;
    const NAME: &'static str = "string.smooth";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Smooth a string through its vertices",
            inputs: vec![string_input()],
            params: vec![Field::new("spacing", FieldType::Number { unit: "m" }, "Longest step along the curve in plan; 5 m by default")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        let smoothed = smooth_string(line, p.spacing).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let message = format!("smoothed at {} m: {} vertices, was {}", p.spacing, smoothed.vertices.len(), line.vertices.len());
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(smoothed))]).message(message))
    }
}

/// `string.filter`: vertices within `tolerance` metres (in 3D) of the line through their kept
/// neighbours removed (Douglas-Peucker); the ends of an open string stay. A new revision.
pub struct FilterString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterStringParams {
    pub string: ObjectId,
    pub tolerance: f64,
}

impl Command for FilterString {
    type Params = FilterStringParams;
    const NAME: &'static str = "string.filter";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Remove vertices within a tolerance of the line",
            inputs: vec![string_input()],
            params: vec![Field::new("tolerance", FieldType::Number { unit: "m" }, "Largest 3D distance a removed vertex may be from the line")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        let filtered = filter_string(line, p.tolerance).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let removed = line.vertices.len() - filtered.vertices.len();
        let message = format!("filtered at {} m: {removed} of {} vertices removed", p.tolerance, line.vertices.len());
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(filtered))]).message(message))
    }
}
