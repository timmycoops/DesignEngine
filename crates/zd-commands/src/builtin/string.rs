use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ObjectId, ObjectKind, Polyline};
use zd_geom::{Corner, OffsetSide, drape_string, grade_string, offset_string, triangulate_grid};

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

/// `string.offset`: a new string `distance` metres in plan to the left or right of this one (looking
/// along it from above), with mitred or round outside corners. Each new vertex takes the elevation of
/// the nearest point on the source, or `rl`. The source is not changed; an offset that pinches off
/// makes one string per piece.
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
            ],
            outputs: vec![Field::object("strings", ObjectKind::String, "The offset string, one per piece")],
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
        let pieces = offset_string(line, p.distance, side, corner, p.rl).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let side_word = match p.side {
            SideParam::Left => "left",
            SideParam::Right => "right",
        };
        let base = p.name.clone().unwrap_or_else(|| format!("{} offset {} m {side_word}", object.name, p.distance));
        let layer = p.layer.clone().unwrap_or_else(|| inputs.layer_name(object));
        let count = pieces.len();
        let message = format!(
            "offset {} m to the {side_word} with {} corners: {count} string(s), {} vertices",
            p.distance,
            if matches!(p.corners, CornerParam::Round) { "round" } else { "mitred" },
            pieces.iter().map(|l| l.vertices.len()).sum::<usize>()
        );
        let changes = pieces
            .into_iter()
            .enumerate()
            .map(|(i, piece)| Change::Create {
                name: if count == 1 { base.clone() } else { format!("{base} ({})", i + 1) },
                layer: layer.clone(),
                attributes: Attributes::new(),
                body: Body::String(piece),
            })
            .collect();
        Ok(Outcome::new(changes).message(message))
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
