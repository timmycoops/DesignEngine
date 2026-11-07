//! `face.build_step`: one step of the manual builder (decision 0048). A wall or a berm from the line you pick, run once, as
//! a design of one row on that line: the roads that came out on the line carry on (decision 0041), and what it makes is
//! ordinary strings and a sheet, yours to edit. Nothing is built again unless you ask. A wall from the line of a berm a step
//! made builds that berm with it, from the crest the berm was built from, as a berm waits for the wall above it to go round
//! the roads' pads (decision 0038). If you've edited the berm's line, the wall stands on your line, as on a line redrawn by
//! hand (decision 0046).

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, FaceDirection, FaceProfile, Object, ObjectId, ObjectKind, Polyline, ProfileStep, Ramp, RampBerm, RampFixed, RampHeading, StepTarget, StepValue, Surface, USUAL_TURN_JOIN, WallSide};
use zd_geom::{COINCIDENT, at_line_heights, EdgeRule, EditReport, Face, FaceEdit, FaceRamp, FaceStep, FaceTarget, GeomError, JoinBoxes, JoinNote, MetBoxes, OffsetSide, WallToCome, find_crossings, level_floor, project_edited_face, register_toe, run_on, conditioned_toe};

use super::SideParam;
use super::face::{MADE_BY, PlacedRamps, ROAD, ROAD_FACE, ROAD_TOP, STEP_ROW, TOP_OFF_LINE, Targets, face_ramp, place_ramps, plan_gap_to, road_top_attributes, road_top_key, road_top_on, walls_ramps};
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

pub struct BuildStep;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildStepParams {
    /// The line to build from: a toe, a crest or a berm's line.
    pub from: ObjectId,
    pub step: BuildOp,
    /// What the line lies on. A line drawn by hand is registered to it. A line a step made says, and keeps its heights, as
    /// does one a design made.
    #[serde(default)]
    pub lies_on: Option<StepTarget>,
    /// The side the walls go up on, looking along the line: as the step that made the line had it, if it doesn't say.
    #[serde(default)]
    pub side: Option<SideParam>,
    /// How far to run the line on past its start and end before building from it (m), so the face is built oversized and the
    /// cut does the fitting: for the first step from a toe, usually. A line a step made is already run on.
    #[serde(default)]
    pub run_on: [f64; 2],
    /// How far the line may move in plan when it's cleaned before building from it (m), as a design's profile cleans its toes;
    /// none leaves it as it is.
    #[serde(default)]
    pub condition: Option<f64>,
    /// Where what's made goes: the line's layer if a step made it, and `builder` if not.
    #[serde(default)]
    pub layer: Option<String>,
    /// The build's name, which starts the new names: the line's build, or its own name.
    #[serde(default)]
    pub name: Option<String>,
}

/// One step: a wall or a berm. Ramps, turns and Join are how each goes about the roads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum BuildOp {
    /// A wall to a surface, a grid or an RL at a batter (degrees). The roads that came out on the line climb it, but for
    /// those it stops, and `ramps` are new ones, each placed by its string across the line or held at its end.
    Project {
        to: StepTarget,
        batter: f64,
        /// How far the crest may move in plan when it's cleaned (m); none leaves it as it landed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        condition: Option<f64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ramps: Vec<Ramp>,
        /// Roads that came out on the line and end there.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        stop: Vec<String>,
    },
    /// A berm of a width (m) on a surface, a grid or an RL: on what the line lies on, if it doesn't say. The roads that came
    /// out on the line cross it, but for those it stops; those in `turns` turn back on it, on a box past their tops (decision
    /// 0038). `join` makes their boxes one pad: on, off, or by themselves where they come within a road's width of each
    /// other, if it doesn't say (decision 0047).
    Berm {
        width: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<StepTarget>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        condition: Option<f64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        turns: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        join: Option<bool>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        stop: Vec<String>,
    },
}

/// How a step built a line, kept on the line for the step after it and for Rebuild: the line it was built from, what that
/// line lies on, the side and the step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StepRecord {
    from: ObjectId,
    from_on: StepTarget,
    side: WallSide,
    step: BuildOp,
    /// How far the line was run on past its ends first (m), and how far it could move when it was cleaned.
    #[serde(default, skip_serializing_if = "no_run_on")]
    run_on: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    condition: Option<f64>,
    /// The fingerprints of the line it was built from and of the line it made, as they were: the step after it knows from
    /// them whether either has been edited since, without building it again.
    from_print: String,
    made_print: String,
    /// The revision of the line it was built from, then: a later one means that line's been edited since, and what this
    /// step made was built from it as it was.
    #[serde(default)]
    from_revision: u32,
    /// The revision the line it made had then: a later one means you've edited it since. 1 as made by a step; one more each
    /// time Rebuild builds it again.
    #[serde(default = "first_revision")]
    made_revision: u32,
}

fn first_revision() -> u32 {
    1
}

/// A line's fingerprint: FNV-1a over the exact bits of its vertices, as 16 hex digits. An edit changes it, and undoing the
/// edit brings it back.
fn fingerprint(line: &Polyline) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let bytes = std::iter::once(u8::from(line.closed)).chain(line.vertices.iter().flatten().flat_map(|c| c.to_bits().to_le_bytes()));
    for byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn no_run_on(ends: &[f64; 2]) -> bool {
    *ends == [0.0, 0.0]
}

/// What a step leaves on what it makes. On every piece: the build's name, the step's number in it from 1, and the step's own
/// id. On its line: what the line lies on, and how it was built, as JSON.
const BUILD: &str = "build";
const BUILD_STEP: &str = "build_step";
const BUILD_ID: &str = "build_id";
const BUILD_ON: &str = "build_on";
const BUILD_RECORD: &str = "build_record";

/// Where the first step from a line drawn by hand puts what it makes, unless it says.
const BUILD_LAYER: &str = "builder";

/// How near a registered vertex has to be to the line it came from to keep the line's height (m).
const ON_THE_LINE: f64 = 1e-6;

/// The batter a berm's line takes the wall above it to stand at, where roads turn back on it, if the wall below it wasn't a
/// step's (degrees): where each next road's flat start meets that wall moves with it (decision 0048).
const USUAL_BATTER: f64 = 65.0;

