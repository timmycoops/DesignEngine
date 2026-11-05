use std::fmt;

use zd_core::MeshValidity;

#[derive(Debug)]
pub enum GeomError {
    InvalidInput(String),
    /// A mesh that must be watertight is not; carries the computed flags.
    NotWatertight { what: &'static str, validity: MeshValidity },
    Manifold(String),
    /// A wall left its target and the target can't be bridged there.
    OffTarget { at: [f64; 2] },
    /// The string is on or above the target all the way along.
    NoHeight,
    /// What was asked for can't be built as asked, and nothing else is tried: a line redrawn by hand that crosses the line
    /// below it, say. The words are for the person who asked.
    Refused(String),
}

impl fmt::Display for GeomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeomError::InvalidInput(message) => write!(f, "invalid input: {message}"),
            GeomError::NotWatertight { what, validity: v } => write!(
                f,
                "{what} is not watertight: {} boundary, {} non-manifold, {} inconsistently wound edges",
                v.boundary_edges, v.non_manifold_edges, v.inconsistent_edges
            ),
            GeomError::Manifold(message) => write!(f, "manifold: {message}"),
            GeomError::OffTarget { at } => write!(f, "the wall runs off the target at {:.3}, {:.3}", at[0], at[1]),
            GeomError::NoHeight => write!(f, "the string is on or above the target all the way along, so there's no wall to make: pick a surface above it"),
            GeomError::Refused(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for GeomError {}
