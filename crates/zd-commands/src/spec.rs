use serde::Serialize;
use zd_core::ObjectKind;

/// What a command takes and makes: enough to build its form and to check a call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommandSpec {
    pub name: &'static str,
    pub summary: &'static str,
    /// Objects the command reads, passed by id in the parameters.
    pub inputs: Vec<Field>,
    pub params: Vec<Field>,
    pub outputs: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    pub name: &'static str,
    #[serde(rename = "type")]
    pub ty: FieldType,
    pub description: &'static str,
}

impl Field {
    pub fn object(name: &'static str, kind: ObjectKind, description: &'static str) -> Field {
        Field { name, ty: FieldType::Object { kind }, description }
    }

    pub fn new(name: &'static str, ty: FieldType, description: &'static str) -> Field {
        Field { name, ty, description }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldType {
    Object {
        #[serde(rename = "object_kind")]
        kind: ObjectKind,
    },
    /// A list of objects by id, of the kinds the command's description names.
    Objects,
    /// A real number with its unit ("m", "%", "deg").
    Number { unit: &'static str },
    Integer,
    Text,
    Bool,
    /// One `[x, y, z]` in mine grid coordinates.
    Point,
    /// A list of `[x, y, z]`.
    Points,
    /// A layer, by name; created if it does not exist.
    Layer,
    /// A file path.
    Path,
}