/// The batter of the wall a crest tops, as the step that built it had it: the wall above the berm on it most likely stands
/// at the same.
fn batter_below(crest: &Object) -> f64 {
    match made_by_a_step(crest).map(|made| made.record.step) {
        Some(BuildOp::Project { batter, .. }) => batter,
        _ => USUAL_BATTER,
    }
}

/// What a step left on a line it made.
struct Made {
    name: String,
    step: usize,
    on: Option<StepTarget>,
    record: StepRecord,
}

fn made_by_a_step(object: &Object) -> Option<Made> {
    let text = |key: &str| match object.attributes.get(key) {
        Some(AttrValue::Text(text)) => Some(text.as_str()),
        _ => None,
    };
    let record = serde_json::from_str(text(BUILD_RECORD)?).ok()?;
    let step = match object.attributes.get(BUILD_STEP) {
        Some(AttrValue::Number(n)) if *n >= 1.0 => *n as usize,
        _ => return None,
    };
    Some(Made { name: text(BUILD)?.to_string(), step, on: text(BUILD_ON).and_then(|on| serde_json::from_str(on).ok()), record })
}

impl Command for BuildStep {
    type Params = BuildStepParams;
    const NAME: &'static str = "face.build_step";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Build one wall or berm from a line",
            inputs: vec![Field::object("from", ObjectKind::String, "The line to build from: a toe, a crest or a berm's line")],
            params: vec![
                Field::new("step", FieldType::Text, "A wall to a target at a batter, with its roads, or a berm of a width, with the roads turning on it"),
                Field::new("lies_on", FieldType::Text, "What the line lies on: a line drawn by hand is registered to it, and a step's line says"),
                Field::new("side", FieldType::Text, "left or right, looking along the line: as the step that made it had it, by default"),
                Field::new("run_on", FieldType::Text, "How far to run the line on past its start and end first [m, m]; none by default"),
                Field::new("condition", FieldType::Number { unit: "m" }, "How far the line may move in plan when it's cleaned first; none by default"),
                Field::new("layer", FieldType::Layer, "Layer for what's made: the line's if a step made it, builder if not"),
                Field::new("name", FieldType::Text, "The build's name: the line's build, or its own name, by default"),
            ],
            outputs: vec![
                Field::object("line", ObjectKind::String, "The wall's crest, or the berm's line"),
                Field::object("wall", ObjectKind::Surface, "The wall, with the berm below it if it stands on one a step made"),
                Field::object("roads", ObjectKind::String, "Each road's pit-side and wall-side edges up the wall"),
            ],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.from)?;
        let side = p.side.map(|side| match side {
            SideParam::Left => WallSide::Left,
            SideParam::Right => WallSide::Right,
        });
        let current = |id: ObjectId| inputs.string(id).ok().map(|(object, line)| (object.clone(), line.clone()));
        let mut targets = Targets::new(inputs);
        let request = Request { object, line, step: &p.step, lies_on: p.lies_on, side, run_on: p.run_on, condition: p.condition, layer: p.layer.clone(), name: p.name.clone(), again: None };
        build_one(inputs, &mut targets, request, &current)
    }
}

/// A step to build: from the command's parameters, or from a step's record when Rebuild builds it again.
struct Request<'r> {
    object: &'r Object,
    line: &'r Polyline,
    step: &'r BuildOp,
    lies_on: Option<StepTarget>,
    side: Option<WallSide>,
    run_on: [f64; 2],
    condition: Option<f64>,
    layer: Option<String>,
    name: Option<String>,
    again: Option<Again>,
}

/// A step built again keeps its id and number, and its line takes its next revision.
#[derive(Clone, Copy)]
struct Again {
    id: ObjectId,
    number: usize,
    made_revision: u32,
}

/// A line by its id as it is now: as Rebuild has just built it again, or as the project has it.
type Current<'c> = dyn Fn(ObjectId) -> Option<(Arc<Object>, Polyline)> + 'c;

/// One step from a line: a berm, a wall, or a wall from a berm's line a step made, which goes up with that berm from the
/// crest the berm was built from.
fn build_one(inputs: &Inputs<'_>, targets: &mut Targets<'_, '_>, r: Request<'_>, current: &Current<'_>) -> Result<Outcome, CommandError> {
    let (object, line) = (r.object, r.line);
    let made = made_by_a_step(object);
    let side = match (r.side, &made) {
        (Some(side), _) => side,
        (None, Some(made)) => made.record.side,
        (None, None) => return Err(CommandError::Invalid(format!("say which side of {} the walls go up on, looking along it", object.name))),
    };
    let Some(lies_on) = r.lies_on.or(made.as_ref().and_then(|made| made.on)) else {
        return Err(CommandError::Invalid(format!("say what {} lies on: a surface, a grid or an RL", object.name)));
    };
    let build = Build {
        inputs,
        name: r.name.or_else(|| made.as_ref().map(|made| made.name.clone())).unwrap_or_else(|| object.name.clone()),
        layer: r.layer.unwrap_or_else(|| if made.is_some() { inputs.layer_name(object) } else { BUILD_LAYER.into() }),
        number: r.again.map_or_else(|| made.as_ref().map_or(1, |made| made.step + 1), |again| again.number),
        id: r.again.map_or_else(ObjectId::new, |again| again.id),
        side,
        from: object.id,
        from_on: lies_on,
        kept: kept_attributes(&object.attributes),
        run_on: r.run_on,
        condition: r.condition.filter(|t| *t > 0.0),
        from_print: fingerprint(line),
        from_revision: object.revision,
        made_revision: r.again.map_or(1, |again| again.made_revision),
    };
    let below = match (r.step, &made) {
        (BuildOp::Project { .. }, Some(Made { record: record @ StepRecord { step: BuildOp::Berm { .. }, .. }, .. })) => Some((record, current(record.from))),
        _ => None,
    };
    let mut uses = vec![lies_on];
    uses.extend(targets_of(r.step));
    if let Some((record, _)) = &below {
        uses.push(record.from_on);
        uses.extend(targets_of(&record.step));
    }
    for target in uses {
        if let StepTarget::Surface { id } = target {
            targets.prepare(id)?;
        }
    }
    let by_hand = drawn_by_hand(object);
    match (r.step, below) {
        (BuildOp::Berm { .. }, _) => build.berm(targets, object, line, by_hand, r.step),
        (BuildOp::Project { .. }, Some((record, Some((crest, crest_line))))) => build.over_berm(targets, object, line, record, (&crest, &crest_line), r.step),
        (BuildOp::Project { .. }, Some((_, None))) => {
            let mut outcome = build.project(targets, object, line, by_hand, r.step)?;
            outcome.messages.insert(1, format!("the crest {} was built from has gone, so the wall stands on it as it is, and no road carries on over the berm", object.name));
            Ok(outcome)
        }
        (BuildOp::Project { .. }, None) => build.project(targets, object, line, by_hand, r.step),
    }
}

