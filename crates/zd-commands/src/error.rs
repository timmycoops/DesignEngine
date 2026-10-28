use std::fmt;

use zd_core::{MeshValidity, ObjectId, ObjectKind, ProjectError};

#[derive(Debug, Clone, PartialEq)]
pub enum CommandError {
    UnknownCommand(String),
    /// The parameters did not match the command's (from the JSON decoder).
    BadParams(String),
    MissingObject(ObjectId),
    WrongKind { id: ObjectId, expected: ObjectKind, found: ObjectKind },
    /// Parameters that parse but make no sense (an index past the end, too few points).
    Invalid(String),
    NotWatertight(MeshValidity),
    Geometry(String),
    Io(String),
    /// The project refused the changes (stale input, duplicate layer, ...).
    Project(ProjectError),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::UnknownCommand(name) => write!(f, "no command named '{name}'"),
            CommandError::BadParams(message) => write!(f, "bad parameters: {message}"),
            CommandError::MissingObject(id) => write!(f, "no live object {id}"),
            CommandError::WrongKind { id, expected, found } => {
                write!(f, "object {id} is a {found:?}, not a {expected:?}")
            }
            CommandError::Invalid(message) => write!(f, "{message}"),
            CommandError::NotWatertight(v) => write!(
                f,
                "mesh is not watertight: {} boundary, {} non-manifold, {} inconsistently wound edges",
                v.boundary_edges, v.non_manifold_edges, v.inconsistent_edges
            ),
            CommandError::Geometry(message) => write!(f, "geometry: {message}"),
            CommandError::Io(message) => write!(f, "{message}"),
            CommandError::Project(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CommandError {}

impl From<ProjectError> for CommandError {
    fn from(e: ProjectError) -> Self {
        CommandError::Project(e)
    }
}
