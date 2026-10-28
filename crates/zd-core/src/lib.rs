//! Object types for the Zovlyn Pit Design Engine.
//!
//! Phase 0 holds only what the stress test needs: a triangulated mesh, a regular grid, and the
//! explicit validity flags every mesh carries. Ids, layers and provenance arrive with the command
//! layer in Phase 1.

mod bounds;
mod grid;
mod mesh;
mod validity;

pub use bounds::Aabb;
pub use grid::Grid;
pub use mesh::TriMesh;
pub use validity::{MeshValidity, is_zero_area};