/// `face.rebuild`: Rebuild from here (decision 0048). The steps built on a line you've edited, and the steps built on theirs,
/// built again from it as it is now, in order, each with the settings it had. What each made is replaced by its next
/// revision, keeping its id, so one undo takes the whole rebuild back.
pub struct Rebuild;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RebuildParams {
    /// The line to build again from: a toe, a crest or a berm's line, edited or not.
    pub from: ObjectId,
}

impl Command for Rebuild {
    type Params = RebuildParams;
    const NAME: &'static str = "face.rebuild";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Build the steps above a line again, from the line as it is now",
            inputs: vec![Field::object("from", ObjectKind::String, "The line to build again from: a toe, a crest or a berm's line")],
            params: vec![],
            outputs: vec![Field::object("steps", ObjectKind::String, "Each step's line, wall and roads again, as their next revisions")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (root, _) = inputs.string(p.from)?;
        let above = steps_above(inputs, p.from);
        if above.is_empty() {
            return Err(CommandError::Invalid(format!("no step has been built from {}: build one from it first", root.name)));
        }
        let mut pieces: HashMap<String, Vec<Arc<Object>>> = HashMap::new();
        for object in inputs.objects() {
            if let Some(AttrValue::Text(id)) = object.attributes.get(BUILD_ID) {
                pieces.entry(id.clone()).or_default().push(object.clone());
            }
        }
        let mut rebuilt: HashMap<ObjectId, (Arc<Object>, Polyline)> = HashMap::new();
        let mut targets = Targets::new(inputs);
        let mut changes = Vec::new();
        let mut messages = vec![format!("{} step{} built again from {} as it is now", above.len(), if above.len() == 1 { "" } else { "s" }, root.name)];
        for line in &above {
            let made = made_by_a_step(line).expect("listed by its record");
            let Some(AttrValue::Text(step_id)) = line.attributes.get(BUILD_ID) else { unreachable!("a step's line carries its id") };
            let id = ObjectId::parse(step_id).ok_or_else(|| CommandError::Invalid(format!("{} carries a step id that isn't one: {step_id}", line.name)))?;
            // What the step made, read as inputs: their revisions are what the rebuild replaces.
            let old: Vec<Arc<Object>> = pieces.get(step_id).into_iter().flatten().map(|piece| inputs.object(piece.id).cloned()).collect::<Result<_, _>>()?;
            if line.revision > made.record.made_revision {
                messages.push(format!("{} had been edited by hand: it's built again, and undo brings your edit back", line.name));
            }
            let outcome = {
                let current = |id: ObjectId| rebuilt.get(&id).cloned().or_else(|| inputs.string(id).ok().map(|(object, line)| (object.clone(), line.clone())));
                let Some((from, from_line)) = current(made.record.from) else { unreachable!("each step comes after the line it was built from") };
                let record = &made.record;
                let request = Request {
                    object: &from,
                    line: &from_line,
                    step: &record.step,
                    lies_on: Some(record.from_on),
                    side: Some(record.side),
                    run_on: record.run_on,
                    condition: record.condition,
                    layer: Some(inputs.layer_name(line)),
                    name: Some(made.name.clone()),
                    again: Some(Again { id, number: made.step, made_revision: line.revision + 1 }),
                };
                build_one(inputs, &mut targets, request, &current)?
            };
            let (revised, new_line) = as_revisions(line, &old, outcome.changes);
            rebuilt.insert(line.id, new_line);
            changes.extend(revised);
            messages.extend(outcome.messages);
        }
        let mut outcome = Outcome::new(changes);
        outcome.messages = messages;
        Ok(outcome)
    }
}

/// The lines of the steps built on a line, and on theirs, each after the line it was built from: nearest first, then by step.
fn steps_above(inputs: &Inputs<'_>, from: ObjectId) -> Vec<Arc<Object>> {
    let mut built_on: HashMap<ObjectId, Vec<(usize, Arc<Object>)>> = HashMap::new();
    for object in inputs.objects() {
        if let Some(made) = made_by_a_step(object) {
            built_on.entry(made.record.from).or_default().push((made.step, object.clone()));
        }
    }
    let mut seen = std::collections::HashSet::from([from]);
    let (mut queue, mut order) = (vec![from], Vec::new());
    let mut next = 0;
    while let Some(&id) = queue.get(next) {
        next += 1;
        let mut lines = built_on.remove(&id).unwrap_or_default();
        lines.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        for (_, line) in lines {
            if seen.insert(line.id) {
                queue.push(line.id);
                order.push(line);
            }
        }
    }
    order
}

