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
mod tin;
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
pub use tin::{LONGEST_HULL_EDGE, SAME_HEIGHT, TinBound, TinCrossing, TinRefusal, Triangulated, triangulate_strings};
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

/// `work` on each of `0..count`, at most `at_once` at a time, the results in order. For work that holds a lot of memory
/// each, as a Manifold boolean on a large solid: started all at once, a hundred of them run the machine out of memory,
/// and Manifold spreads each one across the cores anyway.
pub(crate) fn at_most<R: Send>(count: usize, at_once: usize, work: impl Fn(usize) -> R + Sync) -> Vec<R> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    let (next, work) = (&next, &work);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..at_once.clamp(1, count.max(1)))
            .map(|_| {
                scope.spawn(move || {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= count {
                            return mine;
                        }
                        mine.push((i, work(i)));
                    }
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("work run in turns doesn't panic")).collect()
    });
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

#[cfg(test)]
mod at_most_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::at_most;

    /// Forty jobs three at a time: never more than three running, every one run once, the results in order.
    #[test]
    fn work_runs_at_most_so_many_at_a_time_and_comes_back_in_order() {
        let (running, most, runs) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0));
        let out = at_most(40, 3, |i| {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            runs.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            running.fetch_sub(1, Ordering::SeqCst);
            i * 10
        });
        assert_eq!(out, (0..40).map(|i| i * 10).collect::<Vec<_>>());
        assert_eq!(runs.load(Ordering::SeqCst), 40);
        assert!(most.load(Ordering::SeqCst) <= 3, "{} at once", most.load(Ordering::SeqCst));
        assert!(at_most(0, 4, |i| i).is_empty());
        assert_eq!(at_most(5, 0, |i| i), vec![0, 1, 2, 3, 4], "none at once is one at a time");
    }
}
