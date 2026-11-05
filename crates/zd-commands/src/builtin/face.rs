use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, FaceDesign, FaceDirection, FaceLine, FaceProfile, LineOn, MOST_TOLERANCE, ObjectId, ObjectKind, Polyline, ProfileStep, Ramp, RampBerm, RampFixed, RampHeading, RuleValue, StepTarget, StepValue, Surface, TriMesh, WallSide};
use zd_geom::{COINCIDENT, EdgeRule, Face, FaceBench, FaceRamp, FaceRoad, FaceStep, FaceTarget, GeomError, OffsetSide, RoadTop, SolidRegion, at_line_heights, conditioned_toe, find_crossings, level_floor, next_toe, plan_crossings, project_face, project_wall, register_toe, run_on, triangulate_grid};

use super::{SideParam, in_parallel};
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
/// The attributes on a road's edges: its ramp, and the face it's cut into, so it can be shown with it.
const ROAD: &str = "road";
const ROAD_FACE: &str = "road_face";
/// Where a road came out on a crest, so a design run from that crest carries it on: `road_top A pit` and
/// `road_top A wall` are its top corners as "x y z", then its heading along the string, width and grade
/// (decision 0041).
const ROAD_TOP: &str = "road_top";
/// On each wall's toe and crest, the row of its profile it is, from 1 as the panel numbers them, so a ramp placed on a
/// wall's line can be put on that wall.
const STEP_ROW: &str = "step";

fn road_top_key(ramp: &str, what: &str) -> String {
    format!("{ROAD_TOP} {ramp} {what}")
}

/// What a road leaves on the crest it came out on.
fn road_top_attributes(ramp: &Ramp, road: &FaceRoad) -> Attributes {
    let point = |p: Option<&[f64; 3]>| p.map(|p| AttrValue::Text(format!("{} {} {}", p[0], p[1], p[2])));
    let heading = AttrValue::Text(if road.back { "back" } else { "along" }.into());
    [("pit", point(road.pit_edge.vertices.last())), ("wall", point(road.wall_edge.vertices.last())), ("heading", Some(heading)), ("width", Some(AttrValue::Number(ramp.width))), ("grade", Some(AttrValue::Number(ramp.grade)))]
        .into_iter()
        .filter_map(|(what, value)| Some((road_top_key(&ramp.name, what), value?)))
        .collect()
}

/// Where the design below left a ramp's road on a line, and whether it was running back against the string.
fn road_top_on(attributes: &Attributes, ramp: &str) -> Option<(RoadTop, bool)> {
    let point = |what: &str| -> Option<[f64; 3]> {
        let AttrValue::Text(text) = attributes.get(&road_top_key(ramp, what))? else { return None };
        let numbers: Vec<f64> = text.split_whitespace().map(str::parse).collect::<Result<_, _>>().ok()?;
        match numbers[..] {
            [x, y, z] if numbers.iter().all(|c| c.is_finite()) => Some([x, y, z]),
            _ => None,
        }
    };
    let back = attributes.get(&road_top_key(ramp, "heading")) == Some(&AttrValue::Text("back".into()));
    Some((RoadTop { pit: point("pit")?, wall: point("wall")? }, back))
}

/// How far a line passes from a point, in plan (m).
fn plan_gap_to(line: &Polyline, p: [f64; 3]) -> f64 {
    line.vertices
        .windows(2)
        .map(|w| {
            let (d, q) = ([w[1][0] - w[0][0], w[1][1] - w[0][1]], [p[0] - w[0][0], p[1] - w[0][1]]);
            let length = d[0] * d[0] + d[1] * d[1];
            let t = if length > 0.0 { ((q[0] * d[0] + q[1] * d[1]) / length).clamp(0.0, 1.0) } else { 0.0 };
            (q[0] - t * d[0]).hypot(q[1] - t * d[1])
        })
        .fold(f64::INFINITY, f64::min)
}

/// How far a road's top may be off the line it carries on from before the run says so (m): further than conditioning
/// can move a line.
const TOP_OFF_LINE: f64 = MOST_TOLERANCE;
/// How close a registered toe has to be to the line it came from, past what conditioning moved it, to be on it (m).
const ON_THE_LINE: f64 = 1e-6;

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
        run_design(inputs, p.design, design)
    }
}

