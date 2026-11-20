//! `surface.merge` (Surface tools plan, workstream 2; decision 0094): the bottom or the top of the surfaces, grids and
//! solids picked, as a grid at a cell size or as an exact surface, new on a layer. What's read is left as it was.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ObjectId, ObjectKind, Surface};
use zd_geom::{Envelope, MergeInput, merge_to_grid, merge_to_surface};

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

pub struct MergeSurfaces;

/// Which of them is kept at each place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeKeep {
    /// The lowest: a pit shell or strip solids dug out of the topography.
    #[default]
    Bottom,
    /// The highest: a dump laid on it.
    Top,
}

/// What it comes out as.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeOutput {
    /// A grid at `cell`: fast at any size.
    #[default]
    Grid,
    /// An exact surface: every crest, toe and berm where they are, joined exactly where they meet.
    Surface,
}

fn default_merge_layer() -> String {
    "merged".into()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MergeSurfacesParams {
    /// The surfaces, grids and solids to merge: two or more.
    pub objects: Vec<ObjectId>,
    #[serde(default)]
    pub keep: MergeKeep,
    #[serde(default)]
    pub output: MergeOutput,
    /// The grid's cell (m): 5 unless set, or, every input a grid on one lattice, that lattice node by node.
    #[serde(default)]
    pub cell: Option<f64>,
    #[serde(default = "default_merge_layer")]
    pub layer: String,
    /// What it's called: "Merged bottom" or "Merged top" by default.
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for MergeSurfaces {
    type Params = MergeSurfacesParams;
    const NAME: &'static str = "surface.merge";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Merge surfaces, grids and solids: their bottom (dug out) or top (laid on), as a grid at a cell or an exact surface",
            inputs: vec![Field::new("objects", FieldType::Objects, "The surfaces, grids and solids to merge; a solid counts by its floor for the bottom, its top for the top")],
            params: vec![
                Field::new("keep", FieldType::Text, "`bottom` (the lowest, the default) or `top` (the highest)"),
                Field::new("output", FieldType::Text, "`grid` (the default) or `surface` (exact)"),
                Field::new("cell", FieldType::Number { unit: "m" }, "The grid's cell; 5 m unless set, or grids alike node by node"),
                Field::new("layer", FieldType::Layer, "Layer for what's made"),
                Field::new("name", FieldType::Text, "What it's called"),
            ],
            outputs: vec![Field::object("merged", ObjectKind::Surface, "The merged grid or surface")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.objects.len() < 2 {
            return Err(CommandError::Invalid(format!("a merge takes two things or more; {} was given", p.objects.len())));
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(twice) = p.objects.iter().find(|id| !seen.insert(**id)) {
            return Err(CommandError::Invalid(format!("object {twice} is given twice")));
        }
        let mut objects = Vec::new();
        for &id in &p.objects {
            objects.push(inputs.object(id)?);
        }
        let mut merging = Vec::with_capacity(objects.len());
        for o in &objects {
            merging.push(match &o.body {
                Body::Surface(s) => MergeInput::Surface(s.mesh()),
                Body::Grid(g) => MergeInput::Grid(g),
                Body::Solid(s) => MergeInput::Solid(s.mesh()),
                other => return Err(CommandError::WrongKind { id: o.id, expected: ObjectKind::Surface, found: other.kind() }),
            });
        }
        let keep = match p.keep {
            MergeKeep::Bottom => Envelope::Bottom,
            MergeKeep::Top => Envelope::Top,
        };
        let which = if keep == Envelope::Bottom { "bottom" } else { "top" };
        let name = p.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).map_or_else(|| format!("Merged {which}"), str::to_string);
        let layer = p.layer.trim();
        if layer.is_empty() {
            return Err(CommandError::Invalid("no layer for what's made".into()));
        }
        let names: Vec<&str> = objects.iter().map(|o| o.name.as_str()).collect();
        let of = if names.len() > 3 { format!("{} and {} more", names[..3].join(", "), names.len() - 3) } else { names.join(", ") };
        let (body, said) = match p.output {
            MergeOutput::Grid => {
                let merged = merge_to_grid(&merging, keep, p.cell).map_err(|e| CommandError::Invalid(e.to_string()))?;
                let g = &merged.grid;
                let how = if merged.same_lattice { "node by node on their own lattice".to_string() } else { format!("sampled at {} m", g.cell_size[0]) };
                let said = format!("the {which} of {of}, as a grid {how}: {} by {} nodes, {} with a height, in {:.0} ms", g.cols, g.rows, merged.nodes_with_height, merged.ms);
                (Body::Grid(Arc::new(merged.grid)), said)
            }
            MergeOutput::Surface => {
                let merged = merge_to_surface(&merging, keep).map_err(|e| CommandError::Invalid(e.to_string()))?;
                let said = format!("the {which} of {of}, as an exact surface: {} triangles in, {} out, in {:.0} ms", merged.triangles_in, merged.mesh.triangles.len(), merged.ms);
                (Body::Surface(Surface::new(merged.mesh)), said)
            }
        };
        Ok(Outcome::new(vec![Change::Create { name, layer: layer.to_string(), attributes: Attributes::new(), body }]).message(said))
    }
}
