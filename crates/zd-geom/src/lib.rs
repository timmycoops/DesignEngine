//! Geometry for the Zovlyn Pit Design Engine.
//!
//! Grid triangulation and resampling, seam solids between roof and floor grids, closing a design
//! surface into a half-space, the Manifold cut, BVH ray casting for snapping, the seam stack check,
//! string geometry, section lines, and the import repair step with its crossing check.

mod between;
mod benches;
mod blocks;
mod layers;
mod close;
mod crossings;
mod drape;
mod cut;
mod error;
mod face;
mod grid;
mod halfspace;
mod orient;
mod polygons;
mod polygon_ops;
mod compare_surfaces;
mod merge_surfaces;
mod trim_in_plan;
mod project;
mod ramp;
mod ray;
mod repair;
mod reserves;
mod road;
mod rule;
mod self_union;
mod sheet;
mod seam;
mod section_fill;
mod section_snap;
mod slice;
mod stack;
mod fillet;
mod from_surfaces;
mod string;
mod string_cut;
mod strips;
mod transform;
mod vertices;
mod wall;

pub use benches::{BenchCut, BenchPiece, MAX_LEVELS, bench_levels, cut_solid_by_levels};
pub use blocks::{COVER_SLIVER, PolygonCut, PolygonPiece, Uncovered, cut_solid_by_polygons, polygons_overlapping, polygons_uncovered};
pub use layers::{GridCover, GridHole, LayerCut, LayerPiece, LayerStack, cut_solid_by_layers, grid_hole_patches, layer_stack, solid_off_the_grids};
pub use between::{Between, solid_between};
pub use cut::{CutResult, CutTimings, DesignIndex, TINY_EDGE, TinyEdges, cut_solid_by_indexed_surface, cut_solid_by_surface, cut_solid_by_whole_surface, join_tiny_edges};
pub use error::GeomError;
pub use grid::{SampledGrid, sample_surface_on_grid, triangulate_grid};
pub use halfspace::{CapMethod, ClosedHalfSpace, Side, close_surface_to_halfspace};
pub use close::{CloseLog, close_solid};
pub use drape::{GridDrape, drape_on_grids};
pub use crossings::{COINCIDENT, find_crossings, find_solid_crossings};
pub use polygons::{JoinedEnd, LinePolygon, LinePolygons, ShortLineEnd, polygons_from_lines};
pub use merge_surfaces::{Envelope, MOST_MERGE_NODES, MergeInput, MergedGrid, MergedSurface, merge_to_grid, merge_to_surface, solid_skin};
pub use trim_in_plan::{GridTrim, InPlan, ON_THE_TRIM, SolidTrim, SurfaceTrim, mesh_plan_area, trim_grid_in_plan, trim_ring, trim_solid_in_plan, trim_string_in_plan, trim_surface_in_plan};
pub use compare_surfaces::{COLUMN_CHECK, MOST_COLUMN_NODES, SurfaceCompare, compare_surfaces};
pub use polygon_ops::{ON_A_POLYGON, PolygonBoolean, PolygonOp, PolygonRing, polygon_boolean};
pub use orient::{OrientLog, SITES, Separation, boundary_loops, orient_patches, orient_upward, separate_touching_edges, split_patches};
pub use repair::{RepairLog, closed_as_written, repair_mesh};
pub use rule::{EdgeRule, SolidRegion};
pub use self_union::{UnionLog, self_union};
pub use ray::SurfaceRayCaster;
pub use seam::{SeamLog, SeamSolid, build_seam_solid};
pub use section_fill::{SectionFill, fill_between_lines, fill_section_loops};
pub use section_snap::{CutSegment, SectionSnap, SectionSnapKind, snap_to_section};
pub use from_surfaces::{Contour, MAX_CONTOUR_LEVELS, Meeting, contour_levels, contours, plan_area, plan_length, solid_outline, surface_outline, where_surfaces_meet};
pub use slice::{SectionPlane, SliceIndex, SliceLine, slice_indexed, slice_mesh, string_near_plane};
pub use stack::{StackHorizon, SuggestedHorizon, check_seam_stack, stack_issue_nodes, suggest_no_seam_rl, suggest_seam_tree};
pub use face::{EditReport, Face, FaceBench, FaceEdit, FaceStep, FaceTarget, JoinBoxes, JoinNote, MetBoxes, RegisterLog, TooClose, StepReport, WallToCome, at_line_heights, conditioned_toe, level_floor, project_edited_face, project_face, register_toe, run_on};
pub use reserves::{Composite, EVERY_RL, NodeColumn, Sample, Variable, cells_in_solid, column_volume, column_volume_within, columns_in_solid, composite, horizon_samples, in_situ_rd, mid_seam_cells, mid_seam_cells_within, thickness_between};
pub use project::{ProjectTo, ProjectedString, project_string};
pub use ramp::{RampPolygon, RampShort, RampTo, ramp_polygon};
pub use road::{FaceRamp, FaceRoad, NoRoad, RoadTop};
pub use sheet::{Across, sheet_between, sheets_as_one, strings_across};
pub use wall::{ConditionLog, Stretch, WallLog, WallStep, next_toe, project_wall, project_wall_by_edge};
pub use strips::{CLEAR_OF_SOLID, Crossing, FaceCut, FaceShort, FilledGap, JoinLog, ShortEnd, Shortfall, SliceRefusal, SliceTimings, StripPiece, StripSlice, faces_through_solid, join_faces, place, short_words, shortfall_words, slice_solid_by_faces, slice_solid_by_faces_watched};
pub use string_cut::{Extended, Keep, Trimmed, clip_string, extend_string, split_string_at, trim_string};
pub use fillet::{Rounded, fillet_string};
pub use transform::{Transform, move_grid, transform_crossings, transform_mesh, transform_string};
pub use vertices::{delete_vertices, densify_string, grade_vertices, move_vertices, picked_in_order, set_vertices_rl, vertices_at_crossings};
pub use string::{Corner, DrapeLog, MOST_LOOSE_PIECES, OffsetSide, ON_THE_STRING, PieceJoin, PlanCrossing, branching_ends, drape_string, filter_string, grade_string, join_pieces, join_strings, offset_string, plan_crossings, redraw_stretch, smooth_string, split_string};

/// Below this many triangles a search stays on one thread.
const ONE_THREAD_BELOW: usize = 2_000;

/// Threads for a search over `triangles`. A small mesh stays on one: starting threads costs more
/// than it saves, and a run of thousands of solids is already spread across the cores.
pub(crate) fn worker_threads(triangles: usize) -> usize {
    if triangles < ONE_THREAD_BELOW { 1 } else { std::thread::available_parallelism().map_or(1, |n| n.get()) }
}