/// `face.step`: a face design saved and run in one go, so each step of a design built a step at a time is one undo.
/// A step design is a face design on one string, run whole each time a step is added or changed, so a change low down
/// flows up to the top (the Step design plan in the build plan).
pub struct Step;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepParams {
    /// The design to revise. A step design is saved empty first, as what a run makes is marked with its id.
    pub design: ObjectId,
    pub name: String,
    pub content: FaceDesign,
}

impl Command for Step {
    type Params = StepParams;
    const NAME: &'static str = "face.step";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Save a face design and run it, as one step",
            inputs: vec![Field::object("design", ObjectKind::FaceDesign, "The design to revise and run")],
            params: vec![Field::new("name", FieldType::Text, "Name of the design"), Field::new("content", FieldType::Text, "Lines, profiles and rules")],
            outputs: vec![Field::object("design", ObjectKind::FaceDesign, "The saved design"), Field::object("faces", ObjectKind::Surface, "One face per line, with its toe and crest strings")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let object = inputs.object(p.design)?;
        if !matches!(object.body, Body::FaceDesign(_)) {
            return Err(CommandError::WrongKind { id: p.design, expected: ObjectKind::FaceDesign, found: object.kind() });
        }
        let mut outcome = run_design(inputs, p.design, &p.content)?;
        let body = Body::FaceDesign(Arc::new(p.content.clone()));
        outcome.changes.insert(0, Change::Revise { id: p.design, base: object.revision, name: Some(p.name.clone()), layer: None, attributes: None, body: Some(body) });
        Ok(outcome)
    }
}

/// Every line of a design through its profile, in place of what the last run of the design made.
fn run_design(inputs: &Inputs<'_>, design_id: ObjectId, design: &FaceDesign) -> Result<Outcome, CommandError> {
    let problems = design.problems();
    if !problems.is_empty() {
        return Err(CommandError::Invalid(problems.join("\n")));
    }
    let mut targets = Targets { inputs, grids: HashMap::new() };
    for id in design.profiles.iter().flat_map(surfaces_of) {
        targets.prepare(id)?;
    }
    let regions = solid_regions(inputs, design)?;
    let mut changes: Vec<Change> = inputs.objects().filter(|o| o.attributes.get(MADE_BY) == Some(&AttrValue::Text(design_id.to_string()))).map(|o| Change::Delete { id: o.id, base: o.revision }).collect();
    let mut messages = Vec::new();
    let ramp_strings: HashMap<ObjectId, &Polyline> = design.profiles.iter().flat_map(|profile| &profile.ramps).filter(|ramp| ramp.fixed != RampFixed::Line).filter_map(|ramp| Some((ramp.string?, inputs.string(ramp.string?).ok()?.1))).collect();
    let mut crossing: HashSet<(&str, &str)> = HashSet::new();
    let mut not_crossing: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
    let mut runs: Vec<(&FaceLine, ObjectId)> = Vec::new();
    for line in &design.lines {
        for id in strings_of(inputs, line, design_id, &ramp_strings)? {
            if !runs.iter().any(|(other, already)| *already == id && other.profile == line.profile) {
                runs.push((line, id));
            }
        }
    }
    for &(_, id) in &runs {
        inputs.string(id)?;
    }
    // Strips stand alone, so they run side by side.
    let run = FaceRun { design, id: design_id, inputs, targets: &targets, regions: &regions, ramp_strings: &ramp_strings };
    let mut not_built = Vec::new();
    for strip in in_parallel(&runs, |&(line, id)| run.strip(line, id)) {
        let strip = match strip {
            Ok(strip) => strip,
            Err(CommandError::Invalid(why)) => {
                not_built.push(why.replace("invalid input: ", ""));
                continue;
            }
            Err(e) => return Err(e),
        };
        changes.extend(strip.changes);
        messages.extend(strip.messages);
        crossing.extend(strip.crossing.iter().map(|ramp| (strip.profile, *ramp)));
        for ramp in &strip.missing {
            not_crossing.entry((strip.profile, *ramp)).or_default().push(strip.name);
        }
    }
    // A strip that can't be built leaves the others standing, and only a run with nothing to show is refused.
    if !not_built.is_empty() && not_built.len() == runs.len() {
        return Err(CommandError::Invalid(not_built.join("\n")));
    }
    messages.extend(not_built.iter().map(|why| format!("{why}. {NOT_BUILT}")));
    for profile in &design.profiles {
        for ramp in &profile.ramps {
            let key = (profile.name.as_str(), ramp.name.as_str());
            if profile.walls_of(&ramp.name).is_none() {
                messages.push(format!("Ramp {}: it isn't on a wall yet, so it made no road", ramp.name));
            } else if ramp.fixed == RampFixed::Line {
                if !crossing.contains(&key) {
                    messages.push(format!("Ramp {}: no road {} came out on any line using {}, so it has nothing to carry on from", ramp.name, ramp.name, profile.name));
                } else if let Some(missed) = not_crossing.get(&key) {
                    messages.push(format!("Ramp {}: no road {} came out on {}, so it has nothing to carry on from there", ramp.name, ramp.name, missed.join(", ")));
                }
            } else if !ramp.string.is_some_and(|id| ramp_strings.contains_key(&id)) {
                messages.push(format!("Ramp {}: its string has gone, so it made no road", ramp.name));
            } else if !crossing.contains(&key) {
                messages.push(format!("Ramp {}: its string doesn't cross any line using {}", ramp.name, profile.name));
            } else if let Some(missed) = not_crossing.get(&key) {
                messages.push(format!("Ramp {}: its string doesn't cross {}", ramp.name, missed.join(", ")));
            }
        }
    }
    let mut outcome = Outcome::new(changes);
    for message in messages {
        outcome = outcome.message(message);
    }
    Ok(outcome)
}

