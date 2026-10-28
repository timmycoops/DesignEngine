use std::fmt;

use zd_core::MeshValidity;

#[derive(Debug)]
pub enum GeomError {
    InvalidInput(String),
    /// A mesh that must be watertight is not; carries the computed flags.
    NotWatertight { what: &'static str, validity: MeshValidity },
    Manifold(String),
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
        }
    }
}

impl std::error::Error for GeomError {}
