use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, FaceDesign, FaceProfile, MOST_TOLERANCE, ObjectId, ObjectKind, ProfileStep, StepTarget, StepValue, Surface, TriMesh, WallSide};
use zd_geom::{COINCIDENT, FaceStep, FaceTarget, GeomError, OffsetSide, conditioned_toe, find_crossings, next_toe, project_face, project_wall, register_toe, run_on, triangulate_grid};

use super::SideParam;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// `face.project_step`: one wall from a string up to a target surface or grid, at a fixed batter. It
/// makes the crest, the wall as a surface, and the string the next step starts from: the crest
/// conditioned in plan, moved out a berm and registered back on the target. The string stays as it is.
pub struct ProjectStep;

fn default_tolerance() -> f64 {
    0.1
}

fn too_much_tolerance(tolerance: f64) -> String {
    format!("a tolerance of {tolerance} m would cut corners off the crest: it's how far the next toe may leave the crest, at most {MOST_TOLERANCE} m. The berm width has its own box")
}

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
            return Err(CommandError::Invalid(too_much_tolerance(p.tolerance)));
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

/// `face.save_design`: a new face design, or the next revision of one. What it holds is only checked
/// when it runs, as a design is saved part way through being set up.
pub struct SaveDesign;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveDesignParams {
    /// The design to revise; a new one if left out.
    #[serde(default)]
    pub design: Option<ObjectId>,
    pub name: String,
    pub content: FaceDesign,
}

/// The layer designs themselves sit on.
const DESIGN_LAYER: &str = "face designs";

impl Command for SaveDesign {
    type Params = SaveDesignParams;
    const NAME: &'static str = "face.save_design";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Save a face design",
            inputs: vec![],
            params: vec![
                Field::new("design", FieldType::Text, "The design to revise; a new one if left out"),
                Field::new("name", FieldType::Text, "Name of the design"),
                Field::new("content", FieldType::Text, "Lines, profiles and rules"),
            ],
            outputs: vec![Field::object("design", ObjectKind::FaceDesign, "The saved design")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let body = Body::FaceDesign(Arc::new(p.content.clone()));
        let change = match p.design {
            Some(id) => {
                let object = inputs.object(id)?;
                if !matches!(object.body, Body::FaceDesign(_)) {
                    return Err(CommandError::WrongKind { id, expected: ObjectKind::FaceDesign, found: object.kind() });
                }
                Change::Revise { id, base: object.revision, name: Some(p.name.clone()), layer: None, attributes: None, body: Some(body) }
            }
            None => Change::Create { name: p.name.clone(), layer: DESIGN_LAYER.into(), attributes: Attributes::new(), body },
        };
        Ok(Outcome::new(vec![change]).message(format!("saved {}", p.name)))
    }
}

/// `face.project`: every line of a design through its profile: run on past its ends, registered and
/// stepped the way the profile goes, in one run and one undo. Each line makes one face surface, with its
/// toe and crest strings. Re-running replaces what the last run of the design made.
pub struct ProjectDesign;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDesignParams {
    pub design: ObjectId,
}

/// The attribute that ties what a run made to its design.
const MADE_BY: &str = "face_design";

impl Command for ProjectDesign {
    type Params = ProjectDesignParams;
    const NAME: &'static str = "face.project";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Run every line of a face design",
            inputs: vec![Field::object("design", ObjectKind::FaceDesign, "The design to run")],
            params: vec![],
            outputs: vec![Field::object("faces", ObjectKind::Surface, "One face per line, with its toe and crest strings")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.design)?;
        let Body::FaceDesign(design) = &object.body else {
            return Err(CommandError::WrongKind { id: p.design, expected: ObjectKind::FaceDesign, found: object.kind() });
        };
        if let Some(problem) = design.problems().into_iter().next() {
            return Err(CommandError::Invalid(problem));
        }
        let mut targets = Targets { inputs, grids: HashMap::new() };
        for id in design.profiles.iter().flat_map(surfaces_of) {
            targets.prepare(id)?;
        }
        let mut changes: Vec<Change> = inputs.objects().filter(|o| o.attributes.get(MADE_BY) == Some(&AttrValue::Text(p.design.to_string()))).map(|o| Change::Delete { id: o.id, base: o.revision }).collect();
        let mut messages = Vec::new();
        for line in &design.lines {
            let (string, path) = inputs.string(line.string)?;
            let profile = design.profile(&line.profile).ok_or_else(|| CommandError::Invalid(format!("no profile called {}", line.profile)))?;
            let base = format!("{} {}", string.name, line.profile);
            let refuse = |e: GeomError| CommandError::Invalid(format!("{base}: {e}"));
            let extended = run_on(path, line.run_on).map_err(refuse)?;
            let shaped = match profile.condition {
                Some(tolerance) if tolerance > 0.0 => conditioned_toe(&extended, tolerance).map_err(refuse)?,
                _ => extended.clone(),
            };
            let registered = targets.mesh(profile.register)?;
            let (toe, register) = register_toe(&shaped, registered).map_err(refuse)?;
            let steps = steps_of(design, &profile.steps, &targets)?;
            let side = match line.side {
                WallSide::Left => OffsetSide::Left,
                WallSide::Right => OffsetSide::Right,
            };
            let face = project_face(&toe, registered, side, profile.direction, &steps).map_err(refuse)?;
            messages.push(line_report(&base, &face, [extended.vertices.len(), shaped.vertices.len()], register.off_grid));
            let attributes = || Attributes::from([(MADE_BY.to_string(), AttrValue::Text(p.design.to_string()))]);
            let create = |name: String, body: Body| Change::Create { name, layer: design.layer.clone(), attributes: attributes(), body };
            for (k, bench) in face.benches.iter().enumerate() {
                changes.push(create(format!("{base} toe {}", k + 1), Body::String(bench.toe.clone())));
                changes.push(create(format!("{base} crest {}", k + 1), Body::String(bench.crest.clone())));
            }
            let crossings = find_crossings(&face.surface, COINCIDENT);
            if crossings.pairs > 0 {
                messages.push(format!("{base}: {} pairs of face triangles cross", crossings.pairs));
            }
            changes.push(create(format!("{base} face"), Body::Surface(Surface::new(face.surface).with_crossings(crossings))));
        }
        let mut outcome = Outcome::new(changes);
        for message in messages {
            outcome = outcome.message(message);
        }
        Ok(outcome)
    }
}

