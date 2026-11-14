//! Object types for the Zovlyn Pit Design Engine.
//!
//! Geometry (triangulated meshes, grids, explicit validity flags), the object model around it
//! (ids, layers, objects and their revisions), and the project that holds every revision with an
//! undo history. Commands live in `zd-commands`.

mod attributes;
mod bounds;
mod display;
mod face;
mod grid;
mod id;
mod mesh;
mod object;
mod project;
mod seam;
mod validity;
mod workflow;

pub use attributes::intern;
pub use bounds::Aabb;
pub use display::{Colouring, Contours, Display, Look};
pub use face::{AzimuthSector, FaceDesign, FaceDirection, FaceLine, FaceProfile, HorizonValue, LineEdit, LineOn, MOST_TOLERANCE, ProfileStep, Ramp, RampBerm, RampFixed, RampHeading, RuleValue, StepTarget, StepValue, USUAL_TURN_JOIN, WallRule, WallSide};
pub use grid::Grid;
pub use id::{LayerId, ObjectId, RunId};
pub use mesh::TriMesh;
pub use object::{AttrValue, Attributes, Body, Layer, Object, ObjectKind, ObjectRef, Pieces, Polyline, Solid, Surface};
pub use project::{Applied, AppliedChange, Change, ChangeKind, Project, ProjectError};
pub use seam::{Fingerprint, GridLink, Horizon, IssueKind, Lattice, SeamModel, StackIssue, blank_no_seam, blank_no_thickness, grid_holes, moved_path, relative_path, same_path};
pub use validity::{CrossingCheck, Crossings, MeshValidity, is_zero_area};
pub use workflow::{BASE_CASE, Case, InputRule, LastRun, Source, StepAction, Workflow, WorkflowStep};
