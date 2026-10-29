//! Object types for the Zovlyn Pit Design Engine.
//!
//! Geometry (triangulated meshes, grids, explicit validity flags), the object model around it
//! (ids, layers, objects and their revisions), and the project that holds every revision with an
//! undo history. Commands live in `zd-commands`.

mod bounds;
mod grid;
mod id;
mod mesh;
mod object;
mod project;
mod seam;
mod validity;

pub use bounds::Aabb;
pub use grid::Grid;
pub use id::{LayerId, ObjectId, RunId};
pub use mesh::TriMesh;
pub use object::{AttrValue, Attributes, Body, Layer, Object, ObjectKind, ObjectRef, Polyline, Solid, Surface};
pub use project::{Applied, AppliedChange, Change, ChangeKind, Project, ProjectError};
pub use seam::{Fingerprint, GridLink, Horizon, IssueKind, Lattice, SeamModel, StackIssue};
pub use validity::{MeshValidity, is_zero_area};
