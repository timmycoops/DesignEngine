//! The engine's own commands. Phase 1 starts with enough to prove the command layer end to end;
//! the full string set lands with workstream 6.

mod layer;
mod object;
mod solid;
mod string;
mod surface;

pub use layer::{CreateLayer, CreateLayerParams};
pub use object::{DeleteObject, ObjectParams, RenameObject, RenameParams};
pub use solid::{CutBySurface, CutBySurfaceParams, SolidFromMesh};
pub use string::{
    CreateString, CreateStringParams, DeleteVertex, DeleteVertexParams, InsertVertex, InsertVertexParams, MoveVertex,
    MoveVertexParams,
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
    registry.register(ImportSurface);
    registry.register(SurfaceFromMesh);
    registry.register(SolidFromMesh);
    registry.register(CutBySurface);
}

fn finite(point: &[f64; 3]) -> Result<(), CommandError> {
    if point.iter().all(|c| c.is_finite()) {
        Ok(())
    } else {
        Err(CommandError::Invalid(format!("point {point:?} is not finite")))
    }
}