/// How the run ends a line about a strip it couldn't build. The panel shows these in red.
const NOT_BUILT: &str = "Nothing is built on this string";

/// What every line of a run shares.
struct FaceRun<'a, 'b> {
    design: &'a FaceDesign,
    /// The design's own id, which marks what the run makes.
    id: ObjectId,
    inputs: &'a Inputs<'b>,
    targets: &'a Targets<'a, 'b>,
    regions: &'a HashMap<ObjectId, SolidRegion>,
    ramp_strings: &'a HashMap<ObjectId, &'a Polyline>,
}

/// What one strip's run made and has to say, and the ramps that did and didn't cross it.
struct StripRun<'a> {
    name: &'a str,
    profile: &'a str,
    changes: Vec<Change>,
    messages: Vec<String>,
    crossing: Vec<&'a str>,
    missing: Vec<&'a str>,
}

impl<'a> FaceRun<'a, '_> {
    /// One line on one string: a face, its toes and crests, and any roads.
    fn strip(&self, line: &'a FaceLine, id: ObjectId) -> Result<StripRun<'a>, CommandError> {
        let (design, inputs, targets) = (self.design, self.inputs, self.targets);
        let (string, path) = inputs.string(id)?;
        let profile = design.profile(&line.profile).ok_or_else(|| CommandError::Invalid(format!("no profile called {}", line.profile)))?;
        let base = format!("{} {}", string.name, line.profile);
        let refuse = |e: GeomError| CommandError::Invalid(format!("{base}: {e}"));
        let extended = run_on(path, line.run_on).map_err(refuse)?;
        let shaped = match profile.condition {
            Some(tolerance) if tolerance > 0.0 => conditioned_toe(&extended, tolerance).map_err(refuse)?,
            _ => extended.clone(),
        };
        let floor;
        let registered = match profile.register {
            StepTarget::Surface { id } => targets.mesh(id)?,
            StepTarget::Rl { rl } => {
                floor = level_floor(&shaped, rl);
                &floor
            }
        };
        let (mut toe, register) = register_toe(&shaped, registered).map_err(refuse)?;
        // A crest another design's run made is already where that design left it, so a design run from it starts from
        // the same line as one design would, and only its run-on lies on the grid.
        if string.attributes.get(MADE_BY).is_some_and(|by| *by != AttrValue::Text(self.id.to_string())) {
            toe = at_line_heights(&toe, path, profile.condition.unwrap_or(0.0) + ON_THE_LINE);
        }
        let (side, faces) = match line.side {
            WallSide::Left => (OffsetSide::Left, OffsetSide::Right),
            WallSide::Right => (OffsetSide::Right, OffsetSide::Left),
        };
        let rules = LineRules { design, string: path, string_name: &string.name, faces, regions: self.regions, inputs };
        // Where a ramp lands on this strip comes from the strip's face with no roads on it.
        let mut placed = PlacedRamps::default();
        if profile.ramps.iter().any(|ramp| ramp.fixed != RampFixed::Line && profile.walls_of(&ramp.name).is_some()) {
            let none = walls_ramps(&profile.steps, &[]);
            let plain = project_face(&toe, registered, side, profile.direction, &steps_of(&rules, &profile.steps, targets, &none)?, &[]).map_err(refuse)?;
            placed = place_ramps(profile, &plain, self.ramp_strings, &string.name);
        }
        carry_on_ramps(profile, &toe, &string.attributes, &string.name, &mut placed);
        let ramps: Vec<&Ramp> = placed.on_strip.iter().map(|(ramp, _)| *ramp).collect();
        let specs: Vec<FaceRamp> = placed.on_strip.iter().map(|(_, spec)| *spec).collect();
        let on_walls = walls_ramps(&profile.steps, &ramps);
        let steps = steps_of(&rules, &profile.steps, targets, &on_walls)?;
        let face = project_face(&toe, registered, side, profile.direction, &steps, &specs).map_err(refuse)?;
        let mut changes = Vec::new();
        let mut messages = vec![line_report(&base, &face, [extended.vertices.len(), shaped.vertices.len()], register.off_grid)];
        messages.extend(placed.notes);
        let names: Vec<&str> = ramps.iter().map(|ramp| ramp.name.as_str()).collect();
        messages.extend((0..ramps.len()).map(|r| ramp_report(&names, &string.name, r, &face)));
        // What a run makes keeps its string's attributes, so a strip's number carries on to its face. Not the roads that
        // came out on the string, which this run has carried on or left, nor the row it was in its own design.
        let attributes = || {
            let mut kept: Attributes = string.attributes.iter().filter(|(key, _)| !key.starts_with(&format!("{ROAD_TOP} ")) && key.as_str() != STEP_ROW).map(|(key, value)| (key.clone(), value.clone())).collect();
            kept.insert(MADE_BY.to_string(), AttrValue::Text(self.id.to_string()));
            kept
        };
        let create = |name: String, body: Body| Change::Create { name, layer: design.layer.clone(), attributes: attributes(), body };
        // A wall going down starts from its crest and comes down to its toe.
        let (start, end) = match profile.direction {
            FaceDirection::Up => ("toe", "crest"),
            FaceDirection::Down => ("crest", "toe"),
        };
        // A road that came out on a crest leaves its top there, for a design run from that crest to carry it on. One that
        // stopped short has nowhere to go, and going down waits.
        let mut tops: HashMap<usize, Attributes> = HashMap::new();
        if profile.direction == FaceDirection::Up {
            for road in face.roads.iter().filter(|road| !face.no_roads.iter().any(|stopped| stopped.ramp == road.ramp)) {
                if let Some(k) = face.benches.iter().position(|bench| bench.step == road.steps[1]) {
                    tops.entry(k).or_default().extend(road_top_attributes(ramps[road.ramp], road));
                }
            }
        }
        for (k, bench) in face.benches.iter().enumerate() {
            let row = (STEP_ROW.to_string(), AttrValue::Number((bench.step + 1) as f64));
            let mut toe = create(format!("{base} {start} {}", k + 1), Body::String(bench.toe.clone()));
            let mut crest = create(format!("{base} {end} {}", k + 1), Body::String(bench.crest.clone()));
            for line in [&mut toe, &mut crest] {
                if let Change::Create { attributes, .. } = line {
                    attributes.extend([row.clone()]);
                }
            }
            if let (Change::Create { attributes, .. }, Some(top)) = (&mut crest, tops.remove(&k)) {
                attributes.extend(top);
            }
            changes.push(toe);
            changes.push(crest);
        }
        // A design ending on a berm hands on the berm's line, where the next wall would start.
        if let Some(line) = &face.berm_line {
            changes.push(create(format!("{base} next {start}"), Body::String(line.clone())));
        }
        for road in &face.roads {
            let name = &ramps[road.ramp].name;
            let edge = |which: &str, line: &Polyline| {
                let mut marked = attributes();
                marked.insert(ROAD.to_string(), AttrValue::Text(name.clone()));
                marked.insert(ROAD_FACE.to_string(), AttrValue::Text(format!("{base} face")));
                Change::Create { name: format!("{base} road {name} {which} edge"), layer: design.layer.clone(), attributes: marked, body: Body::String(line.clone()) }
            };
            changes.push(edge("pit", &road.pit_edge));
            changes.push(edge("wall", &road.wall_edge));
        }
        let crossings = find_crossings(&face.surface, COINCIDENT);
        if crossings.pairs > 0 {
            messages.push(format!("{base}: {} pairs of face triangles cross", crossings.pairs));
        }
        changes.push(create(format!("{base} face"), Body::Surface(Surface::new(face.surface).with_crossings(crossings))));
        Ok(StripRun { name: &string.name, profile: &profile.name, changes, messages, crossing: placed.crossing, missing: placed.missing })
    }
}

