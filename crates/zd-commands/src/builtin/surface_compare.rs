//! `surface.compare` (Surface tools plan, workstream 1; decision 0093): the cut and fill between two surfaces or grids
//! inside a closed string, or wherever both are, exact as the solids between them each way, with a 2.5D column check
//! alongside and, asked for, a thickness grid. The surfaces read are left as they were.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ObjectId, ObjectKind};
use zd_geom::{COLUMN_CHECK, compare_surfaces};

use super::from_surfaces::mesh_of;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

pub struct CompareSurfaces;

fn default_cell() -> f64 {
    2.0
}

fn default_thickness_layer() -> String {
    "thickness".into()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompareSurfacesParams {
    /// The surface or grid compared against: the topography, say.
    pub surface: ObjectId,
    /// The one compared: the design. Cut is where it's below `surface`, fill where it's above.
    pub with: ObjectId,
    /// A closed string to compare inside; wherever both are if left out.
    #[serde(default)]
    pub string: Option<ObjectId>,
    /// The column check's cell (m).
    #[serde(default = "default_cell")]
    pub cell: f64,
    /// Make the thickness grid, `with` less `surface` at each column: off unless asked for.
    #[serde(default)]
    pub thickness: bool,
    #[serde(default = "default_thickness_layer")]
    pub layer: String,
    /// The thickness grid's name: "<with> less <surface>" by default.
    #[serde(default)]
    pub name: Option<String>,
}

/// A volume for a sentence, to a tenth of a cubic metre with thousands marked.
fn bcm(v: f64) -> String {
    let whole = format!("{:.1}", v.abs());
    let (int, frac) = whole.split_once('.').unwrap_or((&whole, "0"));
    let mut marked = String::new();
    for (k, c) in int.chars().enumerate() {
        if k > 0 && (int.len() - k) % 3 == 0 {
            marked.push(',');
        }
        marked.push(c);
    }
    format!("{}{marked}.{frac}", if v < 0.0 { "-" } else { "" })
}

impl Command for CompareSurfaces {
    type Params = CompareSurfacesParams;
    const NAME: &'static str = "surface.compare";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Compare two surfaces or grids: the cut and fill between them, exact, with a column check, inside a closed string or wherever both are",
            inputs: vec![
                Field::object("surface", ObjectKind::Surface, "The surface or grid compared against: the topography, say"),
                Field::object("with", ObjectKind::Surface, "The one compared: cut where it's below, fill where it's above"),
                Field::object("string", ObjectKind::String, "A closed string to compare inside; wherever both are if left out"),
            ],
            params: vec![
                Field::new("cell", FieldType::Number { unit: "m" }, "The column check's cell (m)"),
                Field::new("thickness", FieldType::Bool, "Make the thickness grid, the second less the first"),
                Field::new("layer", FieldType::Layer, "Layer for the thickness grid"),
                Field::new("name", FieldType::Text, "The thickness grid's name; '<with> less <surface>' by default"),
            ],
            outputs: vec![Field::object("thickness", ObjectKind::Grid, "The thickness grid, when asked for")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.surface == p.with {
            return Err(CommandError::Invalid("a surface compared with itself has nothing between: pick two different surfaces".into()));
        }
        let (a, b) = (inputs.object(p.surface)?, inputs.object(p.with)?);
        let (first, second) = (mesh_of(a, false)?, mesh_of(b, false)?);
        let ring = match p.string {
            Some(id) => {
                let (object, line) = inputs.string(id)?;
                if line.vertices.len() < 3 {
                    return Err(CommandError::Invalid(format!("'{}' has too few vertices to close round an area", object.name)));
                }
                Some((object.name.clone(), line.vertices.iter().map(|v| [v[0], v[1]]).collect::<Vec<_>>()))
            }
            None => None,
        };
        let compared = compare_surfaces(&first, &second, ring.as_ref().map(|(_, r)| r.as_slice()), p.cell, p.thickness).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let where_ = ring.as_ref().map_or("wherever both are".to_string(), |(name, _)| format!("inside '{name}'"));
        let net = compared.cut - compared.fill;
        let mut said = vec![format!(
            "'{}' against '{}' {where_}: cut {} m³, fill {} m³, net {} {} m³, over {} m² both cover; exact, in {:.0} ms",
            b.name,
            a.name,
            bcm(compared.cut),
            bcm(compared.fill),
            if net >= 0.0 { "cut" } else { "fill" },
            bcm(net.abs()),
            bcm(compared.area),
            compared.exact_ms
        )];
        let off = compared.column_difference();
        said.push(format!(
            "by columns at {} m ({} of them): cut {} m³, fill {} m³, {:.3} % from the exact, in {:.0} ms",
            compared.cell,
            compared.columns,
            bcm(compared.column_cut),
            bcm(compared.column_fill),
            100.0 * off,
            compared.column_ms
        ));
        let mut changes = Vec::new();
        if let Some(grid) = compared.thickness {
            let name = p.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).map_or_else(|| format!("{} less {}", b.name, a.name), str::to_string);
            said.push(format!("thickness grid '{name}' on '{}': {} m cells, {} by {}", p.layer, compared.cell, grid.cols, grid.rows));
            changes.push(Change::Create { name, layer: p.layer.clone(), attributes: Attributes::new(), body: Body::Grid(Arc::new(grid)) });
        }
        let mut outcome = Outcome::new(changes);
        outcome.said(said);
        if off > COLUMN_CHECK {
            outcome.notes.push(
                Note::warning(Code::SurfaceColumnsDiffer, format!("the column check at {} m is {:.3} % from the exact volumes, more than {} %: a finer cell may close it, or the surfaces fold where columns can't follow", compared.cell, 100.0 * off, 100.0 * COLUMN_CHECK)).measured(100.0 * off, "%"),
            );
        }
        Ok(outcome)
    }
}
