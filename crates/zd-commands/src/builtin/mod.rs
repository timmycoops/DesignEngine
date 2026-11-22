//! The engine's own commands. Strings (workstream 6): create, vertex edits, grade, offset, drape,
//! elevation, reverse, close, split, join, smooth and filter. Exchange (workstream 4): strings in
//! from DXF and CSV, objects out to DXF and CSV. Deswik drawings in (decision 0036), Spry projects in (decision 0039),
//! and closed surfaces an import couldn't make solids fixed by repair (decision 0040).

mod attributes;
mod update_records;
mod benches;
mod blocks;
mod build;
mod compare;
mod deswik;
mod exchange;
mod face;
mod grid;
mod interrogate;
mod layer;
mod object;
mod project;
mod ramp;
mod seam;
mod sheet;
mod solid;
mod spry;
mod string;
mod strips;
mod surface;
mod workflow;
mod dataflow;
mod reserves_run;
mod formulas;
mod from_surfaces;
mod surface_compare;
mod surface_merge;
mod trim_in_plan;
mod surface_triangulate;
mod starter;
mod measure;

pub use benches::{BENCH_FLOOR, BENCH_TOP, BENCHED_FROM, BenchEnd, BenchRule, CutByBenches, CutByBenchesParams, rule_for};
pub use blocks::{CutByPolygons, CutByPolygonsParams, CutBySeams, CutBySeamsParams, PolygonsFromLines, PolygonsFromLinesParams, check_cut_by_polygons, check_cut_by_seams, horizon_order};
pub use compare::{CompareSolids, CompareSolidsParams};
pub use build::{BuildOp, BuildStep, BuildStepParams, Rebuild, RebuildParams};
pub use deswik::{ImportDeswik, ImportDeswikParams, MeshesAs};
pub use exchange::{ExportCsv, ExportDxf, ExportParams, ExportSolids, ExportSolidsParams, ImportStrings, ImportStringsParams, LayerInFile};
pub use face::{ProjectDesign, ProjectDesignParams, ProjectStep, ProjectStepParams, SaveDesign, SaveDesignParams, Step, StepParams};
pub use grid::{ImportVulcanSeam, ImportVulcanSeamParams};
pub use interrogate::{COLUMN_DELTA, COLUMN_VOLUME, INTERROGATED, Interrogate, InterrogateParams, names_seam};
pub use layer::{CreateLayer, CreateLayerParams, DeleteLayer, DeleteLayerParams, RecolourLayer, RecolourLayerParams, RenameLayer, RenameLayerParams};
pub use ramp::{RampFromStart, RampFromStartParams};
pub use project::{ProjectString, ProjectStringParams, ProjectToParam};
pub use object::{CopyObjects, CopyParams, DeleteObject, DeleteObjects, MoveObjects, MoveObjectsParams, ObjectParams, ObjectsParams, RenameObject, RenameParams, TransformObjects, TransformParam, TransformParams};
pub use sheet::{SurfaceBetweenStrings, SurfaceBetweenStringsParams};
pub use seam::{BuildSeamModel, BuildSeamModelParams, ModelLinks, RelinkParams, RelinkSeamModels, SuggestSeamDefinition, SuggestSeamDefinitionParams, seam_links};
pub(crate) use solid::check_solids_made;
pub use spry::{ImportSpry, ImportSpryParams};
pub use solid::{CutBySurface, CutBySurfaceParams, ImportSolid, ImportSolidParams, ImportSolids, ImportSolidsParams, RepairSolids, RepairSolidsParams, SolidFromMesh};
pub use string::{
    CornerParam, CreateString, CreateStringParams, DeleteVertex, DeleteVertexParams, DeleteVertices, DeleteVerticesParams, DensifyString, DensifyStringParams, MoveVertices, MoveVerticesParams,
    VerticesAtCrossings, VerticesAtCrossingsParams, DrapeString, DrapeStringParams, FilletString, FilletStringParams, FilterString,
    FilterStringParams, InsertVertex, InsertVertexParams, JoinStrings, JoinStringsParams, MoveVertex, MoveVertexParams, NumberStrings,
    NumberStringsParams, OffsetString, OffsetStringParams, RedrawStretch, RedrawStretchParams, ReverseString, SetClosed, SetClosedParams, SetElevation, SetElevationParams, SetGrade, SetGradeParams, SideParam,
    SmoothString, SmoothStringParams, SplitString, SplitStringParams, StringParams, ExtendString, ExtendStringParams, TrimKeep, TrimString, TrimStringParams, ClipKeep, ClipString, ClipStringParams, PolygonBoolean, PolygonBooleanParams, PolygonOperation,
};
pub use strips::{JoinFaces, JoinFacesParams, SliceByFaces, SliceByFacesParams, SolidBetween, SolidBetweenParams, check_slice};
pub use surface::{ImportSurface, ImportSurfaceParams, MeshParams, SurfaceFromMesh};
pub use workflow::{SaveWorkflow, SaveWorkflowParams, WORKFLOW_LAYER};
pub use surface_compare::{CompareSurfaces, CompareSurfacesParams, THICKNESS_ROLE};
pub use surface_merge::{MergeKeep, MergeOutput, MergeSurfaces, MergeSurfacesParams};
pub use trim_in_plan::{TrimInPlan, TrimInPlanParams};
pub use surface_triangulate::{TriangulateStrings, TriangulateStringsParams};
pub use from_surfaces::{Contours, ContoursParams, FromSection, FromSectionParams, Outline, OutlineParams, WhereSurfacesMeet, WhereSurfacesMeetParams};
pub use measure::{MEASURES, Measure, MeasureParams, block_length, floor_dip, measure, plan_area};
pub use reserves_run::{
    ColumnSteps, FormulaTraceView, ParamRead, StepKind, TraceLineView, column_trace, formula_trace, OUT_OF_MODEL, RunReserves, RunReservesParams, STEPS, StepView, StepWrite, UNDERBURDEN_THICKNESS, UnitView, Working, dataflow_starter, preston_sanders,
    reserves_preview, rerun_params, table_column, StepUnits, units_at_step, ChangedSince, changed_since, Unsaved,
};
pub use starter::{DISCARDED, RENAMED_FIELDS, TONNES_AD_PER_M3, renamed_from_older, starter_formulas, with_starter_formulas};
pub use zd_script::REFERENCE as FORMULA_REFERENCE;
pub use formulas::{FormulaNames, FormulaProblem, FormulaReport, FormulaTestResult, check_formula, FORMULA_STAGES, LABELS as FORMULA_LABELS, check_formula_steps, check_formula_steps_flagging, declared as formula_declared};
pub use dataflow::{DATAFLOW_LAYER, ExportParameters, PARAMETERS_LAYER, SaveDataflow, SaveParameters};
pub use attributes::{AttributeCell, CalculateInTable, CalculateParams, EnterInTable, EnterParams, Entry, RemoveColumn, RemoveColumnParams, SetAttributes, SetAttributesParams, TableCell};
pub use update_records::{RecordChange, RecordsMode, RecordsReport, RecordsTarget, UpdateRecords, UpdateRecordsParams, match_records};

