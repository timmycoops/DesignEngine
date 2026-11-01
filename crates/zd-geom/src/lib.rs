//! Geometry for the Zovlyn Pit Design Engine.
//!
//! Grid triangulation and resampling, seam solids between roof and floor grids, closing a design
//! surface into a half-space, the Manifold cut, BVH ray casting for snapping, the seam stack check,
//! string geometry, section lines, and the import repair step with its crossing check.

mod close;
mod crossings;
mod cut;
mod error;
mod face;
mod grid;
mod halfspace;
mod orient;
mod ray;
mod repair;
mod rule;
mod self_union;
mod seam;
mod section_fill;
mod section_snap;
mod slice;
mod stack;
mod string;
mod wall;

pub use cut::{CutResult, CutTimings, cut_solid_by_surface, cut_solid_by_whole_surface};
pub use error::GeomError;
pub use grid::{SampledGrid, sample_surface_on_grid, triangulate_grid};
pub use halfspace::{CapMethod, ClosedHalfSpace, Side, close_surface_to_halfspace};
pub use close::{CloseLog, close_solid};
pub use crossings::{COINCIDENT, find_crossings, find_solid_crossings};
pub use orient::{OrientLog, SITES, Separation, boundary_loops, orient_patches, orient_upward, separate_touching_edges, split_patches};
pub use repair::{RepairLog, repair_mesh};
pub use rule::{EdgeRule, SolidRegion};
pub use self_union::{UnionLog, self_union};
pub use ray::SurfaceRayCaster;
pub use seam::{SeamLog, SeamSolid, build_seam_solid};
pub use section_fill::{SectionFill, fill_between_lines, fill_section_loops};
pub use section_snap::{CutSegment, SectionSnap, SectionSnapKind, snap_to_section};
pub use slice::{SectionPlane, SliceIndex, SliceLine, slice_indexed, slice_mesh, string_near_plane};
pub use stack::{StackHorizon, SuggestedHorizon, check_seam_stack, stack_issue_nodes, suggest_no_seam_rl, suggest_seam_tree};
pub use face::{Face, FaceBench, FaceStep, FaceTarget, RegisterLog, StepReport, conditioned_toe, project_face, register_toe, run_on};
pub use wall::{ConditionLog, Stretch, WallLog, WallStep, next_toe, project_wall, project_wall_by_edge};
pub use string::{Corner, DrapeLog, OffsetSide, drape_string, filter_string, grade_string, join_strings, offset_string, smooth_string, split_string};

/// Below this many triangles a search stays on one thread.
const ONE_THREAD_BELOW: usize = 2_000;

/// Threads for a search over `triangles`. A small mesh stays on one: starting threads costs more
/// than it saves, and a run of thousands of solids is already spread across the cores.
pub(crate) fn worker_threads(triangles: usize) -> usize {
    if triangles < ONE_THREAD_BELOW { 1 } else { std::thread::available_parallelism().map_or(1, |n| n.get()) }
}
