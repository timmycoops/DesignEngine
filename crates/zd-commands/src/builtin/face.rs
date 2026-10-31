use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ObjectId, ObjectKind, Surface};
use zd_geom::{COINCIDENT, OffsetSide, find_crossings, next_toe, project_wall, triangulate_grid};

use super::SideParam;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// `face.project_step`: one wall from a string up to a target surface or grid, at a fixed batter. It
/// makes the crest, the wall as a surface, and the string the next step starts from: the crest
/// conditioned in plan, moved out a berm and registered back on the target. The string stays as it is.
pub struct ProjectStep;

fn default_tolerance() -> f64 {
    0.1
}

/// Past this a conditioning tolerance stops tidying the crest and starts cutting its corners off.
const MOST_TOLERANCE: f64 = 2.0;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStepParams {
    pub string: ObjectId,
    pub target: ObjectId,
    /// Degrees from horizontal, square to the string in plan.
    pub batter: f64,
    pub side: SideParam,
    /// Width of the berm at the crest, for the next toe; none by default.
    #[serde(default)]
    pub berm: f64,
    /// How far the next toe's plan shape may leave the crest.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// Defaults to the string's layer.
    #[serde(default)]
    pub layer: Option<String>,
    /// Defaults to the string's name.
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for ProjectStep {
    type Params = ProjectStepParams;
    const NAME: &'static str = "face.project_step";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Project a wall from a string up to a surface or grid",
            inputs: vec![
                Field::object("string", ObjectKind::String, "The toe, lying on the grid the wall starts from"),
                Field::object("target", ObjectKind::Surface, "The surface or grid the wall rises to"),
            ],
            params: vec![
                Field::new("batter", FieldType::Number { unit: "deg" }, "From horizontal, square to the string in plan"),
                Field::new("side", FieldType::Text, "left or right, looking along the string"),
                Field::new("berm", FieldType::Number { unit: "m" }, "Width of the berm at the crest; none by default"),
                Field::new("tolerance", FieldType::Number { unit: "m" }, "How far the next toe's plan shape may leave the crest; 0.1 by default, 2 at most"),
                Field::new("layer", FieldType::Layer, "Layer for what's made; the string's by default"),
                Field::new("name", FieldType::Text, "Start of the new names; the string's by default"),
            ],
            outputs: vec![
                Field::object("crest", ObjectKind::String, "Where the wall meets the target"),
                Field::object("wall", ObjectKind::Surface, "The wall from the string to the crest"),
                Field::object("next_toe", ObjectKind::String, "The string the next step starts from, one per piece"),
            ],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        let target = inputs.object(p.target)?;
        if p.tolerance > MOST_TOLERANCE {
            return Err(CommandError::Invalid(format!(
                "a tolerance of {} m would cut corners off the crest: it's how far the next toe may leave the crest, at most {MOST_TOLERANCE} m. The berm width has its own box",
                p.tolerance
            )));
        }
        let triangulated;
        let mesh = match &target.body {
            Body::Surface(s) => s.mesh(),
            Body::Grid(g) => {
                triangulated = triangulate_grid(g);
                &triangulated
            }
            other => return Err(CommandError::WrongKind { id: p.target, expected: ObjectKind::Surface, found: other.kind() }),
        };
        let side = match p.side {
            SideParam::Left => OffsetSide::Left,
            SideParam::Right => OffsetSide::Right,
        };
        let step = project_wall(line, side, p.batter, mesh).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let (toes, conditioned) = next_toe(&step.crest, p.berm, side, p.tolerance, mesh).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let crossings = find_crossings(&step.wall, COINCIDENT);
        let mut message = format!(
            "wall at {}° to {}: {} walls, {} squeezed out, {} crest vertices, {} triangles; next toe keeps {} of {} plan vertices",
            p.batter,
            target.name,
            step.log.walls,
            step.log.squeezed_out,
            step.crest.vertices.len(),
            step.wall.triangles.len(),
            conditioned.plan_vertices,
            conditioned.vertices_in,
        );
        if step.log.vertices_dropped > 0 {
            message += &format!("; {} string vertices on the one before in plan were dropped", step.log.vertices_dropped);
        }
        if let Some(first) = step.log.no_height.first() {
            message += &format!("; no height along {} stretch(es), the first from {:.1}, {:.1}", step.log.no_height.len(), first.from[0], first.from[1]);
        }
        if conditioned.off_surface > 0 {
            message += &format!("; {} next toe vertices have no target under them and keep their elevations", conditioned.off_surface);
        }
        if crossings.pairs > 0 {
            message += &format!("; {} pairs of wall triangles cross", crossings.pairs);
        }
        let base = p.name.clone().unwrap_or_else(|| object.name.clone());
        let layer = p.layer.clone().unwrap_or_else(|| inputs.layer_name(object));
        let create = |name: String, body: Body| Change::Create { name, layer: layer.clone(), attributes: Attributes::new(), body };
        let mut changes = vec![
            create(format!("{base} crest"), Body::String(step.crest)),
            create(format!("{base} wall"), Body::Surface(Surface::new(step.wall).with_crossings(crossings))),
        ];
        let count = toes.len();
        changes.extend(toes.into_iter().enumerate().map(|(i, toe)| {
            let name = if count == 1 { format!("{base} next toe") } else { format!("{base} next toe ({})", i + 1) };
            create(name, Body::String(toe))
        }));
        Ok(Outcome::new(changes).message(message))
    }
}
