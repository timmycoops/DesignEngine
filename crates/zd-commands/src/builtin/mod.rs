//! The engine's own commands. Strings: create, vertex edits, grade, offset, drape, elevation,
//! reverse and close (workstream 6, slice 1); split, join, smooth and filter come next.

mod grid;
mod layer;
mod object;
mod seam;
mod solid;
mod string;
mod surface;

pub use grid::{ImportVulcanSeam, ImportVulcanSeamParams};
pub use layer::{CreateLayer, CreateLayerParams};
pub use object::{DeleteObject, ObjectParams, RenameObject, RenameParams};
pub use seam::{BuildSeamModel, BuildSeamModelParams, SuggestSeamDefinition, SuggestSeamDefinitionParams};
pub use solid::{CutBySurface, CutBySurfaceParams, SolidFromMesh};
pub use string::{
    CornerParam, CreateString, CreateStringParams, DeleteVertex, DeleteVertexParams, DrapeString, DrapeStringParams, InsertVertex,
    InsertVertexParams, MoveVertex, MoveVertexParams, OffsetString, OffsetStringParams, ReverseString, SetClosed, SetClosedParams,
    SetElevation, SetElevationParams, SetGrade, SetGradeParams, SideParam, StringParams,
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
    registry.register(ImportSurface);
    registry.register(SurfaceFromMesh);
    registry.register(SolidFromMesh);
    registry.register(CutBySurface);
    registry.register(ImportVulcanSeam);
    registry.register(SuggestSeamDefinition);
    registry.register(BuildSeamModel);
}

fn finite(point: &[f64; 3]) -> Result<(), CommandError> {
    if point.iter().all(|c| c.is_finite()) {
        Ok(())
    } else {
        Err(CommandError::Invalid(format!("point {point:?} is not finite")))
    }
}
