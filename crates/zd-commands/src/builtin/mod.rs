//! The engine's own commands. Strings (workstream 6): create, vertex edits, grade, offset, drape,
//! elevation, reverse, close, split, join, smooth and filter. Exchange (workstream 4): strings in
//! from DXF and CSV, objects out to DXF and CSV. Deswik drawings in (decision 0036), Spry projects in (decision 0039),
//! and closed surfaces an import couldn't make solids fixed by repair (decision 0040).

mod build;
mod compare;
mod deswik;
mod exchange;
mod face;
mod grid;
mod interrogate;
mod layer;
mod object;
mod seam;
mod solid;
mod spry;
mod string;
mod strips;
mod surface;

pub use compare::{CompareSolids, CompareSolidsParams};
pub use build::{BuildOp, BuildStep, BuildStepParams, Rebuild, RebuildParams};
pub use deswik::{ImportDeswik, ImportDeswikParams, MeshesAs};
pub use exchange::{ExportCsv, ExportDxf, ExportParams, ExportSolids, ExportSolidsParams, ImportStrings, ImportStringsParams, LayerInFile};
pub use face::{ProjectDesign, ProjectDesignParams, ProjectStep, ProjectStepParams, SaveDesign, SaveDesignParams, Step, StepParams};
pub use grid::{ImportVulcanSeam, ImportVulcanSeamParams};
pub use interrogate::{COLUMN_VOLUME, INTERROGATED, Interrogate, InterrogateParams, names_seam};
pub use layer::{CreateLayer, CreateLayerParams, DeleteLayer, DeleteLayerParams, RecolourLayer, RecolourLayerParams, RenameLayer, RenameLayerParams};
pub use object::{DeleteObject, DeleteObjects, MoveObjects, MoveObjectsParams, ObjectParams, ObjectsParams, RenameObject, RenameParams};
pub use seam::{BuildSeamModel, BuildSeamModelParams, ModelLinks, RelinkParams, RelinkSeamModels, SuggestSeamDefinition, SuggestSeamDefinitionParams, seam_links};
pub(crate) use solid::check_solids_made;
pub use spry::{ImportSpry, ImportSpryParams};
pub use solid::{CutBySurface, CutBySurfaceParams, ImportSolid, ImportSolidParams, ImportSolids, ImportSolidsParams, RepairSolids, RepairSolidsParams, SolidFromMesh};
pub use string::{
    CornerParam, CreateString, CreateStringParams, DeleteVertex, DeleteVertexParams, DrapeString, DrapeStringParams, FilterString,
    FilterStringParams, InsertVertex, InsertVertexParams, JoinStrings, JoinStringsParams, MoveVertex, MoveVertexParams, NumberStrings,
    NumberStringsParams, OffsetString, OffsetStringParams, RedrawStretch, RedrawStretchParams, ReverseString, SetClosed, SetClosedParams, SetElevation, SetElevationParams, SetGrade, SetGradeParams, SideParam,
    SmoothString, SmoothStringParams, SplitString, SplitStringParams, StringParams,
};
pub use strips::{JoinFaces, JoinFacesParams, SliceByFaces, SliceByFacesParams, SolidBetween, SolidBetweenParams};
pub use surface::{ImportSurface, ImportSurfaceParams, MeshParams, SurfaceFromMesh};

use crate::{CommandError, Registry};

pub fn register_all(registry: &mut Registry) {
    registry.register(CreateLayer);
    registry.register(RenameLayer);
    registry.register(RecolourLayer);
    registry.register(DeleteLayer);
    registry.register(DeleteObjects);
    registry.register(MoveObjects);
    registry.register(RenameObject);
    registry.register(DeleteObject);
    registry.register(CreateString);
    registry.register(InsertVertex);
    registry.register(MoveVertex);
    registry.register(DeleteVertex);
    registry.register(RedrawStretch);
    registry.register(SetGrade);
    registry.register(OffsetString);
    registry.register(NumberStrings);
    registry.register(DrapeString);
    registry.register(SetElevation);
    registry.register(ReverseString);
    registry.register(SetClosed);
    registry.register(SplitString);
    registry.register(JoinStrings);
    registry.register(SmoothString);
    registry.register(FilterString);
    registry.register(ImportStrings);
    registry.register(ImportSurface);
    registry.register(SurfaceFromMesh);
    registry.register(SolidFromMesh);
    registry.register(ImportSolid);
    registry.register(ImportSolids);
    registry.register(ImportDeswik);
    registry.register(ImportSpry);
    registry.register(RepairSolids);
    registry.register(CutBySurface);
    registry.register(SliceByFaces);
    registry.register(SolidBetween);
    registry.register(Interrogate);
    registry.register(CompareSolids);
    registry.register(ProjectStep);
    registry.register(SaveDesign);
    registry.register(ProjectDesign);
    registry.register(Step);
    registry.register(BuildStep);
    registry.register(Rebuild);
    registry.register(JoinFaces);
    registry.register(ImportVulcanSeam);
    registry.register(SuggestSeamDefinition);
    registry.register(BuildSeamModel);
    registry.register(RelinkSeamModels);
    registry.register(ExportDxf);
    registry.register(ExportCsv);
    registry.register(ExportSolids);
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