/// A step built again, as revisions of what it made before: its line always, and its wall and roads by name. A road it no
/// longer makes goes; a new one is made. Gives the line as it will be, for the steps built on it.
fn as_revisions(line: &Arc<Object>, old: &[Arc<Object>], made: Vec<Change>) -> (Vec<Change>, (Arc<Object>, Polyline)) {
    let mut unused: Vec<&Arc<Object>> = old.iter().filter(|piece| piece.id != line.id).collect();
    let mut changes = Vec::new();
    let mut new_line = None;
    for change in made {
        let Change::Create { name, layer, attributes, body } = change else {
            changes.push(change);
            continue;
        };
        let is_line = attributes.contains_key(BUILD_RECORD);
        let was = if is_line { Some(line) } else { unused.iter().position(|piece| piece.name == name && piece.kind() == body.kind()).map(|k| unused.remove(k)) };
        let Some(was) = was else {
            changes.push(Change::Create { name, layer, attributes, body });
            continue;
        };
        if let (true, Body::String(polyline)) = (is_line, &body) {
            let object = Object { id: was.id, revision: was.revision + 1, name: name.clone(), layer: was.layer, provenance: was.provenance, attributes: attributes.clone(), body: body.clone() };
            new_line = Some((Arc::new(object), polyline.clone()));
        }
        let rename = (name != was.name).then_some(name);
        changes.push(Change::Revise { id: was.id, base: was.revision, name: rename, layer: None, attributes: Some(attributes), body: Some(body) });
    }
    changes.extend(unused.into_iter().map(|piece| Change::Delete { id: piece.id, base: piece.revision }));
    (changes, new_line.expect("a step makes its line"))
}

/// A line no step or design made, as one drawn by hand: it's registered to what it lies on.
fn drawn_by_hand(object: &Object) -> bool {
    made_by_a_step(object).is_none() && !object.attributes.contains_key(MADE_BY)
}

/// The surfaces and grids a step goes to or lies on.
fn targets_of(step: &BuildOp) -> Vec<StepTarget> {
    match step {
        BuildOp::Project { to, .. } => vec![*to],
        BuildOp::Berm { on, .. } => on.iter().copied().collect(),
    }
}

/// What a step makes keeps from its line, as a run keeps its string's (decision 0029): not how the line was built, the
/// roads that came out on it, which the step carries on or ends, or the marks of a design.
fn kept_attributes(attributes: &Attributes) -> Attributes {
    let gone = |key: &str| key.starts_with(&format!("{ROAD_TOP} ")) || [BUILD, BUILD_STEP, BUILD_ID, BUILD_ON, BUILD_RECORD, MADE_BY, STEP_ROW, ROAD, ROAD_FACE].contains(&key);
    attributes.iter().filter(|(key, _)| !gone(key)).map(|(key, value)| (key.clone(), value.clone())).collect()
}

/// The roads that came out on a line, as their tops there say, with the width, grade and ways they were built with (decision
/// 0041), each to carry on from the line.
fn roads_on(attributes: &Attributes) -> Vec<Ramp> {
    let number = |name: &str, what: &str| match attributes.get(&road_top_key(name, what)) {
        Some(AttrValue::Number(n)) if n.is_finite() => Some(*n),
        _ => None,
    };
    let text = |name: &str, what: &str| match attributes.get(&road_top_key(name, what)) {
        Some(AttrValue::Text(text)) => Some(text.clone()),
        _ => None,
    };
    attributes
        .keys()
        .filter_map(|key| key.strip_prefix(&format!("{ROAD_TOP} "))?.strip_suffix(" pit"))
        .filter(|name| road_top_on(attributes, name).is_some())
        .filter_map(|name| {
            Some(Ramp {
                name: name.to_string(),
                string: None,
                fixed: RampFixed::Line,
                heading: if text(name, "heading").as_deref() == Some("back") { RampHeading::Back } else { RampHeading::Along },
                width: number(name, "width")?,
                grade: number(name, "grade")?,
                berm: if text(name, "berm").as_deref() == Some("through") { RampBerm::Through } else { RampBerm::After },
                turn_join: number(name, "turn_join").unwrap_or(USUAL_TURN_JOIN),
                turn_flat: number(name, "turn_flat"),
                turn_end: number(name, "turn_end"),
            })
        })
        .collect()
}

fn names_of(roads: &[Ramp]) -> String {
    match roads {
        [] => "none".into(),
        roads => roads.iter().map(|road| road.name.as_str()).collect::<Vec<_>>().join(", "),
    }
}

/// Each named road came out on the line.
fn on_the_line(named: &[String], roads: &[Ramp], line: &str) -> Result<(), CommandError> {
    match named.iter().find(|name| !roads.iter().any(|road| road.name == **name)) {
        Some(name) => Err(CommandError::Invalid(format!("no road {name} came out on {line}: the roads on it are {}", names_of(roads)))),
        None => Ok(()),
    }
}

/// New roads go on a wall by their strings, and have names of their own.
fn new_roads_fit(new: &[Ramp], carried: &[Ramp], line: &str) -> Result<(), CommandError> {
    for ramp in new {
        if carried.iter().any(|road| road.name == ramp.name) {
            return Err(CommandError::Invalid(format!("road {} already comes out on {line} and carries on by itself: give the new one another name", ramp.name)));
        }
        if ramp.fixed == RampFixed::Line || ramp.string.is_none() {
            return Err(CommandError::Invalid(format!("road {} needs a string across {line} to start from, or its end held", ramp.name)));
        }
    }
    Ok(())
}

fn berm_row(step: &BuildOp) -> ProfileStep {
    let BuildOp::Berm { width, on, condition, join, .. } = step else { unreachable!("a berm") };
    ProfileStep::Berm { min_bench: None, on: *on, width: StepValue::Number(*width), register: true, condition: *condition, join: *join }
}

fn wall_row(step: &BuildOp, roads: Vec<String>, turns: &[String]) -> ProfileStep {
    let BuildOp::Project { to, batter, condition, .. } = step else { unreachable!("a wall") };
    ProfileStep::Project { to: *to, batter: StepValue::Number(*batter), condition: *condition, ramps: roads, turns: turns.to_vec() }
}

