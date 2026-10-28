use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ObjectId, ObjectKind, Solid};
use zd_geom::cut_solid_by_surface;

use super::surface::{MeshParams, describe, mesh_params};
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// `solid.from_mesh`: a solid from vertices and triangles given inline; refused unless watertight.
pub struct SolidFromMesh;

impl Command for SolidFromMesh {
    type Params = MeshParams;
    const NAME: &'static str = "solid.from_mesh";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "New solid from inline vertices and triangles",
            inputs: vec![],
            params: mesh_params("Name of the new solid"),
            outputs: vec![Field::object("solid", ObjectKind::Solid, "The new solid")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let solid = Solid::new(p.mesh()?).map_err(CommandError::NotWatertight)?;
        let flags = describe(&solid.validity(), solid.mesh());
        let volume = format!("volume {:.3} m3", solid.volume());
        Ok(Outcome::new(vec![Change::Create {
            name: p.name.clone(),
            layer: p.layer.clone(),
            attributes: Attributes::new(),
            body: Body::Solid(solid),
        }])
        .message(flags)
        .message(volume))
    }
}

/// `solid.cut_by_surface`: the parts of a solid below and above a design surface, as new solids
/// (engine rule 5). The input solid is left as it is.
pub struct CutBySurface;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutBySurfaceParams {
    pub solid: ObjectId,
    pub surface: ObjectId,
    /// Layer for both new solids.
    pub layer: String,
    /// Default: "<solid> below <surface>".
    #[serde(default)]
    pub below_name: Option<String>,
    /// Default: "<solid> above <surface>".
    #[serde(default)]
    pub above_name: Option<String>,
}

impl Command for CutBySurface {
    type Params = CutBySurfaceParams;
    const NAME: &'static str = "solid.cut_by_surface";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Cut a solid by a surface into the parts below and above it",
            inputs: vec![
                Field::object("solid", ObjectKind::Solid, "The solid to cut"),
                Field::object("surface", ObjectKind::Surface, "The cutting surface"),
            ],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the new solids"),
                Field::new("below_name", FieldType::Text, "Optional name for the part below"),
                Field::new("above_name", FieldType::Text, "Optional name for the part above"),
            ],
            outputs: vec![
                Field::object("below", ObjectKind::Solid, "The part below the surface (left out if empty)"),
                Field::object("above", ObjectKind::Solid, "The part above the surface (left out if empty)"),
            ],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (solid_object, solid) = inputs.solid(p.solid)?;
        let (surface_object, surface) = inputs.surface(p.surface)?;
        let cut = cut_solid_by_surface(solid.mesh(), surface.mesh()).map_err(|e| CommandError::Geometry(e.to_string()))?;

        let t = cut.timings;
        let mut outcome = Outcome::default()
            .message(format!(
                "{} + {} triangles in, {} below + {} above out, {:.0} ms (close {:.0}, to Manifold {:.0}, boolean {:.0}, from Manifold {:.0}); cap {:?}",
                solid.mesh().triangle_count(),
                surface.mesh().triangle_count(),
                cut.below.triangle_count(),
                cut.above.triangle_count(),
                t.total_ms(),
                t.close_ms,
                t.to_manifold_ms,
                t.boolean_ms,
                t.from_manifold_ms,
                cut.cap_method
            ))
            .message(format!(
                "volumes: input {:.3} m3, below {:.3} m3, above {:.3} m3",
                cut.input_volume, cut.below_volume, cut.above_volume
            ));
        if !cut.design_covers_solid {
            outcome = outcome.message("warning: the surface does not cover the solid in plan; the uncovered part is counted as above");
        }

        let parts = [
            (cut.below, p.below_name.clone(), "below"),
            (cut.above, p.above_name.clone(), "above"),
        ];
        for (mesh, name, side) in parts {
            if mesh.triangles.is_empty() {
                outcome = outcome.message(format!("nothing of the solid lies {side} the surface"));
                continue;
            }
            let part = Solid::new(mesh).map_err(CommandError::NotWatertight)?;
            let name = name.unwrap_or_else(|| format!("{} {side} {}", solid_object.name, surface_object.name));
            outcome.changes.push(Change::Create { name, layer: p.layer.clone(), attributes: Attributes::new(), body: Body::Solid(part) });
        }
        Ok(outcome)
    }
}