/// How a profile's ramps land on one strip.
#[derive(Default)]
struct PlacedRamps<'r> {
    /// Those with a road here, and where.
    on_strip: Vec<(&'r Ramp, FaceRamp)>,
    /// Each one whose string crosses this strip, road or not, and each one whose doesn't.
    crossing: Vec<&'r str>,
    missing: Vec<&'r str>,
    notes: Vec<String>,
}

/// Where each ramp lands on a strip: where its string crosses the toe of its first wall. With its end held
/// that says how far along the strip it comes out, so either way a strip's own stretch of string sets its road.
fn place_ramps<'r>(profile: &'r FaceProfile, plain: &Face, ramp_strings: &HashMap<ObjectId, &Polyline>, strip: &str) -> PlacedRamps<'r> {
    let mut placed = PlacedRamps::default();
    for ramp in profile.ramps.iter().filter(|ramp| ramp.fixed != RampFixed::Line) {
        let (Some([first, last]), Some(string)) = (profile.walls_of(&ramp.name), ramp.string.and_then(|id| ramp_strings.get(&id))) else { continue };
        let toe = plain.benches.iter().position(|bench| bench.step >= first && bench.step <= last).map(|k| first_toe(&plain.benches, k));
        let crossings = toe.map_or_else(Vec::new, |toe| plan_crossings(string, toe));
        let Some(crossed) = crossings.first() else {
            placed.missing.push(&ramp.name);
            continue;
        };
        placed.crossing.push(&ramp.name);
        if crossings.len() > 1 {
            placed.notes.push(format!("Ramp {}: {strip}: its string crosses {} times, and the road goes where it first does", ramp.name, crossings.len()));
        }
        placed.on_strip.push((ramp, face_ramp(ramp, crossed.at)));
    }
    placed
}

