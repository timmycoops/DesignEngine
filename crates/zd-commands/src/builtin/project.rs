//! `string.project` (decision 0079): a new string from each string picked, at a batter, on a side, up or down, to an
//! RL, a height above or below, a grid or a surface. The standard project of a design package, as a string tool: no
//! build, no face design, nothing registered first. Each string goes as it's drawn, at its own heights, and is left
//! as it is.

use std::collections::HashSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, FaceDirection, ObjectId, ObjectKind, Surface, TriMesh};
use zd_geom::{COINCIDENT, OffsetSide, ProjectTo, ProjectedString, find_crossings, project_string, triangulate_grid};

use super::SideParam;
use super::face::{crossing_note, stretches_note};
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

pub struct ProjectString;

/// Where the strings are projected to.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectToParam {
    /// One RL for every string.
    Rl { rl: f64 },
    /// So many metres above or below each string, all the way along it (m).
    Height { height: f64 },
    /// A surface or a grid.
    Surface { id: ObjectId },
}

fn up() -> FaceDirection {
    FaceDirection::Up
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStringParams {
    /// The strings to project, each on its own, as one run.
    pub strings: Vec<ObjectId>,
    pub to: ProjectToParam,
    /// Degrees from horizontal, square to the string in plan.
    pub batter: f64,
    /// The side the new string comes out on, looking along each string, whether it goes up or down.
    pub side: SideParam,
    #[serde(default = "up")]
    pub direction: FaceDirection,
    /// Keep the wall between each string and its new one, as a surface.
    #[serde(default)]
    pub wall: bool,
    /// Defaults to each string's own layer.
    #[serde(default)]
    pub layer: Option<String>,
    /// Starts the new names; each string's own name by default.
    #[serde(default)]
    pub name: Option<String>,
}

/// What a new string takes from the one it's projected from: the strip it's on, and the pit (decision 0029).
const KEPT: [&str; 2] = ["strip", "pit"];

impl Command for ProjectString {
    type Params = ProjectStringParams;
    const NAME: &'static str = "string.project";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Project strings at a batter, up or down, to an RL, a height, a grid or a surface",
            inputs: vec![Field::new("strings", FieldType::Objects, "The strings to project, each on its own, as one run")],
            params: vec![
                Field::new("to", FieldType::Text, "An RL, a height above or below each string, or a surface or grid"),
                Field::new("batter", FieldType::Number { unit: "deg" }, "From horizontal, square to the string in plan"),
                Field::new("side", FieldType::Text, "left or right, looking along each string: where the new string comes out"),
                Field::new("direction", FieldType::Text, "up (default) or down"),
                Field::new("wall", FieldType::Bool, "Keep the wall between each string and its new one, as a surface"),
                Field::new("layer", FieldType::Layer, "Layer for what's made; each string's own by default"),
                Field::new("name", FieldType::Text, "Starts the new names; each string's own name by default"),
            ],
            outputs: vec![
                Field::object("strings", ObjectKind::String, "Each string's new line, one per piece"),
                Field::object("walls", ObjectKind::Surface, "The wall between each string and its line, if kept"),
            ],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.strings.is_empty() {
            return Err(CommandError::Invalid("no strings to project".into()));
        }
        let mut seen = HashSet::new();
        if let Some(twice) = p.strings.iter().find(|id| !seen.insert(**id)) {
            return Err(CommandError::Invalid(format!("string {twice} is given twice")));
        }
        let side = match p.side {
            SideParam::Left => OffsetSide::Left,
            SideParam::Right => OffsetSide::Right,
        };
        let down = p.direction == FaceDirection::Down;
        // A grid is its triangulation, made once for every string: projection targets are triangulations.
        let triangulated: TriMesh;
        let (to, to_words) = match p.to {
            ProjectToParam::Rl { rl } => (ProjectTo::Rl(rl), format!("to RL {rl}")),
            ProjectToParam::Height { height } => (ProjectTo::Height(height), format!("{} {height} m", if down { "down" } else { "up" })),
            ProjectToParam::Surface { id } => {
                let target = inputs.object(id)?;
                let mesh = match &target.body {
                    Body::Surface(surface) => surface.mesh(),
                    Body::Grid(grid) => {
                        triangulated = triangulate_grid(grid);
                        &triangulated
                    }
                    other => return Err(CommandError::WrongKind { id, expected: ObjectKind::Surface, found: other.kind() }),
                };
                (ProjectTo::Surface(mesh), format!("to {}", target.name))
            }
        };
        let started = Instant::now();
        let mut made: Vec<(&zd_core::Object, ProjectedString)> = Vec::with_capacity(p.strings.len());
        let mut failed = Vec::new();
        for &id in &p.strings {
            let (object, line) = inputs.string(id)?;
            match project_string(line, side, p.direction, p.batter, to) {
                Ok(projected) => made.push((object, projected)),
                Err(why) => failed.push(format!("'{}': {why}", object.name)),
            }
        }
        // Nothing is half done: a string that can't be projected refuses the run, each one listed with why.
        if !failed.is_empty() {
            return Err(CommandError::Invalid(if p.strings.len() == 1 { failed.remove(0) } else { format!("{} of {} strings can't be projected, so nothing was made. {}", failed.len(), p.strings.len(), failed.join("; ")) }));
        }
        let (mut changes, mut notes) = (Vec::new(), Vec::new());
        let (mut lines, mut vertices, mut walls, mut triangles) = (0, 0, 0, 0);
        let several = made.len() > 1;
        for (k, (object, projected)) in made.into_iter().enumerate() {
            let base = match &p.name {
                Some(name) if several => format!("{name} {}", k + 1),
                Some(name) => name.clone(),
                None => format!("{} {to_words}", object.name),
            };
            let layer = p.layer.clone().unwrap_or_else(|| inputs.layer_name(object));
            let mut attributes = Attributes::new();
            for key in KEPT {
                if let Some(value) = object.attributes.get(key) {
                    attributes.insert(key.to_string(), value.clone());
                }
            }
            let said = format!("'{}' {to_words}", object.name);
            if let Some(first) = projected.log.no_height.first() {
                let text = format!("{said}: no height along {} stretch(es), the first from {:.1}, {:.1}: the new string runs along the string there", projected.log.no_height.len(), first.from[0], first.from[1]);
                notes.push(stretches_note(Code::WallNoHeight, text, &projected.log.no_height).on(object.id));
            }
            if let Some(first) = projected.log.no_target.first() {
                let text = format!("{said}: nothing to meet along {} stretch(es), the first from {:.1}, {:.1}: bridged across", projected.log.no_target.len(), first.from[0], first.from[1]);
                notes.push(stretches_note(Code::WallNoTarget, text, &projected.log.no_target).on(object.id));
            }
            let pieces = projected.lines.len();
            if pieces > 1 {
                let text = format!("{said}: the new string came back in {pieces} pieces, as it pinches off{}", if p.wall { ", so no wall was made for it" } else { "" });
                notes.push(Note::warning(Code::StringProjectedInPieces, text).measured(pieces as f64, "pieces").on(object.id));
            }
            for (i, line) in projected.lines.into_iter().enumerate() {
                lines += 1;
                vertices += line.vertices.len();
                let name = if pieces == 1 { base.clone() } else { format!("{base} ({})", i + 1) };
                changes.push(Change::Create { name, layer: layer.clone(), attributes: attributes.clone(), body: Body::String(line) });
            }
            if p.wall && !projected.wall.triangles.is_empty() {
                let crossings = find_crossings(&projected.wall, COINCIDENT);
                if crossings.pairs > 0 {
                    notes.push(crossing_note(format!("{said}: {} pairs of the wall's triangles cross", crossings.pairs), &crossings).on_made(changes.len()));
                }
                walls += 1;
                triangles += projected.wall.triangles.len();
                changes.push(Change::Create { name: format!("{base} wall"), layer, attributes, body: Body::Surface(Surface::new(projected.wall).with_crossings(crossings)) });
            }
        }
        let plural = |n: usize| if n == 1 { "" } else { "s" };
        let mut message = format!(
            "projected {} string{} {} at {}° {to_words}, on the {}: {lines} string{}, {vertices} vertices",
            p.strings.len(),
            plural(p.strings.len()),
            if down { "down" } else { "up" },
            p.batter,
            if side == OffsetSide::Left { "left" } else { "right" },
            plural(lines),
        );
        if p.wall {
            message += &format!("; {walls} wall{}, {triangles} triangles", plural(walls));
        }
        message += &format!("; {:.0} ms", started.elapsed().as_secs_f64() * 1e3);
        let mut outcome = Outcome::new(changes).message(message);
        outcome.notes.extend(notes);
        Ok(outcome)
    }
}
