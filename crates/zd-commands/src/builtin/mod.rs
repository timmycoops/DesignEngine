//! The engine's own commands. Strings (workstream 6): create, vertex edits, grade, offset, drape,
//! elevation, reverse, close, split, join, smooth and filter. Exchange (workstream 4): strings in
//! from DXF and CSV, objects out to DXF and CSV.

mod exchange;
mod face;
mod grid;
mod layer;
mod object;
mod seam;
mod solid;
mod string;
mod surface;

pub use exchange::{ExportCsv, ExportDxf, ExportParams, ImportStrings, ImportStringsParams};
pub use face::{ProjectDesign, ProjectDesignParams, ProjectStep, ProjectStepParams, SaveDesign, SaveDesignParams};
pub use grid::{ImportVulcanSeam, ImportVulcanSeamParams};
pub use layer::{CreateLayer, CreateLayerParams};
pub use object::{DeleteObject, ObjectParams, RenameObject, RenameParams};
pub use seam::{BuildSeamModel, BuildSeamModelParams, SuggestSeamDefinition, SuggestSeamDefinitionParams};
pub(crate) use solid::check_solids_made;
pub use solid::{CutBySurface, CutBySurfaceParams, ImportSolid, ImportSolidParams, ImportSolids, ImportSolidsParams, SolidFromMesh};
pub use string::{
    CornerParam, CreateString, CreateStringParams, DeleteVertex, DeleteVertexParams, DrapeString, DrapeStringParams, FilterString,
    FilterStringParams, InsertVertex, InsertVertexParams, JoinStrings, JoinStringsParams, MoveVertex, MoveVertexParams, OffsetString,
    OffsetStringParams, ReverseString, SetClosed, SetClosedParams, SetElevation, SetElevationParams, SetGrade, SetGradeParams, SideParam,
    SmoothString, SmoothStringParams, SplitString, SplitStringParams, StringParams,
};
pub use surface::{ImportSurface, ImportSurfaceParams, MeshParams, SurfaceFromMesh};

use crate::{CommandError, Registry};

pub fn register_all(registry: &mut Registry) {
    registry.register(CreateLayer);
    registry.register(RenameObject);
    registry.register(DeleteObject);
    registry.register(CreateString);
    registry.register(InsertVertex);
    registry.register(MoveVertex);
    registry.register(DeleteVertex);
    registry.register(SetGrade);
    registry.register(OffsetString);
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
    registry.register(CutBySurface);
    registry.register(ProjectStep);
    registry.register(SaveDesign);
    registry.register(ProjectDesign);
    registry.register(ImportVulcanSeam);
    registry.register(SuggestSeamDefinition);
    registry.register(BuildSeamModel);
    registry.register(ExportDxf);
    registry.register(ExportCsv);
}

fn finite(point: &[f64; 3]) -> Result<(), CommandError> {
    if point.iter().all(|c| c.is_finite()) {
        Ok(())
    } else {
        Err(CommandError::Invalid(format!("point {point:?} is not finite")))
    }
}

/// Runs `work` on every item across the machine's cores; results come back in item order.
pub(crate) fn in_parallel<I: Sync, T: Send>(items: &[I], work: impl Fn(&I) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, items.len().max(1));
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let work = &work;
        let handles: Vec<_> = items.chunks(chunk).map(|part| scope.spawn(move || part.iter().map(work).collect::<Vec<T>>())).collect();
        handles.into_iter().flat_map(|h| h.join().expect("worker thread")).collect()
    })
}