/// Where a ramp's first wall starts on the face built with no roads, which places the ramps. A wall going straight on from
/// the one below, at its batter with no berm between, is run as one wall with it there, as no roads set them apart, so
/// the bench shares the lower one's toe. A road placed on that crest starts from the crest, as the run with roads has it.
fn first_toe(benches: &[FaceBench], k: usize) -> &Polyline {
    match k.checked_sub(1).map(|j| &benches[j]) {
        Some(below) if below.step + 1 == benches[k].step && below.toe == benches[k].toe => &below.crest,
        _ => &benches[k].toe,
    }
}

/// The ramps that carry on from the line: each starts where the design below left its road on this strip's string,
/// heading the way it was. One that isn't on the first wall starts afresh on its first wall, from the nearest place.
fn carry_on_ramps<'r>(profile: &'r FaceProfile, toe: &Polyline, attributes: &Attributes, strip: &str, placed: &mut PlacedRamps<'r>) {
    let first_wall = profile.steps.iter().position(|step| matches!(step, ProfileStep::Project { .. }));
    for ramp in profile.ramps.iter().filter(|ramp| ramp.fixed == RampFixed::Line) {
        let Some([first, _]) = profile.walls_of(&ramp.name) else { continue };
        let Some((top, back)) = road_top_on(attributes, &ramp.name) else {
            placed.missing.push(&ramp.name);
            continue;
        };
        placed.crossing.push(&ramp.name);
        let off = plan_gap_to(toe, top.pit).max(plan_gap_to(toe, top.wall));
        if off > TOP_OFF_LINE {
            placed.notes.push(format!("Ramp {}: {strip}: its top is {off:.1} m off this line, so it carries on from the nearest place on it", ramp.name));
        }
        if Some(first) != first_wall {
            placed.notes.push(format!("Ramp {}: {strip}: it isn't on the first wall, so it starts afresh on step {} from the nearest place to its top", ramp.name, first + 1));
        }
        placed.on_strip.push((ramp, FaceRamp { back, carries_on: Some(top), ..face_ramp(ramp, [top.wall[0], top.wall[1]]) }));
    }
}