/// The face steps for a step's rows. `to_come` is the wall after a berm that ends them.
fn face_steps<'t>(rows: &[ProfileStep], targets: &'t Targets<'_, '_>, on_walls: &'t [[Vec<usize>; 2]], to_come: Option<WallToCome<'t>>) -> Result<Vec<FaceStep<'t>>, CommandError> {
    let target = |to: &StepTarget| match to {
        StepTarget::Surface { id } => targets.mesh(*id).map(FaceTarget::Surface),
        StepTarget::Rl { rl } => Ok(FaceTarget::Rl(*rl)),
    };
    let number = |value: &StepValue| match value {
        StepValue::Number(value) => Ok(*value),
        StepValue::Rule(name) => Err(CommandError::Invalid(format!("a step takes numbers, not a rule such as {name}"))),
    };
    rows.iter()
        .zip(on_walls)
        .map(|(row, [ramps, turns])| {
            Ok(match row {
                ProfileStep::Project { to, batter, condition, .. } => FaceStep::Project { to: target(to)?, batter: EdgeRule::Fixed(number(batter)?), condition: *condition, ramps, turns },
                ProfileStep::Berm { on, width, register, condition, min_bench, join } => {
                    let join = match join {
                        None => JoinBoxes::Auto,
                        Some(true) => JoinBoxes::On,
                        Some(false) => JoinBoxes::Off,
                    };
                    FaceStep::Berm { on: on.as_ref().map(target).transpose()?, width: EdgeRule::Fixed(number(width)?), register: *register, condition: *condition, min_bench: *min_bench, join, to_come }
                }
            })
        })
        .collect()
}

/// A face a step ran, and its roads in the face's order.
struct Ran {
    face: Face,
    roads: Vec<Ramp>,
    notes: Vec<String>,
}

/// One step being built.
struct Build<'a, 'b> {
    inputs: &'a Inputs<'b>,
    name: String,
    layer: String,
    number: usize,
    id: ObjectId,
    side: WallSide,
    /// The line it's built from, and what that lies on.
    from: ObjectId,
    from_on: StepTarget,
    kept: Attributes,
    run_on: [f64; 2],
    condition: Option<f64>,
    /// The line it's built from as it is, as a fingerprint, and its revision.
    from_print: String,
    from_revision: u32,
    /// The revision the step's new line takes.
    made_revision: u32,
}

impl Build<'_, '_> {
    fn refuse(&self, e: GeomError) -> CommandError {
        CommandError::Invalid(format!("{} step {}: {}", self.name, self.number, e.to_string().replace("invalid input: ", "")))
    }

    fn target_name(&self, target: &StepTarget) -> String {
        match target {
            StepTarget::Rl { rl } => format!("RL {rl:.1}"),
            StepTarget::Surface { id } => self.inputs.object(*id).map_or_else(|_| id.to_string(), |object| object.name.clone()),
        }
    }

    /// A line as a step starts from it, run on past its ends if the step says: one drawn by hand registered to what it lies
    /// on, and one a step or design made as it is, as that's where its step left it, with only its run-on registered (decision
    /// 0041). How many of its vertices had no grid under them.
    fn toe_of(&self, targets: &Targets<'_, '_>, line: &Polyline, by_hand: bool, lies_on: &StepTarget, (ends, condition): ([f64; 2], Option<f64>)) -> Result<(Polyline, usize), CommandError> {
        let mut extended = if ends == [0.0, 0.0] { line.clone() } else { run_on(line, ends).map_err(|e| self.refuse(e))? };
        if let Some(tolerance) = condition {
            extended = conditioned_toe(&extended, tolerance).map_err(|e| self.refuse(e))?;
        }
        if !by_hand && ends == [0.0, 0.0] && condition.is_none() {
            return Ok((extended, 0));
        }
        let floor;
        let mesh = match lies_on {
            StepTarget::Surface { id } => targets.mesh(*id)?,
            StepTarget::Rl { rl } => {
                floor = level_floor(&extended, *rl);
                &floor
            }
        };
        let (toe, log) = register_toe(&extended, mesh).map_err(|e| self.refuse(e))?;
        Ok((if by_hand { toe } else { at_line_heights(&toe, line, condition.unwrap_or(0.0) + ON_THE_LINE) }, log.off_grid))
    }

    /// The face of a step: from `toe`, lying on `lies_on`, up `rows`, with the roads `carried` on from their tops in `tops`,
    /// `new` ones placed by their strings, the roads in `turns` turning back on a berm with no wall after it, before a wall
    /// likely at `batter`, and the line after the first row as you edited it, if you did.
    #[allow(clippy::too_many_arguments)]
    fn face(&self, targets: &Targets<'_, '_>, toe: &Polyline, lies_on: &StepTarget, rows: &[ProfileStep], carried: &[Ramp], tops: &Attributes, new: &[Ramp], (turns, batter): (&[String], f64), edited: Option<&Polyline>) -> Result<Ran, CommandError> {
        let floor;
        let register = match lies_on {
            StepTarget::Surface { id } => targets.mesh(*id)?,
            StepTarget::Rl { rl } => {
                floor = level_floor(toe, *rl);
                &floor
            }
        };
        let side = match self.side {
            WallSide::Left => OffsetSide::Left,
            WallSide::Right => OffsetSide::Right,
        };
        let profile = FaceProfile { name: self.name.clone(), direction: FaceDirection::Up, register: *lies_on, condition: None, steps: rows.to_vec(), ramps: new.to_vec() };
        let edits: Vec<FaceEdit<'_>> = edited.map(|line| FaceEdit { after: 0, line }).into_iter().collect();
        let mut placed = PlacedRamps::default();
        let mut notes = Vec::new();
        // The roads carried on come first, then new ones where their strings cross the line, on the face with no roads.
        for ramp in carried {
            let Some((top, back)) = road_top_on(tops, &ramp.name) else { continue };
            let off = plan_gap_to(toe, top.pit).max(plan_gap_to(toe, top.wall));
            if off > TOP_OFF_LINE {
                notes.push(format!("road {}: its top is {off:.1} m off the line, so it carries on from the nearest place on it", ramp.name));
            }
            placed.on_strip.push((ramp, FaceRamp { back, carries_on: Some(top), ..face_ramp(ramp, [top.wall[0], top.wall[1]]) }));
        }
        if !new.is_empty() {
            let strings: HashMap<ObjectId, &Polyline> = new.iter().filter_map(|ramp| Some((ramp.string?, self.inputs.string(ramp.string?).ok()?.1))).collect();
            let none = walls_ramps(&profile.steps, &[]);
            let plain = project_edited_face(toe, register, side, FaceDirection::Up, &face_steps(&profile.steps, targets, &none, None)?, &[], &edits).map_err(|e| self.refuse(e))?;
            let landed = place_ramps(&profile, &plain, &strings, &self.name);
            notes.extend(landed.missing.iter().map(|ramp| format!("road {ramp}: its string doesn't cross the line, so it made no road")));
            notes.extend(landed.notes.iter().map(|note| note.replacen("Ramp ", "road ", 1)));
            placed.on_strip.extend(landed.on_strip);
        }
        let ramps: Vec<&Ramp> = placed.on_strip.iter().map(|(ramp, _)| *ramp).collect();
        let specs: Vec<FaceRamp> = placed.on_strip.iter().map(|(_, spec)| *spec).collect();
        let on_walls = walls_ramps(&profile.steps, &ramps);
        let turning: Vec<usize> = turns.iter().filter_map(|name| ramps.iter().position(|ramp| ramp.name == *name)).collect();
        let to_come = (!turning.is_empty()).then_some(WallToCome { turns: &turning, batter });
        let steps = face_steps(&profile.steps, targets, &on_walls, to_come)?;
        let face = project_edited_face(toe, register, side, FaceDirection::Up, &steps, &specs, &edits).map_err(|e| self.refuse(e))?;
        Ok(Ran { face, roads: ramps.into_iter().cloned().collect(), notes })
    }

