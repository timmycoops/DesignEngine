use std::fmt;

use zd_core::{MeshValidity, ObjectId, ObjectKind, ProjectError};

use crate::Code;

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
    /// Saving or opening the project folder failed.
    Store(String),
    /// Stopped by whoever started it, with nothing applied (decision 0060).
    Cancelled,
    /// The project changed while the run read it, so its changes weren't applied (decision 0060).
    Changed,
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
            CommandError::Store(message) => write!(f, "project store: {message}"),
            CommandError::Cancelled => write!(f, "cancelled; nothing was changed"),
            CommandError::Changed => write!(f, "the project changed while this ran, so nothing was changed; run it again"),
        }
    }
}

impl std::error::Error for CommandError {}

impl CommandError {
    /// The kind of refusal, as a code a script or the app can rely on (decision 0059).
    pub fn code(&self) -> Code {
        match self {
            CommandError::UnknownCommand(_) => Code::RefusedUnknownCommand,
            CommandError::BadParams(_) => Code::RefusedBadParams,
            CommandError::MissingObject(_) => Code::RefusedMissingObject,
            CommandError::WrongKind { .. } => Code::RefusedWrongKind,
            CommandError::Invalid(_) => Code::RefusedInvalid,
            CommandError::NotWatertight(_) => Code::RefusedNotWatertight,
            CommandError::Geometry(_) => Code::RefusedGeometry,
            CommandError::Io(_) => Code::RefusedIo,
            CommandError::Project(_) => Code::RefusedProject,
            CommandError::Store(_) => Code::RefusedStore,
            CommandError::Cancelled => Code::RefusedCancelled,
            CommandError::Changed => Code::RefusedChanged,
        }
    }
}

impl From<ProjectError> for CommandError {
    fn from(e: ProjectError) -> Self {
        CommandError::Project(e)
    }
}