/// The strings a line runs on. For a layer that's what is on it now, hidden or not, less ramp strings and what
/// this design made last time, which the run is about to replace.
fn strings_of(inputs: &Inputs<'_>, line: &FaceLine, design: ObjectId, ramp_strings: &HashMap<ObjectId, &Polyline>) -> Result<Vec<ObjectId>, CommandError> {
    let layer = match line.on {
        LineOn::String(id) => return Ok(vec![id]),
        LineOn::Layer(layer) => layer,
    };
    let made_here = AttrValue::Text(design.to_string());
    let strings: Vec<ObjectId> = inputs.objects_on(layer)?.into_iter().filter(|o| matches!(o.body, Body::String(_)) && o.attributes.get(MADE_BY) != Some(&made_here) && !ramp_strings.contains_key(&o.id)).map(|o| o.id).collect();
    if strings.is_empty() {
        return Err(CommandError::Invalid("A line names a layer with no strings on it".to_string()));
    }
    Ok(strings)
}

fn face_ramp(ramp: &Ramp, at: [f64; 2]) -> FaceRamp {
    let ends_at = (ramp.fixed == RampFixed::End).then_some(at);
    FaceRamp { start: at, back: ramp.heading == RampHeading::Back, width: ramp.width, grade: ramp.grade / 100.0, ends_at, after_berm: ramp.berm == RampBerm::After, turn_join: ramp.turn_join, turn_flat: ramp.flat_before_turn(), carries_on: None, turn_end: ramp.box_end() }
}

