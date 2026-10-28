use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ObjectId, ObjectKind, Polyline};

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