use crate::{CommandError, Module, Registry};

pub fn register_all(registry: &mut Registry) {
    // Each command in the module a licence turns on to run it (decision 0098).
    // Viewer (free): open, show, measure, interrogate, compare, import and export; nothing designed or cut.
    registry.register(Module::Viewer, CreateLayer);
    registry.register(Module::Viewer, ExportParameters);
    registry.register(Module::Viewer, Measure);
    registry.register(Module::Viewer, RenameLayer);
    registry.register(Module::Viewer, RecolourLayer);
    registry.register(Module::Viewer, CompareSurfaces);
    registry.register(Module::Viewer, ImportStrings);
    registry.register(Module::Viewer, ImportSurface);
    registry.register(Module::Viewer, ImportSolid);
    registry.register(Module::Viewer, ImportSolids);
    registry.register(Module::Viewer, ImportDeswik);
    registry.register(Module::Viewer, ImportSpry);
    registry.register(Module::Viewer, Interrogate);
    registry.register(Module::Viewer, CompareSolids);
    registry.register(Module::Viewer, ImportVulcanSeam);
    registry.register(Module::Viewer, ExportDxf);
    registry.register(Module::Viewer, ExportCsv);
    registry.register(Module::Viewer, ExportSolids);
    // Design: strings, faces, ramps, surfaces, workflows and editing objects.
    registry.register(Module::Design, DeleteLayer);
    registry.register(Module::Design, DeleteObjects);
    registry.register(Module::Design, MoveObjects);
    registry.register(Module::Design, TransformObjects);
    registry.register(Module::Design, CopyObjects);
    registry.register(Module::Design, RenameObject);
    registry.register(Module::Design, SetAttributes);
    registry.register(Module::Design, DeleteObject);
    registry.register(Module::Design, CreateString);
    registry.register(Module::Design, WhereSurfacesMeet);
    registry.register(Module::Design, MergeSurfaces);
    registry.register(Module::Design, TrimInPlan);
    registry.register(Module::Design, TriangulateStrings);
    registry.register(Module::Design, Contours);
    registry.register(Module::Design, Outline);
    registry.register(Module::Design, FromSection);
    registry.register(Module::Design, InsertVertex);
    registry.register(Module::Design, MoveVertex);
    registry.register(Module::Design, DeleteVertex);
    registry.register(Module::Design, MoveVertices);
    registry.register(Module::Design, DeleteVertices);
    registry.register(Module::Design, VerticesAtCrossings);
    registry.register(Module::Design, DensifyString);
    registry.register(Module::Design, RedrawStretch);
    registry.register(Module::Design, SetGrade);
    registry.register(Module::Design, OffsetString);
    registry.register(Module::Design, NumberStrings);
    registry.register(Module::Design, PolygonsFromLines);
    registry.register(Module::Design, DrapeString);
    registry.register(Module::Design, SetElevation);
    registry.register(Module::Design, ReverseString);
    registry.register(Module::Design, SetClosed);
    registry.register(Module::Design, SplitString);
    registry.register(Module::Design, JoinStrings);
    registry.register(Module::Design, TrimString);
    registry.register(Module::Design, ExtendString);
    registry.register(Module::Design, ClipString);
    registry.register(Module::Design, PolygonBoolean);
    registry.register(Module::Design, SmoothString);
    registry.register(Module::Design, FilletString);
    registry.register(Module::Design, FilterString);
    registry.register(Module::Design, ProjectString);
    registry.register(Module::Design, SurfaceBetweenStrings);
    registry.register(Module::Design, RampFromStart);
    registry.register(Module::Design, SurfaceFromMesh);
    registry.register(Module::Design, ProjectStep);
    registry.register(Module::Design, SaveDesign);
    registry.register(Module::Design, ProjectDesign);
    registry.register(Module::Design, Step);
    registry.register(Module::Design, BuildStep);
    registry.register(Module::Design, Rebuild);
    registry.register(Module::Design, JoinFaces);
    registry.register(Module::Design, SaveWorkflow);
    // Solids and blocks: solids, cuts, blocks, benches and seam models.
    registry.register(Module::Solids, CutByPolygons);
    registry.register(Module::Solids, CutBySeams);
    registry.register(Module::Solids, CutByBenches);
    registry.register(Module::Solids, SolidFromMesh);
    registry.register(Module::Solids, RepairSolids);
    registry.register(Module::Solids, CutBySurface);
    registry.register(Module::Solids, SliceByFaces);
    registry.register(Module::Solids, SolidBetween);
    registry.register(Module::Solids, SuggestSeamDefinition);
    registry.register(Module::Solids, BuildSeamModel);
    registry.register(Module::Solids, RelinkSeamModels);
    // Reserves: parameters, dataflows and their formulas, reserves runs.
    registry.register(Module::Reserves, SaveParameters);
    registry.register(Module::Reserves, SaveDataflow);
    registry.register(Module::Reserves, RunReserves);
    registry.register(Module::Reserves, EnterInTable);
    registry.register(Module::Reserves, RemoveColumn);
    registry.register(Module::Reserves, UpdateRecords);
    registry.register(Module::Reserves, CalculateInTable);
}

