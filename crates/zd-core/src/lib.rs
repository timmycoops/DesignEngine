//! Object types for the Zovlyn Pit Design Engine.
//!
//! Geometry (triangulated meshes, grids, explicit validity flags), the object model around it
//! (ids, layers, objects and their revisions), and the project that holds every revision with an
//! undo history. Commands live in `zd-commands`.

mod bounds;
mod face;
mod grid;
mod id;
mod mesh;
mod object;
mod project;
mod seam;
mod validity;

pub use bounds::Aabb;
pub use face::{AzimuthSector, FaceDesign, FaceDirection, FaceLine, FaceProfile, HorizonValue, LineOn, MOST_TOLERANCE, ProfileStep, RampHeading, RuleValue, StepTarget, StepValue, WallRule, WallSide};
pub use grid::Grid;
pub use id::{LayerId, ObjectId, RunId};
pub use mesh::TriMesh;
pub use object::{AttrValue, Attributes, Body, Layer, Object, ObjectKind, ObjectRef, Polyline, Solid, Surface};
pub use project::{Applied, AppliedChange, Change, ChangeKind, Project, ProjectError};
pub use seam::{Fingerprint, GridLink, Horizon, IssueKind, Lattice, SeamModel, StackIssue, blank_no_seam, blank_no_thickness};
pub use validity::{CrossingCheck, Crossings, MeshValidity, is_zero_area};