    /// What every piece a step makes carries.
    fn marked(&self) -> Attributes {
        let mut attributes = self.kept.clone();
        attributes.insert(BUILD.into(), AttrValue::Text(self.name.clone()));
        attributes.insert(BUILD_STEP.into(), AttrValue::Number(self.number as f64));
        attributes.insert(BUILD_ID.into(), AttrValue::Text(self.id.to_string()));
        attributes
    }

    /// The step's new line, saying what it lies on and how it was built.
    fn line(&self, name: String, line: Polyline, on: StepTarget, step: BuildOp, more: Attributes) -> Result<Change, CommandError> {
        let made_print = fingerprint(&line);
        let record = StepRecord { from: self.from, from_on: self.from_on, side: self.side, step, run_on: self.run_on, condition: self.condition, from_print: self.from_print.clone(), made_print, from_revision: self.from_revision, made_revision: self.made_revision };
        let mut attributes = self.marked();
        attributes.insert(BUILD_ON.into(), AttrValue::Text(json(&on)?));
        attributes.insert(BUILD_RECORD.into(), AttrValue::Text(json(&record)?));
        attributes.extend(more);
        Ok(Change::Create { name, layer: self.layer.clone(), attributes, body: Body::String(line) })
    }

    /// A berm from a crest: its line, with the roads that came out on the crest crossing it, turning on it or ending there.
    /// Its sheet waits for the wall above, which builds it round the roads' pads.
    fn berm(&self, targets: &Targets<'_, '_>, object: &Object, line: &Polyline, by_hand: bool, step: &BuildOp) -> Result<Outcome, CommandError> {
        let BuildOp::Berm { width, on, turns, stop, .. } = step else { unreachable!("a berm") };
        let roads = roads_on(&object.attributes);
        on_the_line(stop, &roads, &object.name)?;
        on_the_line(turns, &roads, &object.name)?;
        if let Some(name) = turns.iter().find(|name| stop.contains(name)) {
            return Err(CommandError::Invalid(format!("road {name} can't both end on {} and turn on the berm", object.name)));
        }
        let carried: Vec<Ramp> = roads.into_iter().filter(|road| !stop.contains(&road.name)).collect();
        let on = on.unwrap_or(self.from_on);
        let step = match step.clone() {
            BuildOp::Berm { width, condition, turns, join, stop, .. } => BuildOp::Berm { width, on: Some(on), condition, turns, join, stop },
            project => project,
        };
        let (toe, off_grid) = self.toe_of(targets, line, by_hand, &self.from_on, (self.run_on, self.condition))?;
        let ran = self.face(targets, &toe, &self.from_on, &[berm_row(&step)], &carried, &object.attributes, &[], (turns, batter_below(object)), None)?;
        let Some(berm_line) = ran.face.berm_line.clone() else { return Err(CommandError::Invalid(format!("{} step {}: the berm made no line", self.name, self.number))) };
        let mut messages = vec![format!("{} step {}: a berm {width} m wide on {}, from {}", self.name, self.number, self.target_name(&on), object.name)];
        if off_grid > 0 {
            messages.push(format!("{off_grid} of the line's vertices are off its grid and carried on at their grade"));
        }
        messages.extend(step_notes(&ran.face, &[berm_row(&step)]));
        messages.extend(ran.notes);
        let names: Vec<&str> = ran.roads.iter().map(|road| road.name.as_str()).collect();
        for road in &ran.roads {
            let stopped = ran.face.no_roads.iter().find(|stopped| ran.roads.get(stopped.ramp).is_some_and(|r| r.name == road.name));
            messages.push(match stopped {
                Some(stopped) => format!("road {}: stopped at {:.1}, {:.1}: {}", road.name, stopped.at[0], stopped.at[1], stop_reason(&names, stopped)),
                None if turns.contains(&road.name) => format!("road {}: turns back on the berm, for the wall above", road.name),
                None if road.berm == RampBerm::Through => format!("road {}: climbs on up through the berm", road.name),
                None => format!("road {}: crosses the berm to climb on from its line", road.name),
            });
        }
        messages.extend(stop.iter().map(|name| format!("road {name}: ends on {}", object.name)));
        messages.extend(pad_notes(&ran.face, &names));
        let change = self.line(format!("{} toe {}", self.name, self.number), berm_line, on, step, Attributes::new())?;
        let mut outcome = Outcome::new(vec![change]);
        outcome.messages = messages;
        Ok(outcome)
    }