fn finite(point: &[f64; 3]) -> Result<(), CommandError> {
    if point.iter().all(|c| c.is_finite()) {
        Ok(())
    } else {
        Err(CommandError::Invalid(format!("point {point:?} is not finite")))
    }
}

/// Runs `work` on every item across the machine's cores; results come back in item order.
/// [`in_parallel`] for a long run (decision 0060): says how many of the items are done as each finishes, and once the
/// run is cancelled takes no more and refuses it, so nothing is applied.
pub(crate) fn in_parallel_watched<I: Sync, T: Send>(inputs: &crate::Inputs<'_>, of: &'static str, items: &[I], work: impl Fn(&I) -> T + Sync) -> Result<Vec<T>, crate::CommandError> {
    inputs.progress(0, items.len(), of);
    let done = in_parallel(items, |item| {
        inputs.stop_if_cancelled().ok()?;
        let result = work(item);
        inputs.step();
        Some(result)
    });
    inputs.stop_if_cancelled()?;
    Ok(done.into_iter().map(|r| r.expect("not cancelled, so every item ran")).collect())
}

pub(crate) fn in_parallel<I: Sync, T: Send>(items: &[I], work: impl Fn(&I) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, items.len().max(1));
    // Each core takes the next item not yet taken, so a run of heavy items isn't left to one core while the others
    // stand idle; the results go back in the items' order.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut done: Vec<(usize, T)> = std::thread::scope(|scope| {
        let (work, next) = (&work, &next);
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(move || {
                    let mut mine = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(k) else { break };
                        mine.push((k, work(item)));
                    }
                    mine
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("worker thread")).collect()
    });
    done.sort_unstable_by_key(|(k, _)| *k);
    done.into_iter().map(|(_, result)| result).collect()
}