/// The surfaces a profile names.
fn surfaces_of(profile: &FaceProfile) -> Vec<ObjectId> {
    let mut ids = vec![profile.register];
    for step in &profile.steps {
        if let ProfileStep::Project { to: StepTarget::Surface { id }, .. } | ProfileStep::Berm { on: Some(StepTarget::Surface { id }), .. } = step {
            ids.push(*id);
        }
    }
    ids
}

/// Surfaces and grids a run steps to. A grid is triangulated once, however many steps use it.
struct Targets<'a, 'b> {
    inputs: &'a Inputs<'b>,
    grids: HashMap<ObjectId, TriMesh>,
}

impl Targets<'_, '_> {
    fn prepare(&mut self, id: ObjectId) -> Result<(), CommandError> {
        let object = self.inputs.object(id)?;
        match &object.body {
            Body::Surface(_) => Ok(()),
            Body::Grid(grid) => {
                self.grids.entry(id).or_insert_with(|| triangulate_grid(grid));
                Ok(())
            }
            other => Err(CommandError::WrongKind { id, expected: ObjectKind::Surface, found: other.kind() }),
        }
    }

    fn mesh(&self, id: ObjectId) -> Result<&TriMesh, CommandError> {
        if let Some(mesh) = self.grids.get(&id) {
            return Ok(mesh);
        }
        let object = self.inputs.object(id)?;
        match &object.body {
            Body::Surface(surface) => Ok(surface.mesh()),
            other => Err(CommandError::WrongKind { id, expected: ObjectKind::Surface, found: other.kind() }),
        }
    }
}

fn steps_of<'t>(design: &FaceDesign, steps: &[ProfileStep], targets: &'t Targets<'_, '_>) -> Result<Vec<FaceStep<'t>>, CommandError> {
    let value = |given: &StepValue| design.value_of(given).ok_or_else(|| CommandError::Invalid(format!("no rule called {given:?}")));
    let target = |to: &StepTarget| match to {
        StepTarget::Surface { id } => targets.mesh(*id).map(FaceTarget::Surface),
        StepTarget::Rl { rl } => Ok(FaceTarget::Rl(*rl)),
    };
    steps
        .iter()
        .map(|step| {
            Ok(match step {
                ProfileStep::Project { to, batter, condition } => FaceStep::Project { to: target(to)?, batter: value(batter)?, condition: *condition },
                ProfileStep::Berm { on, width, register, condition } => FaceStep::Berm { on: on.as_ref().map(target).transpose()?, width: value(width)?, register: *register, condition: *condition },
            })
        })
        .collect()
}

/// One line's result, naming each place the design needs looking at.
fn line_report(name: &str, face: &zd_geom::Face, [toe_in, toe_out]: [usize; 2], off_grid: usize) -> String {
    let (walls, triangles) = (face.benches.len(), face.surface.triangles.len());
    let mut report = format!("{name}: {walls} walls, {triangles} triangles");
    if toe_out != toe_in {
        report += &format!("; toe conditioned from {toe_in} to {toe_out} vertices");
    }
    if off_grid > 0 {
        report += &format!("; {off_grid} toe vertices off their grid carried on at their grade");
    }
    for step in &face.steps {
        let number = step.step + 1;
        if step.no_wall {
            report += &format!("; step {number} has no height: its target is on or past the string all the way along");
        }
        if let Some(first) = step.no_height.first() {
            report += &format!("; step {number} has no height along {} stretch(es), the first from {:.1}, {:.1}", step.no_height.len(), first.from[0], first.from[1]);
        }
        if let Some(first) = step.no_target.first() {
            report += &format!("; step {number} has no target along {} stretch(es), bridged, the first from {:.1}, {:.1}", step.no_target.len(), first.from[0], first.from[1]);
        }
        if step.off_surface > 0 {
            report += &format!("; step {number}: {} berm vertices have no surface under them and hold their crest's height", step.off_surface);
        }
        if step.pieces_dropped > 0 {
            report += &format!("; step {number}: a berm pinched into pieces and {} were left out", step.pieces_dropped);
        }
    }
    report
}