/// The surfaces a profile names.
fn surfaces_of(profile: &FaceProfile) -> Vec<ObjectId> {
    let mut ids = Vec::new();
    if let StepTarget::Surface { id } = profile.register {
        ids.push(id);
    }
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

/// The solids a design's rules go by, each ready to ask once.
fn solid_regions(inputs: &Inputs<'_>, design: &FaceDesign) -> Result<HashMap<ObjectId, SolidRegion>, CommandError> {
    let mut regions = HashMap::new();
    for rule in &design.rules {
        if let RuleValue::InsideSolid { solid, .. } = rule.value {
            let object = inputs.object(solid)?;
            let Body::Solid(body) = &object.body else {
                return Err(CommandError::WrongKind { id: solid, expected: ObjectKind::Solid, found: object.kind() });
            };
            regions.insert(solid, SolidRegion::new(body.mesh()).map_err(|e| CommandError::Invalid(format!("{}: {e}", rule.name)))?);
        }
    }
    Ok(regions)
}

/// What a line's rules look at.
struct LineRules<'a, 'b> {
    design: &'a FaceDesign,
    string: &'a Polyline,
    string_name: &'a str,
    /// The side of the string its walls look towards.
    faces: OffsetSide,
    regions: &'a HashMap<ObjectId, SolidRegion>,
    inputs: &'a Inputs<'b>,
}

impl<'a> LineRules<'a, '_> {
    /// A step's value along a line. `horizon` is the surface the row finishes on.
    fn edge_rule(&self, given: &'a StepValue, horizon: Option<ObjectId>) -> Result<EdgeRule<'a>, CommandError> {
        let name = match given {
            StepValue::Number(value) => return Ok(EdgeRule::Fixed(*value)),
            StepValue::Rule(name) => name,
        };
        let rule = self.design.rule(name).ok_or_else(|| CommandError::Invalid(format!("no rule called {name}")))?;
        Ok(match &rule.value {
            RuleValue::Fixed { value } => EdgeRule::Fixed(*value),
            RuleValue::FixedList { values } => {
                let n = self.string.vertices.len();
                let segments = if self.string.closed { n } else { n.saturating_sub(1) };
                if values.len() != segments {
                    return Err(CommandError::Invalid(format!("{name} has {} values for the {segments} segments of {}", values.len(), self.string_name)));
                }
                EdgeRule::List { string: self.string, values }
            }
            RuleValue::InsidePolygon { polygon, inside, outside } => {
                let (object, line) = self.inputs.string(*polygon)?;
                if !line.closed {
                    return Err(CommandError::Invalid(format!("{name} needs a closed string, and {} is open", object.name)));
                }
                EdgeRule::Polygon { polygon: line, inside: *inside, outside: *outside }
            }
            RuleValue::InsideSolid { solid, inside, outside } => {
                let region = self.regions.get(solid).ok_or_else(|| CommandError::Invalid(format!("{name} has no solid")))?;
                EdgeRule::Solid { region, inside: *inside, outside: *outside, upside_down: false }
            }
            RuleValue::SegmentAzimuth { sectors, otherwise } => EdgeRule::Azimuth { string: self.string, faces: self.faces, sectors, otherwise: *otherwise },
            RuleValue::ByHorizon { values, otherwise } => {
                let value = values.iter().find(|v| Some(v.surface) == horizon).map_or(*otherwise, |v| v.value);
                EdgeRule::Fixed(value)
            }
        })
    }
}

/// Which of a strip's ramps climb each row, and which of those turn back before it, as places in `ramps`.
fn walls_ramps(steps: &[ProfileStep], ramps: &[&Ramp]) -> Vec<[Vec<usize>; 2]> {
    let places = |named: &[String]| named.iter().filter_map(|name| ramps.iter().position(|r| r.name == *name)).collect();
    steps.iter().map(|step| if let ProfileStep::Project { ramps: named, turns, .. } = step { [places(named), places(turns)] } else { [Vec::new(), Vec::new()] }).collect()
}

fn steps_of<'t>(rules: &LineRules<'t, '_>, steps: &'t [ProfileStep], targets: &'t Targets<'_, '_>, on_walls: &'t [[Vec<usize>; 2]]) -> Result<Vec<FaceStep<'t>>, CommandError> {
    let surface_of = |to: &StepTarget| match to {
        StepTarget::Surface { id } => Some(*id),
        StepTarget::Rl { .. } => None,
    };
    // What the wall below a berm reached is the berm's horizon, unless it names its own.
    let mut reached = None;
    let target = |to: &StepTarget| match to {
        StepTarget::Surface { id } => targets.mesh(*id).map(FaceTarget::Surface),
        StepTarget::Rl { rl } => Ok(FaceTarget::Rl(*rl)),
    };
    steps
        .iter()
        .zip(on_walls)
        .map(|(step, [ramps, turns])| {
            Ok(match step {
                ProfileStep::Project { to, batter, condition, .. } => {
                    reached = surface_of(to);
                    FaceStep::Project { to: target(to)?, batter: rules.edge_rule(batter, reached)?, condition: *condition, ramps, turns }
                }
                ProfileStep::Berm { on, width, register, condition, min_bench } => {
                    let horizon = on.as_ref().map_or(reached, surface_of);
                    FaceStep::Berm { on: on.as_ref().map(target).transpose()?, width: rules.edge_rule(width, horizon)?, register: *register, condition: *condition, min_bench: *min_bench }
                }
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
        if let Some(first) = step.no_berm.first() {
            report += &format!("; step {number}: no berm along {} stretch(es) under the minimum bench, the first from {:.1}, {:.1}", step.no_berm.len(), first.from[0], first.from[1]);
        }
    }
    report
}

/// How far one ramp got, as a line of its own that the panel shows against it. `names` are the profile's ramps.
fn ramp_report(names: &[&str], strip: &str, ramp: usize, face: &Face) -> String {
    let name = names[ramp];
    let mut parts = Vec::new();
    for road in face.roads.iter().filter(|road| road.ramp == ramp) {
        let (Some(bottom), Some(top)) = (road.pit_edge.vertices.first(), road.pit_edge.vertices.last()) else { continue };
        let steps = if road.steps[0] == road.steps[1] { format!("step {}", road.steps[0] + 1) } else { format!("steps {} to {}", road.steps[0] + 1, road.steps[1] + 1) };
        parts.push(format!("{steps}, {:.1} m from RL {:.1} to {:.1}, starts at {:.1}, {:.1}", road.pit_edge.plan_length(), bottom[2], top[2], bottom[0], bottom[1]));
        if let Some(miss) = road.off_end {
            parts.push(format!("it comes out {miss:.1} m from where it's held"));
        }
        for (at, steepest) in &road.steep_flats {
            parts.push(format!("its flat before the turn at {:.1}, {:.1} follows the floor at up to {:.1} %, steeper than the road", at[0], at[1], steepest * 100.0));
        }
    }
    for stopped in face.no_roads.iter().filter(|stopped| stopped.ramp == ramp) {
        let why = match stopped.crosses.and_then(|other| names.get(other)) {
            Some(other) => format!("it would cross ramp {other} running the other way on this wall, or come within a road's width of it: move one of them along the strip"),
            None => stopped.why.clone(),
        };
        parts.push(format!("stopped on step {} at {:.1}, {:.1}: {why}", stopped.step + 1, stopped.at[0], stopped.at[1]));
    }
    format!("Ramp {name}: {strip}: {}", parts.join("; "))
}
