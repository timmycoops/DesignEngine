//! Geometry for the Zovlyn Pit Design Engine.
//!
//! Grid triangulation and resampling, seam solids between roof and floor grids, closing a design
//! surface into a half-space, the Manifold cut, BVH ray casting for snapping, the seam stack check,
//! string geometry, and the import repair step with its crossing check.

mod close;
mod crossings;
mod cut;
mod error;
mod grid;
mod halfspace;
mod orient;
mod ray;
mod repair;
mod self_union;
mod seam;
mod stack;
mod string;

pub use cut::{CutResult, CutTimings, cut_solid_by_surface};
pub use error::GeomError;
pub use grid::{SampledGrid, sample_surface_on_grid, triangulate_grid};
pub use halfspace::{CapMethod, ClosedHalfSpace, Side, close_surface_to_halfspace};
pub use close::{CloseLog, close_solid};
pub use crossings::find_crossings;
pub use orient::{OrientLog, SITES, Separation, boundary_loops, orient_patches, orient_upward, separate_touching_edges, split_patches};
pub use repair::{RepairLog, repair_mesh};
pub use self_union::{UnionLog, self_union};
pub use ray::SurfaceRayCaster;
pub use seam::{SeamLog, SeamSolid, build_seam_solid};
pub use stack::{StackHorizon, SuggestedHorizon, check_seam_stack, stack_issue_nodes, suggest_seam_tree};
pub use string::{Corner, DrapeLog, OffsetSide, drape_string, filter_string, grade_string, join_strings, offset_string, smooth_string, split_string};