    /// A wall from a line that isn't a berm's a step made: a toe, a crest, or any line drawn by hand.
    fn project(&self, targets: &Targets<'_, '_>, object: &Object, line: &Polyline, by_hand: bool, step: &BuildOp) -> Result<Outcome, CommandError> {
        let BuildOp::Project { ramps: new, stop, .. } = step else { unreachable!("a wall") };
        let roads = roads_on(&object.attributes);
        on_the_line(stop, &roads, &object.name)?;
        let carried: Vec<Ramp> = roads.into_iter().filter(|road| !stop.contains(&road.name)).collect();
        new_roads_fit(new, &carried, &object.name)?;
        let (toe, off_grid) = self.toe_of(targets, line, by_hand, &self.from_on, (self.run_on, self.condition))?;
        let climbing: Vec<String> = carried.iter().chain(new).map(|road| road.name.clone()).collect();
        let rows = [wall_row(step, climbing, &[])];
        let ran = self.face(targets, &toe, &self.from_on, &rows, &carried, &object.attributes, new, (&[], USUAL_BATTER), None)?;
        let mut head = Vec::new();
        if off_grid > 0 {
            head.push(format!("{off_grid} of the line's vertices are off its grid and carried on at their grade"));
        }
        head.extend(stop.iter().map(|name| format!("road {name}: ends on {}", object.name)));
        self.wall(ran, &rows, step, &object.name, None, head)
    }

    /// A wall from the line of a berm a step made, built with the berm from the crest it was built from. The berm as it
    /// would be built now, unless you've edited its line, in which case the wall stands on your line.
    fn over_berm(&self, targets: &Targets<'_, '_>, object: &Object, line: &Polyline, record: &StepRecord, (crest, crest_line): (&Object, &Polyline), step: &BuildOp) -> Result<Outcome, CommandError> {
        let BuildOp::Project { ramps: new, stop, .. } = step else { unreachable!("a wall") };
        let BuildOp::Berm { turns, stop: ended, .. } = &record.step else { unreachable!("a berm") };
        let carried: Vec<Ramp> = roads_on(&crest.attributes).into_iter().filter(|road| !ended.contains(&road.name)).collect();
        if let Some(name) = stop.iter().find(|name| carried.iter().any(|road| road.name == **name)) {
            return Err(CommandError::Invalid(format!("road {name} crossed the berm to {}: to end it, end it on {}, in the berm's step", object.name, crest.name)));
        }
        on_the_line(stop, &[], &object.name)?;
        new_roads_fit(new, &carried, &object.name)?;
        // The crest the berm was built from, as that step took it. Your line stands for the berm's if you've edited it, or the
        // crest, since the berm was built.
        let (toe, _) = self.toe_of(targets, crest_line, drawn_by_hand(crest), &record.from_on, (record.run_on, record.condition))?;
        let berm = berm_row(&record.step);
        let edited = fingerprint(crest_line) != record.from_print || fingerprint(line) != record.made_print;
        let climbing: Vec<String> = carried.iter().chain(new).map(|road| road.name.clone()).collect();
        let rows = [berm, wall_row(step, climbing, turns)];
        let ran = self.face(targets, &toe, &record.from_on, &rows, &carried, &crest.attributes, new, (&[], USUAL_BATTER), edited.then_some(line))?;
        let head = if edited { vec![format!("{} isn't where the berm from {} puts it now, as it's been edited or the crest has, so the wall stands on it as it is", object.name, crest.name)] } else { Vec::new() };
        self.wall(ran, &rows, step, &object.name, Some(&crest.name), head)
    }

    /// What a wall step makes: its crest, with the roads that came out on it; its sheet; and the roads' edges.
    fn wall(&self, ran: Ran, rows: &[ProfileStep], step: &BuildOp, from: &str, over: Option<&str>, head: Vec<String>) -> Result<Outcome, CommandError> {
        let BuildOp::Project { to, batter, .. } = step else { unreachable!("a wall") };
        let face = &ran.face;
        let Some(bench) = face.benches.last() else {
            return Err(CommandError::Invalid(format!("{} step {}: the wall has no height: {} is on or below {from} all the way along", self.name, self.number, self.target_name(to))));
        };
        let names: Vec<&str> = ran.roads.iter().map(|road| road.name.as_str()).collect();
        let reached = |ramp: usize| !face.no_roads.iter().any(|stopped| stopped.ramp == ramp);
        let mut tops = Attributes::new();
        for road in face.roads.iter().filter(|road| reached(road.ramp) && road.steps[1] == bench.step) {
            tops.extend(road_top_attributes(&ran.roads[road.ramp], road));
        }
        let crossings = find_crossings(&face.surface, COINCIDENT);
        let over = over.map_or_else(String::new, |crest| format!(", with the berm from {crest}"));
        let mut messages = vec![format!("{} step {}: a wall to {} at {batter}° from {from}{over}, {} triangles", self.name, self.number, self.target_name(to), face.surface.triangles.len())];
        messages.extend(head);
        messages.extend(step_notes(face, rows));
        messages.extend(ran.notes.iter().cloned());
        for edit in &face.edits {
            messages.extend(edit_note(from, &names, edit));
        }
        messages.extend(pad_notes(face, &names));
        messages.extend((0..ran.roads.len()).map(|r| road_note(&names, r, face)));
        if crossings.pairs > 0 {
            messages.push(format!("{} pairs of the wall's triangles cross", crossings.pairs));
        }
        let wall_name = format!("{} wall {}", self.name, self.number);
        let mut changes = vec![self.line(format!("{} crest {}", self.name, self.number), bench.crest.clone(), *to, step.clone(), tops)?];
        changes.push(Change::Create { name: wall_name.clone(), layer: self.layer.clone(), attributes: self.marked(), body: Body::Surface(Surface::new(face.surface.clone()).with_crossings(crossings)) });
        for road in &face.roads {
            let name = &ran.roads[road.ramp].name;
            for (which, edge) in [("pit", &road.pit_edge), ("wall", &road.wall_edge)] {
                let mut attributes = self.marked();
                attributes.insert(ROAD.into(), AttrValue::Text(name.clone()));
                attributes.insert(ROAD_FACE.into(), AttrValue::Text(wall_name.clone()));
                changes.push(Change::Create { name: format!("{} road {name} {which} edge {}", self.name, self.number), layer: self.layer.clone(), attributes, body: Body::String(edge.clone()) });
            }
        }
        let mut outcome = Outcome::new(changes);
        outcome.messages = messages;
        Ok(outcome)
    }
}

