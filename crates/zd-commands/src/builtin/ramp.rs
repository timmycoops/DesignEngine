//! `string.ramp_polygon` (decision 0079): a ramp placed from a start, as its polygon. Tim, 16 November 2026: "you place
//! a start, set the width and grade and draws the ramp polygon essentially", along a line picked or a direction, to an
//! RL or a surface, the polygon and nothing else. Placed again with `replaces`, the same polygon takes its next
//! revision somewhere else: that's how a ramp is moved.

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Body, Change, ObjectId, ObjectKind, Polyline, StepTarget, TriMesh};
use zd_geom::{OffsetSide, RampShort, RampTo, ramp_polygon, triangulate_grid};

use super::SideParam;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Place};

pub struct RampFromStart;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RampFromStartParams {
    /// Where the ramp starts, and the RL it starts at.
    pub start: [f64; 3],
    /// A point the way it runs: along the line, the side of the start it goes; with no line, its bearing. Plan only.
    pub towards: [f64; 3],
    /// The string its inside edge runs along, in plan. Without one it runs straight.
    #[serde(default)]
    pub line: Option<ObjectId>,
    /// The road's width (m).
    pub width: f64,
    /// The grade along the inside edge (%), its size: what it runs to says whether it climbs or falls.
    pub grade: f64,
    /// The side of the inside edge the road lies on, looking the way it runs.
    pub side: SideParam,
    /// What it runs to: an RL, or a surface or grid.
    pub to: StepTarget,
    /// A ramp polygon this one takes the place of, as its next revision: the ramp moved.
    #[serde(default)]
    pub replaces: Option<ObjectId>,
    /// Defaults to the line's layer, or `ramps` with no line.
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Where a ramp with no line to follow goes, unless it says.
const RAMP_LAYER: &str = "ramps";

/// What a ramp's polygon carries: how it was set and what it came to.
const WIDTH: &str = "ramp_width";
const GRADE: &str = "ramp_grade";
const LENGTH: &str = "ramp_length";
const FROM_RL: &str = "ramp_from_rl";
const TO_RL: &str = "ramp_to_rl";

/// A number as it's said in a name or a line: to the centimetre, with no trailing zeros.
fn said(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

impl Command for RampFromStart {
    type Params = RampFromStartParams;
    const NAME: &'static str = "string.ramp_polygon";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Place a ramp from a start: its polygon, along a line or a direction, at a width and grade, to an RL or a surface",
            inputs: vec![Field::object("line", ObjectKind::String, "The string its inside edge runs along, in plan; without one it runs straight")],
            params: vec![
                Field::new("start", FieldType::Point, "Where the ramp starts, and the RL it starts at"),
                Field::new("towards", FieldType::Point, "A point the way it runs; plan only"),
                Field::new("width", FieldType::Number { unit: "m" }, "The road's width"),
                Field::new("grade", FieldType::Number { unit: "%" }, "Along the inside edge; what it runs to says whether it climbs or falls"),
                Field::new("side", FieldType::Text, "left or right of the inside edge, looking the way it runs: where the road lies"),
                Field::new("to", FieldType::Text, "What it runs to: an RL, or a surface or grid"),
                Field::new("replaces", FieldType::Text, "A ramp polygon this one takes the place of, as its next revision"),
                Field::new("layer", FieldType::Layer, "Layer for the polygon; the line's by default, or ramps"),
                Field::new("name", FieldType::Text, "The polygon's name"),
            ],
            outputs: vec![Field::object("polygon", ObjectKind::String, "The ramp's outline, closed: its inside edge at the grade, and its outside edge the width out")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let line = p.line.map(|id| inputs.string(id)).transpose()?;
        let side = match p.side {
            SideParam::Left => OffsetSide::Left,
            SideParam::Right => OffsetSide::Right,
        };
        // A grid is its triangulation: projection targets are triangulations.
        let triangulated: TriMesh;
        let (to, to_words) = match p.to {
            StepTarget::Rl { rl } => (RampTo::Rl(rl), format!("RL {}", said(rl))),
            StepTarget::Surface { id } => {
                let target = inputs.object(id)?;
                let mesh = match &target.body {
                    Body::Surface(surface) => surface.mesh(),
                    Body::Grid(grid) => {
                        triangulated = triangulate_grid(grid);
                        &triangulated
                    }
                    other => return Err(CommandError::WrongKind { id, expected: ObjectKind::Surface, found: other.kind() }),
                };
                (RampTo::Surface(mesh), target.name.clone())
            }
        };
        let old = match p.replaces {
            Some(id) => {
                let (object, was): (_, &Polyline) = inputs.string(id)?;
                if !was.closed || !object.attributes.contains_key(WIDTH) {
                    return Err(CommandError::Invalid(format!("'{}' isn't a ramp's polygon, so a ramp can't take its place", object.name)));
                }
                Some(object)
            }
            None => None,
        };
        let ramp = ramp_polygon(p.start, p.towards, line.map(|(_, line)| line), p.width, p.grade, side, to).map_err(|e| CommandError::Invalid(e.to_string()))?;

        let mut attributes = old.map(|object| object.attributes.clone()).unwrap_or_default();
        if let (None, Some((object, _))) = (old, line) {
            for key in ["strip", "pit"] {
                if let Some(value) = object.attributes.get(key) {
                    attributes.insert(key.to_string(), value.clone());
                }
            }
        }
        for (key, value) in [(WIDTH, p.width), (GRADE, ramp.grade), (LENGTH, ramp.length), (FROM_RL, ramp.from_rl), (TO_RL, ramp.to_rl)] {
            attributes.insert(key.to_string(), AttrValue::Number(value));
        }
        let way = if ramp.grade > 0.0 { "up" } else { "down" };
        let end = ramp.inside.vertices[ramp.inside.vertices.len() - 1];
        let mut message = format!(
            "a ramp {} m wide at {} % {way} {}, from RL {} to RL {}: {} m long, {} vertices",
            said(p.width),
            said(ramp.grade.abs()),
            match line {
                Some((object, _)) => format!("along '{}'", object.name),
                None => "straight from its start".to_string(),
            },
            said(ramp.from_rl),
            said(ramp.to_rl),
            said(ramp.length),
            ramp.polygon.vertices.len(),
        );
        if old.is_some() {
            message += "; moved from where it was";
        }
        let change = match old {
            Some(object) => Change::Revise { id: object.id, base: object.revision, name: p.name.clone(), layer: p.layer.clone(), attributes: Some(attributes), body: Some(Body::String(ramp.polygon)) },
            None => Change::Create {
                name: p.name.clone().unwrap_or_else(|| format!("Ramp RL {} to {}", said(ramp.from_rl), said(ramp.to_rl))),
                layer: p.layer.clone().unwrap_or_else(|| line.map_or_else(|| RAMP_LAYER.to_string(), |(object, _)| inputs.layer_name(object))),
                attributes,
                body: Body::String(ramp.polygon),
            },
        };
        let mut outcome = Outcome::new(vec![change]).message(message);
        if let (Some((object, _)), true) = (line, ramp.start_moved > 1e-3) {
            outcome.notes.push(Note::info(format!("the start was {} m off '{}': the ramp starts at the nearest place on it", said(ramp.start_moved), object.name)));
        }
        match ramp.short {
            Some(RampShort::LineEnded { more }) => {
                let text = format!("the ramp ends at RL {}, short of {to_words}: the line ends there, {} m before it would get there at {} %", said(ramp.to_rl), said(more), said(ramp.grade.abs()));
                outcome.notes.push(Note::warning(Code::RampShortOfTarget, text).measured(more, "m").at(Place::Point { at: end }).on_made(0));
            }
            Some(RampShort::NeverMet) => {
                let text = format!("the ramp ends at RL {} without meeting {to_words}: {} there", said(ramp.to_rl), if line.is_some() { "the line ends" } else { "the surface ends" });
                outcome.notes.push(Note::warning(Code::RampShortOfTarget, text).at(Place::Point { at: end }).on_made(0));
            }
            None => {}
        }
        Ok(outcome)
    }
}
