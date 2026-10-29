//! Geometry for the Zovlyn Pit Design Engine.
//!
//! Phase 0 covers what the stress test needs: grid triangulation and resampling, seam solids
//! between roof and floor grids, closing a design surface into a half-space, the Manifold cut,
//! and BVH ray casting for snapping.

mod cut;
mod error;
mod grid;
mod halfspace;
mod orient;
mod ray;
mod seam;
mod stack;

pub use cut::{CutResult, CutTimings, cut_solid_by_surface};
pub use error::GeomError;
pub use grid::{SampledGrid, sample_surface_on_grid, triangulate_grid};
pub use halfspace::{CapMethod, ClosedHalfSpace, Side, close_surface_to_halfspace};
pub use orient::{boundary_loops, orient_upward};
pub use ray::SurfaceRayCaster;
pub use seam::{SeamLog, SeamSolid, build_seam_solid};
pub use stack::{StackHorizon, SuggestedHorizon, check_seam_stack, stack_issue_nodes, suggest_seam_tree};