/// What a step keeps on its line, as JSON text.
fn json(value: &impl Serialize) -> Result<String, CommandError> {
    serde_json::to_string(value).map_err(|e| CommandError::Invalid(format!("the step's record: {e}")))
}

/// What a step's rows met on the way: where a wall has no height or no target, a berm left nothing under it or pinched.
fn step_notes(face: &Face, rows: &[ProfileStep]) -> Vec<String> {
    let mut notes = Vec::new();
    for report in &face.steps {
        let what = match rows.get(report.step) {
            Some(ProfileStep::Berm { .. }) => "the berm",
            _ => "the wall",
        };
        if report.no_wall {
            notes.push(format!("{what} has no height: its target is on or below its line all the way along"));
        }
        if let Some(first) = report.no_height.first() {
            notes.push(format!("{what} has no height along {} stretch(es), the first from {:.1}, {:.1}", report.no_height.len(), first.from[0], first.from[1]));
        }
        if let Some(first) = report.no_target.first() {
            notes.push(format!("{what} has no target along {} stretch(es), bridged, the first from {:.1}, {:.1}", report.no_target.len(), first.from[0], first.from[1]));
        }
        if report.off_surface > 0 {
            notes.push(format!("{} of the berm's vertices have nothing under them and hold the crest's height", report.off_surface));
        }
        if report.pieces_dropped > 0 {
            notes.push(format!("the berm pinched into pieces and {} were left out", report.pieces_dropped));
        }
    }
    notes
}

/// Switchbacks' boxes made one pad, by Join or by themselves, and Join with nothing to join (decision 0047).
fn pad_notes(face: &Face, names: &[&str]) -> Vec<String> {
    let name = |r: usize| names.get(r).copied().unwrap_or("?");
    let mut notes: Vec<String> = face.met.iter().map(|met: &MetBoxes| {
        let how = if met.gap < 0.0 { format!("would have overlapped by {:.1} m", -met.gap) } else { format!("would have been {:.1} m apart, under a road's width", met.gap) };
        format!("the boxes of {} and {} {how}, so they're one pad", name(met.ramps[0]), name(met.ramps[1]))
    }).collect();
    notes.extend(face.edits.iter().filter(|edit| !edit.joined.is_empty()).map(|edit| {
        let pairs: Vec<String> = edit.joined.iter().map(|[a, b]| format!("{} and {}", name(*a), name(*b))).collect();
        match edit.berm {
            Some([narrowest, widest]) => format!("the switchbacks of {} are one pad, the berm {narrowest:.1} to {widest:.1} m wide", pairs.join(", ")),
            None => format!("the switchbacks of {} are one pad", pairs.join(", ")),
        }
    }));
    notes.extend(face.join_notes.iter().map(|note: &JoinNote| match (note.turning.as_slice(), note.facing_away.first()) {
        ([], _) => "Join: no road turns back on a box on the berm, so there's nothing to join".to_string(),
        ([one], _) => format!("Join: only road {} turns on the berm, so there's nothing to join it to", name(*one)),
        (_, Some([a, b])) => format!("Join: the switchbacks of {} and {} face away from each other, so nothing lies between them to join", name(*a), name(*b)),
        (_, None) => "Join: the boxes couldn't be found on the line, so they're left as they are".to_string(),
    }));
    notes
}

/// The berm's line as you edited it, where the wall stands on it. A join is a pad's note.
fn edit_note(line: &str, names: &[&str], edit: &EditReport) -> Option<String> {
    if !edit.joined.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if let Some([narrowest, widest]) = edit.berm {
        parts.push(if (widest - narrowest).abs() < 0.05 { format!("the berm below it is {narrowest:.1} m wide") } else { format!("the berm below it is {narrowest:.1} to {widest:.1} m wide") });
    }
    for &(ramp, off) in &edit.roads_off {
        parts.push(format!("road {} started {off:.1} m from it, so it goes in from the nearest place on it", names.get(ramp).copied().unwrap_or("?")));
    }
    (!parts.is_empty()).then(|| format!("{line} as you edited it: {}", parts.join("; ")))
}

fn stop_reason(names: &[&str], stopped: &zd_geom::NoRoad) -> String {
    let name = |r: usize| names.get(r).copied().unwrap_or("?");
    match (stopped.crosses, stopped.too_close) {
        (Some(other), _) => format!("it would cross road {} running the other way on this wall, or come within a road's width of it: move one of them along the line", name(other)),
        (None, Some((other, apart, short))) => format!("its top is only {apart:.1} m along the crest from road {}'s, too close for both to turn back on one berm: move one of them {short:.1} m further along", name(other)),
        (None, None) => stopped.why.clone(),
    }
}

/// How far one road got up the wall.
fn road_note(names: &[&str], ramp: usize, face: &Face) -> String {
    let mut parts = Vec::new();
    for road in face.roads.iter().filter(|road| road.ramp == ramp) {
        let (Some(bottom), Some(top)) = (road.pit_edge.vertices.first(), road.pit_edge.vertices.last()) else { continue };
        parts.push(format!("{:.1} m from RL {:.1} to {:.1}, from {:.1}, {:.1}", road.pit_edge.plan_length(), bottom[2], top[2], bottom[0], bottom[1]));
        if let Some(miss) = road.off_end {
            parts.push(format!("it comes out {miss:.1} m from where it's held"));
        }
        for (at, steepest) in &road.steep_flats {
            parts.push(format!("its flat before the turn at {:.1}, {:.1} follows the floor at up to {:.1} %, steeper than the road", at[0], at[1], steepest * 100.0));
        }
    }
    for stopped in face.no_roads.iter().filter(|stopped| stopped.ramp == ramp) {
        parts.push(format!("stopped at {:.1}, {:.1}: {}", stopped.at[0], stopped.at[1], stop_reason(names, stopped)));
    }
    if parts.is_empty() {
        parts.push("made no road".into());
    }
    format!("road {}: {}", names.get(ramp).copied().unwrap_or("?"), parts.join("; "))
}
