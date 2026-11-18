use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, Object, ObjectId, ObjectKind, Polyline};
use zd_geom::{
    Corner, GeomError, Keep, OffsetSide, branching_ends, drape_string, extend_string, fillet_string, filter_string, grade_string, join_pieces, join_strings, offset_string, plan_crossings, redraw_stretch, smooth_string,
    split_string, split_string_at, trim_string, triangulate_grid,
};

use super::finite;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Severity};

fn string_input() -> Field {
    Field::object("string", ObjectKind::String, "The string to edit")
}

/// The other way to give a command its strings (decision 0078): any number, as one run.
fn strings_input() -> Field {
    Field::new("strings", FieldType::Objects, "Several strings to edit as one run, in place of `string`")
}

/// The strings a run works on (decision 0078): one given as `string`, which runs as it always has, or any number
/// given as `strings`, which is one run, one undo and one record for them all.
struct Picked {
    ids: Vec<ObjectId>,
    /// Given as `strings`, even if it's a list of one.
    listed: bool,
}

fn picked(string: Option<ObjectId>, strings: &[ObjectId]) -> Result<Picked, CommandError> {
    match (string, strings) {
        (Some(id), []) => Ok(Picked { ids: vec![id], listed: false }),
        (None, []) => Err(CommandError::Invalid("no string to work on: give one as `string`, or several as `strings`".into())),
        (Some(_), _) => Err(CommandError::Invalid("give `string` or `strings`, not both".into())),
        (None, ids) => {
            let mut seen = std::collections::HashSet::new();
            if let Some(twice) = ids.iter().find(|id| !seen.insert(**id)) {
                return Err(CommandError::Invalid(format!("string {twice} is in `strings` twice")));
            }
            Ok(Picked { ids: ids.to_vec(), listed: true })
        }
    }
}

/// A setting that names a vertex means one string: with several it's refused, not guessed at.
fn one_string_only(picked: &Picked, given: bool, what: &str) -> Result<(), CommandError> {
    if given && picked.ids.len() > 1 {
        return Err(CommandError::Invalid(format!("{what} is for one string; {} were given", picked.ids.len())));
    }
    Ok(())
}

/// Names for a sentence, the first ten and how many more.
fn names(of: &[&str]) -> String {
    let shown: Vec<String> = of.iter().take(10).map(|n| format!("'{n}'")).collect();
    if of.len() > 10 { format!("{} and {} more", shown.join(", "), of.len() - 10) } else { shown.join(", ") }
}

/// The run refused because some of its strings can't be done, each listed with why (decision 0078): nothing is half done.
fn cannot(failed: Vec<String>, of: usize) -> CommandError {
    CommandError::Invalid(format!("{} of {of} strings can't be done, so nothing was changed. {}", failed.len(), failed.join("; ")))
}

/// Edits every string picked with `edit`, as one run. Given as `string` it runs as it always has: the string revised
/// and `edit`'s note as it is. Given as `strings`: a string `edit` can't do refuses the run, each one listed with why;
/// one that comes back as it was is not revised, and is said; each string's own note is on it, under its name, after
/// a line saying what the run `did` to how many.
fn edit_each<'a>(inputs: &Inputs<'a>, picked: &Picked, did: &str, mut edit: impl FnMut(&'a Arc<Object>, &'a Polyline) -> Result<(Polyline, Option<Note>), CommandError>) -> Result<Outcome, CommandError> {
    if !picked.listed {
        let (object, line) = inputs.string(picked.ids[0])?;
        let (edited, note) = edit(object, line)?;
        let mut outcome = Outcome::new(vec![Change::revise_body(object, Body::String(edited))]);
        outcome.notes.extend(note);
        return Ok(outcome);
    }
    let (mut changes, mut notes, mut failed, mut left) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for &id in &picked.ids {
        let (object, line) = inputs.string(id)?;
        match edit(object, line) {
            Ok((edited, note)) => {
                notes.extend(note.map(|n| Note { text: format!("'{}': {}", object.name, n.text), ..n }.on(object.id)));
                if edited == *line {
                    left.push(object.name.as_str());
                } else {
                    changes.push(Change::revise_body(object, Body::String(edited)));
                }
            }
            Err(CommandError::Invalid(why) | CommandError::Geometry(why)) => failed.push(format!("'{}': {why}", object.name)),
            Err(other) => return Err(other),
        }
    }
    if !failed.is_empty() {
        return Err(cannot(failed, picked.ids.len()));
    }
    let mut said = format!("{did}: {} of {} strings", changes.len(), picked.ids.len());
    if !left.is_empty() {
        said += &format!("; {} came out as {} and {} left: {}", left.len(), if left.len() == 1 { "it was" } else { "they were" }, if left.len() == 1 { "is" } else { "are" }, names(&left));
    }
    let mut outcome = Outcome::new(changes).message(said);
    outcome.notes.extend(notes);
    Ok(outcome)
}

/// Open strings need two vertices, closed ones three; every coordinate finite.
fn check_line(line: &Polyline) -> Result<(), CommandError> {
    let needed = if line.closed { 3 } else { 2 };
    if line.vertices.len() < needed {
        let kind = if line.closed { "a closed" } else { "an open" };
        return Err(CommandError::Invalid(format!("{kind} string needs at least {needed} vertices")));
    }
    line.vertices.iter().try_for_each(finite)
}

fn vertex_index(line: &Polyline, index: usize) -> Result<(), CommandError> {
    if index < line.vertices.len() {
        Ok(())
    } else {
        Err(CommandError::Invalid(format!("vertex {index} is past the end of a {}-vertex string", line.vertices.len())))
    }
}

/// `string.create`: a new string from points (digitised, typed or imported).
pub struct CreateString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStringParams {
    pub name: String,
    pub layer: String,
    pub points: Vec<[f64; 3]>,
    #[serde(default)]
    pub closed: bool,
}

impl Command for CreateString {
    type Params = CreateStringParams;
    const NAME: &'static str = "string.create";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "New string from points",
            inputs: vec![],
            params: vec![
                Field::new("name", FieldType::Text, "Name of the new string"),
                Field::new("layer", FieldType::Layer, "Layer for the new string"),
                Field::new("points", FieldType::Points, "Vertices in order"),
                Field::new("closed", FieldType::Bool, "Join the last vertex back to the first"),
            ],
            outputs: vec![Field::object("string", ObjectKind::String, "The new string")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let line = Polyline { vertices: p.points.clone(), closed: p.closed };
        check_line(&line)?;
        Ok(Outcome::new(vec![Change::Create {
            name: p.name.clone(),
            layer: p.layer.clone(),
            attributes: Attributes::new(),
            body: Body::String(line),
        }]))
    }
}

/// `string.insert_vertex`: a vertex inserted before `index` (`index` = vertex count appends).
pub struct InsertVertex;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsertVertexParams {
    pub string: ObjectId,
    pub index: usize,
    pub point: [f64; 3],
}

impl Command for InsertVertex {
    type Params = InsertVertexParams;
    const NAME: &'static str = "string.insert_vertex";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Insert a vertex",
            inputs: vec![string_input()],
            params: vec![
                Field::new("index", FieldType::Integer, "Insert before this vertex; the vertex count appends"),
                Field::new("point", FieldType::Point, "The new vertex"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        if p.index > line.vertices.len() {
            return Err(CommandError::Invalid(format!(
                "cannot insert at {} in a {}-vertex string",
                p.index,
                line.vertices.len()
            )));
        }
        finite(&p.point)?;
        let mut edited = line.clone();
        edited.vertices.insert(p.index, p.point);
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(edited))]))
    }
}

/// `string.move_vertex`: one vertex moved to a new position (the end of a drag).
pub struct MoveVertex;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveVertexParams {
    pub string: ObjectId,
    pub index: usize,
    pub to: [f64; 3],
}

impl Command for MoveVertex {
    type Params = MoveVertexParams;
    const NAME: &'static str = "string.move_vertex";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Move a vertex",
            inputs: vec![string_input()],
            params: vec![
                Field::new("index", FieldType::Integer, "The vertex to move"),
                Field::new("to", FieldType::Point, "Its new position"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        vertex_index(line, p.index)?;
        finite(&p.to)?;
        let mut edited = line.clone();
        edited.vertices[p.index] = p.to;
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(edited))]))
    }
}

/// `string.delete_vertex`: one vertex removed; refused if it would leave too few.
pub struct DeleteVertex;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteVertexParams {
    pub string: ObjectId,
    pub index: usize,
}

impl Command for DeleteVertex {
    type Params = DeleteVertexParams;
    const NAME: &'static str = "string.delete_vertex";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Delete a vertex",
            inputs: vec![string_input()],
            params: vec![Field::new("index", FieldType::Integer, "The vertex to delete")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        vertex_index(line, p.index)?;
        let mut edited = line.clone();
        edited.vertices.remove(p.index);
        check_line(&edited)?;
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(edited))]))
    }
}

/// `string.redraw_stretch`: a stretch of an open string redrawn by hand, as a planner redraws part of a line (decision
/// 0046). The first and last points come onto the string where they're nearest it, within a metre, and the points
/// between take the place of every vertex between those two places. One revision, one undo.
pub struct RedrawStretch;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedrawStretchParams {
    pub string: ObjectId,
    /// Where the new stretch leaves the string, the new path, and where it comes back onto it.
    pub through: Vec<[f64; 3]>,
}

impl Command for RedrawStretch {
    type Params = RedrawStretchParams;
    const NAME: &'static str = "string.redraw_stretch";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Redraw a stretch of a string",
            inputs: vec![string_input()],
            params: vec![Field::new("through", FieldType::Points, "Where it leaves the string, the new path, and where it comes back onto the string")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        p.through.iter().try_for_each(finite)?;
        let redrawn = redraw_stretch(line, &p.through).map_err(|e| CommandError::Invalid(e.to_string().replace("invalid input: ", "")))?;
        check_line(&redrawn)?;
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(redrawn))]))
    }
}

/// The vertex range `from..=to` (either direction), defaulting `to` to the last vertex.
fn vertex_range(line: &Polyline, from: usize, to: Option<usize>) -> Result<usize, CommandError> {
    vertex_index(line, from)?;
    let to = to.unwrap_or(line.vertices.len() - 1);
    vertex_index(line, to)?;
    Ok(to)
}

/// `string.set_grade`: elevations along the string at a grade. Vertex `from` takes `rl` (its own
/// elevation by default) and each vertex from there to `to` rises `grade` % per 100 m of plan length;
/// the rest keep theirs. A new revision of the string. Several strings are each graded from their first vertex to their
/// last, from their own elevation there or from `rl`.
pub struct SetGrade;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetGradeParams {
    /// The string, or `strings` for several as one run (decision 0078).
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    /// The first vertex by default, and always with several strings.
    #[serde(default)]
    pub from: usize,
    /// Defaults to the last vertex; may be before `from` to grade backwards.
    #[serde(default)]
    pub to: Option<usize>,
    /// Grade in %, positive uphill in the direction from `from` to `to`.
    pub grade: f64,
    /// Elevation at `from`; its current elevation by default.
    #[serde(default)]
    pub rl: Option<f64>,
}

impl Command for SetGrade {
    type Params = SetGradeParams;
    const NAME: &'static str = "string.set_grade";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Set a grade along a string",
            inputs: vec![string_input(), strings_input()],
            params: vec![
                Field::new("from", FieldType::Integer, "The vertex the grade starts at; the first by default, and with several strings"),
                Field::new("to", FieldType::Integer, "The vertex it ends at; the last by default"),
                Field::new("grade", FieldType::Number { unit: "%" }, "Positive uphill from the start, along plan length"),
                Field::new("rl", FieldType::Number { unit: "m" }, "Elevation at the start; its own by default"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        one_string_only(&picked, p.from != 0 || p.to.is_some(), "a grade between two vertices (`from`, `to`)")?;
        edit_each(inputs, &picked, &format!("graded at {} %", p.grade), |_, line| {
            let to = vertex_range(line, p.from, p.to)?;
            let rl = p.rl.unwrap_or(line.vertices[p.from][2]);
            let graded = grade_string(line, p.from, to, rl, p.grade).map_err(|e| CommandError::Invalid(e.to_string()))?;
            let message = format!(
                "vertices {} to {} at {} % from RL {rl:.3}: RL {:.3} at vertex {}",
                p.from + 1,
                to + 1,
                p.grade,
                graded.vertices[to][2],
                to + 1
            );
            Ok((graded, Some(Note::info(message))))
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideParam {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CornerParam {
    #[default]
    Mitre,
    Round,
}

fn default_mitre_limit() -> f64 {
    4.0
}

fn once() -> u32 {
    1
}

/// `string.offset`: a new string `distance` metres in plan to the left or right of this one (looking
/// along it from above), with mitred or round outside corners. Each new vertex takes the elevation of
/// the nearest point on the source, or `rl`. The source is not changed; an offset that pinches off
/// makes one string per piece. With `times`, a string at each multiple of the distance. Several strings are each
/// offset the same, to their own left or right: one running the other way to the first goes to the other side, and
/// is warned of.
pub struct OffsetString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OffsetStringParams {
    /// The string, or `strings` for several as one run (decision 0078).
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    pub distance: f64,
    pub side: SideParam,
    #[serde(default)]
    pub corners: CornerParam,
    /// Mitres reaching farther than this many times the distance are bevelled.
    #[serde(default = "default_mitre_limit")]
    pub mitre_limit: f64,
    /// A fixed elevation for the new string; by default each vertex takes the source's.
    #[serde(default)]
    pub rl: Option<f64>,
    /// Defaults to the source string's layer.
    #[serde(default)]
    pub layer: Option<String>,
    /// Defaults to the source's name with the offset. For one string only.
    #[serde(default)]
    pub name: Option<String>,
    /// How many strings, each `distance` further out, all measured from the source.
    #[serde(default = "once")]
    pub times: u32,
}

impl Command for OffsetString {
    type Params = OffsetStringParams;
    const NAME: &'static str = "string.offset";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Offset a string in plan",
            inputs: vec![string_input(), strings_input()],
            params: vec![
                Field::new("distance", FieldType::Number { unit: "m" }, "Horizontal distance"),
                Field::new("side", FieldType::Text, "left or right, looking along the string"),
                Field::new("corners", FieldType::Text, "mitre (default) or round"),
                Field::new("mitre_limit", FieldType::Number { unit: "x distance" }, "Mitres reaching farther are bevelled; 4 by default"),
                Field::new("rl", FieldType::Number { unit: "m" }, "Fixed elevation; the source's by default"),
                Field::new("layer", FieldType::Layer, "Layer for the new string; the source's by default"),
                Field::new("name", FieldType::Text, "Optional, for one string"),
                Field::new("times", FieldType::Integer, "How many strings, each the distance further out; 1 by default"),
            ],
            outputs: vec![Field::object("strings", ObjectKind::String, "The offset strings, one per piece")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        one_string_only(&picked, p.name.is_some(), "a name of your own (`name`)")?;
        if let Some(rl) = p.rl {
            finite(&[0.0, 0.0, rl])?;
        }
        let side = match p.side {
            SideParam::Left => OffsetSide::Left,
            SideParam::Right => OffsetSide::Right,
        };
        let corner = match p.corners {
            CornerParam::Mitre => Corner::Mitre { limit: p.mitre_limit },
            CornerParam::Round => Corner::Round,
        };
        if p.times == 0 {
            return Err(CommandError::Invalid("An offset needs to be made at least once".into()));
        }
        let side_word = match p.side {
            SideParam::Left => "left",
            SideParam::Right => "right",
        };
        let mut changes = Vec::new();
        let mut vertices = 0;
        let mut failed = Vec::new();
        let mut ways: Vec<(&Arc<Object>, Way)> = Vec::new();
        for &id in &picked.ids {
            let (object, line) = inputs.string(id)?;
            ways.push((object, way_of(line)));
            let layer = p.layer.clone().unwrap_or_else(|| inputs.layer_name(object));
            for time in 1..=p.times {
                let distance = p.distance * f64::from(time);
                let pieces = match offset_string(line, distance, side, corner, p.rl) {
                    Ok(pieces) => pieces,
                    Err(e) => {
                        let why = if p.times > 1 { format!("at {distance} m: {e}") } else { e.to_string() };
                        if !picked.listed {
                            return Err(CommandError::Invalid(why));
                        }
                        failed.push(format!("'{}': {why}", object.name));
                        break;
                    }
                };
                let base = match &p.name {
                    Some(name) if p.times > 1 => format!("{name} {time}"),
                    Some(name) => name.clone(),
                    None => format!("{} offset {distance} m {side_word}", object.name),
                };
                let count = pieces.len();
                vertices += pieces.iter().map(|l| l.vertices.len()).sum::<usize>();
                changes.extend(pieces.into_iter().enumerate().map(|(i, piece)| Change::Create {
                    name: if count == 1 { base.clone() } else { format!("{base} ({})", i + 1) },
                    layer: layer.clone(),
                    attributes: Attributes::new(),
                    body: Body::String(piece),
                }));
            }
        }
        if !failed.is_empty() {
            return Err(cannot(failed, picked.ids.len()));
        }
        let message = format!(
            "offset {} m to the {side_word}{} with {} corners: {} string(s){}, {vertices} vertices",
            p.distance,
            if p.times > 1 { format!(", {} times", p.times) } else { String::new() },
            if matches!(p.corners, CornerParam::Round) { "round" } else { "mitred" },
            changes.len(),
            if picked.listed { format!(" from {}", picked.ids.len()) } else { String::new() },
        );
        let mut outcome = Outcome::new(changes).message(message);
        // Left and right are each string's own, looking along it: one running against the first went the other way.
        for (object, way) in ways.iter().skip(1) {
            if way.against(&ways[0].1) {
                let text = format!("'{}' runs the other way to '{}', so its {side_word} is the other side: reverse it first if that's not meant", object.name, ways[0].0.name);
                outcome.notes.push(Note::warning(Code::StringRunsOtherWay, text).on(object.id));
            }
        }
        Ok(outcome)
    }
}

/// Which way a string runs, for telling whether two offset to the same side: an open one from its first vertex to its
/// last in plan, a closed one clockwise or not.
enum Way {
    Along([f64; 2]),
    Turning(f64),
    /// An open string whose ends are over each other in plan, or a closed one with no area.
    Neither,
}

fn way_of(line: &Polyline) -> Way {
    let v = &line.vertices;
    if line.closed {
        // About the first vertex, so mine grid eastings and northings don't swamp the sum.
        let o = v[0];
        let twice_area: f64 = (0..v.len()).map(|i| (v[i], v[(i + 1) % v.len()])).map(|(a, b)| (a[0] - o[0]) * (b[1] - o[1]) - (b[0] - o[0]) * (a[1] - o[1])).sum();
        return if twice_area == 0.0 { Way::Neither } else { Way::Turning(twice_area) };
    }
    match (v.first(), v.last()) {
        (Some(a), Some(b)) if a[0] != b[0] || a[1] != b[1] => Way::Along([b[0] - a[0], b[1] - a[1]]),
        _ => Way::Neither,
    }
}

impl Way {
    /// True where both run a way that can be told apart and they differ: open strings more than a right angle apart,
    /// closed ones turning opposite ways. An open and a closed string aren't compared.
    fn against(&self, first: &Way) -> bool {
        match (self, first) {
            (Way::Along(a), Way::Along(b)) => a[0] * b[0] + a[1] * b[1] < 0.0,
            (Way::Turning(a), Way::Turning(b)) => a * b < 0.0,
            _ => false,
        }
    }
}

fn default_step() -> f64 {
    1.0
}

fn default_attribute() -> String {
    "strip".into()
}

fn default_number_name() -> String {
    "Strip #".into()
}

/// `string.number_across`: strings numbered in the order a line drawn across them crosses them, as a pit's
/// strips are numbered by direction. Each takes its number as an attribute, and a name with it in. A string
/// the line doesn't cross stays as it is, and one it crosses more than once goes by the first.
pub struct NumberStrings;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumberStringsParams {
    pub strings: Vec<ObjectId>,
    /// The two ends of the line, in plan. Numbers run from `from` to `to`.
    pub from: [f64; 3],
    pub to: [f64; 3],
    pub start: f64,
    /// Negative to count down.
    #[serde(default = "default_step")]
    pub step: f64,
    /// The attribute that holds the number.
    #[serde(default = "default_attribute")]
    pub attribute: String,
    /// Each string's name, with its number in place of `#`, or on the end if there's no `#`.
    #[serde(default = "default_number_name")]
    pub name: String,
}

impl NumberStringsParams {
    fn name_of(&self, number: f64) -> String {
        if self.name.contains('#') { self.name.replace('#', &number.to_string()) } else { format!("{} {number}", self.name) }
    }
}

impl Command for NumberStrings {
    type Params = NumberStringsParams;
    const NAME: &'static str = "string.number_across";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Number strings in the order a line crosses them",
            inputs: vec![Field::new("strings", FieldType::Objects, "The strings to number")],
            params: vec![
                Field::new("from", FieldType::Point, "Where the line starts, and the numbers with it; plan only"),
                Field::new("to", FieldType::Point, "Where the line ends; plan only"),
                Field::new("start", FieldType::Number { unit: "" }, "The first number"),
                Field::new("step", FieldType::Number { unit: "" }, "Added each time, negative to count down; 1 by default"),
                Field::new("attribute", FieldType::Text, "The attribute that holds the number; strip by default"),
                Field::new("name", FieldType::Text, "Each string's name, with its number in place of #; Strip # by default"),
            ],
            outputs: vec![Field::object("strings", ObjectKind::String, "The strings numbered")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        finite(&p.from)?;
        finite(&p.to)?;
        finite(&[p.start, p.step, 0.0])?;
        if p.step == 0.0 {
            return Err(CommandError::Invalid("A step of zero would give every string the same number".into()));
        }
        if p.attribute.trim().is_empty() {
            return Err(CommandError::Invalid("The number needs an attribute to go in".into()));
        }
        let across = Polyline { vertices: vec![p.from, p.to], closed: false };
        if across.plan_length() == 0.0 {
            return Err(CommandError::Invalid("The line's two ends are the same point in plan".into()));
        }
        let mut crossed = Vec::new();
        let mut missed = Vec::new();
        let mut messages = Vec::new();
        for &id in &p.strings {
            let (object, line) = inputs.string(id)?;
            let crossings = plan_crossings(&across, line);
            match crossings.first() {
                Some(first) => crossed.push((first.along, object)),
                None => missed.push(object.name.as_str()),
            }
            if crossings.len() > 1 {
                let text = format!("{} crosses the line {} times: numbered where it first does", object.name, crossings.len());
                messages.push(Note::new(Severity::Info, Some(Code::StringCrossesAgain), text).on(object.id));
            }
        }
        if crossed.is_empty() {
            return Err(CommandError::Invalid("The line doesn't cross any of these strings".into()));
        }
        crossed.sort_by(|a, b| a.0.total_cmp(&b.0));
        let number = |k: usize| p.start + k as f64 * p.step;
        let changes = crossed.iter().enumerate().map(|(k, (_, object))| {
            let mut attributes = object.attributes.clone();
            attributes.insert(p.attribute.clone(), AttrValue::Number(number(k)));
            Change::Revise { id: object.id, base: object.revision, name: Some(p.name_of(number(k))), layer: None, attributes: Some(attributes), body: None }
        });
        let mut outcome = Outcome::new(changes.collect()).message(format!("Numbered {} strings from {} to {}", crossed.len(), number(0), number(crossed.len() - 1)));
        if !missed.is_empty() {
            let text = format!("The line didn't cross {}: left as they were", missed.join(", "));
            outcome.notes.push(Note::warning(Code::StringNotCrossed, text).measured(missed.len() as f64, "strings"));
        }
        outcome.notes.extend(messages);
        Ok(outcome)
    }
}

/// `string.drape`: the string laid on a surface or elevation grid. Each vertex takes the surface's
/// elevation under it; with `crossings` (the default) a vertex is added wherever the string crosses a
/// triangle edge, so it follows the surface between vertices. Vertices with no surface under them keep
/// their elevations and are counted. A new revision of the string. Several strings are each draped on the one surface,
/// which is triangulated once.
pub struct DrapeString;

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrapeStringParams {
    /// The string, or `strings` for several as one run (decision 0078).
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    pub surface: ObjectId,
    #[serde(default = "yes")]
    pub crossings: bool,
}

impl Command for DrapeString {
    type Params = DrapeStringParams;
    const NAME: &'static str = "string.drape";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Drape a string onto a surface",
            inputs: vec![string_input(), strings_input(), Field::object("surface", ObjectKind::Surface, "A surface or elevation grid")],
            params: vec![Field::new("crossings", FieldType::Bool, "Add vertices where the string crosses triangle edges")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        let target = inputs.object(p.surface)?;
        let triangulated;
        let mesh = match &target.body {
            Body::Surface(s) => s.mesh(),
            Body::Grid(g) => {
                triangulated = triangulate_grid(g);
                &triangulated
            }
            other => return Err(CommandError::WrongKind { id: p.surface, expected: ObjectKind::Surface, found: other.kind() }),
        };
        edit_each(inputs, &picked, &format!("draped on {}", target.name), |object, line| {
            let (draped, log) = drape_string(line, mesh, p.crossings).map_err(|e| CommandError::Invalid(e.to_string()))?;
            let mut message = format!("draped on {}: {} crossings added", target.name, log.crossings_added);
            let mut note = Note::info("");
            if log.off_surface > 0 {
                message += &format!("; {} vertices have no surface under them and keep their elevations", log.off_surface);
                note = Note::warning(Code::StringOffSurface, "").measured(log.off_surface as f64, "vertices");
            }
            Ok((draped, Some(Note { text: message, ..note }.on(object.id))))
        })
    }
}

/// `string.set_elevation`: vertices `from` to `to` (all by default) set to one elevation. A new
/// revision of the string. Several strings each go to it whole.
pub struct SetElevation;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetElevationParams {
    /// The string, or `strings` for several as one run (decision 0078).
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    pub rl: f64,
    #[serde(default)]
    pub from: Option<usize>,
    #[serde(default)]
    pub to: Option<usize>,
}

impl Command for SetElevation {
    type Params = SetElevationParams;
    const NAME: &'static str = "string.set_elevation";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Set a string's vertices to one elevation",
            inputs: vec![string_input(), strings_input()],
            params: vec![
                Field::new("rl", FieldType::Number { unit: "m" }, "The elevation"),
                Field::new("from", FieldType::Integer, "First vertex; the first by default"),
                Field::new("to", FieldType::Integer, "Last vertex; the last by default"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        one_string_only(&picked, p.from.is_some() || p.to.is_some(), "an elevation between two vertices (`from`, `to`)")?;
        finite(&[0.0, 0.0, p.rl])?;
        edit_each(inputs, &picked, &format!("set to RL {:.3}", p.rl), |_, line| {
            let from = p.from.unwrap_or(0);
            let to = vertex_range(line, from, p.to)?;
            let mut edited = line.clone();
            for v in &mut edited.vertices[from.min(to)..=from.max(to)] {
                v[2] = p.rl;
            }
            let message = format!("vertices {} to {} set to RL {:.3}", from.min(to) + 1, from.max(to) + 1, p.rl);
            Ok((edited, Some(Note::info(message))))
        })
    }
}

/// `string.reverse`: the string's direction reversed (which side is left, which way a grade runs).
pub struct ReverseString;

/// `string.set_closed`: the string closed (its last vertex joined back to its first) or opened.
pub struct SetClosed;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StringParams {
    /// The string, or `strings` for several as one run (decision 0078).
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetClosedParams {
    /// The string, or `strings` for several as one run (decision 0078).
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    pub closed: bool,
}

impl Command for ReverseString {
    type Params = StringParams;
    const NAME: &'static str = "string.reverse";

    fn spec(&self) -> CommandSpec {
        CommandSpec { name: Self::NAME, summary: "Reverse a string's direction", inputs: vec![string_input(), strings_input()], params: vec![], outputs: vec![] }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        edit_each(inputs, &picked, "reversed", |_, line| {
            let mut edited = line.clone();
            edited.vertices.reverse();
            Ok((edited, None))
        })
    }
}

impl Command for SetClosed {
    type Params = SetClosedParams;
    const NAME: &'static str = "string.set_closed";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Close or open a string",
            inputs: vec![string_input(), strings_input()],
            params: vec![Field::new("closed", FieldType::Bool, "Join the last vertex back to the first")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        edit_each(inputs, &picked, if p.closed { "closed" } else { "opened" }, |object, line| {
            // One string already so is a mistake to say; among several it's left as it is, and said.
            if line.closed == p.closed && !picked.listed {
                return Err(CommandError::Invalid(format!("'{}' is already {}", object.name, if p.closed { "closed" } else { "open" })));
            }
            let edited = Polyline { vertices: line.vertices.clone(), closed: p.closed };
            check_line(&edited)?;
            Ok((edited, None))
        })
    }
}

/// `string.split`: an open string split at an inner vertex, or at the place on it nearest a point clicked (String
/// tools plan, workstream 1), where a vertex goes in at the string's height unless the place is within a millimetre
/// of one: it keeps the first part, and the second (from that vertex on) becomes a new string on the same layer. A
/// closed string opens there instead.
pub struct SplitString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitStringParams {
    pub string: ObjectId,
    /// The vertex to split at, or `point` instead.
    #[serde(default)]
    pub at: Option<usize>,
    /// A point on the string to split at, in mine grid coordinates: within a metre of it in plan.
    #[serde(default)]
    pub point: Option<[f64; 3]>,
}

impl Command for SplitString {
    type Params = SplitStringParams;
    const NAME: &'static str = "string.split";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Split a string at a vertex or a point on it",
            inputs: vec![string_input()],
            params: vec![
                Field::new("at", FieldType::Integer, "The vertex to split at, or `point`"),
                Field::new("point", FieldType::Point, "Or a point on the string to split at, within a metre of it in plan"),
            ],
            outputs: vec![Field::object("second", ObjectKind::String, "The second part (none when a closed string opens)")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (object, line) = inputs.string(p.string)?;
        let (first, second, place) = match (p.at, p.point) {
            (Some(at), None) => {
                let (first, second) = split_string(line, at).map_err(|e| CommandError::Invalid(e.to_string()))?;
                (first, second, format!("vertex {}", at + 1))
            }
            (None, Some(point)) => {
                finite(&point)?;
                let (first, second, at) = split_string_at(line, [point[0], point[1]]).map_err(|e| CommandError::Invalid(why(e)))?;
                // Split where no vertex was, one went in.
                let split_at = if second.is_some() { *first.vertices.last().expect("vertices") } else { first.vertices[0] };
                let place = if line.vertices.contains(&split_at) { format!("vertex {}", at + 1) } else { "a new vertex".to_string() };
                (first, second, place)
            }
            (None, None) => return Err(CommandError::Invalid("no place to split at: give a vertex as `at`, or a point on the string as `point`".into())),
            (Some(_), Some(_)) => return Err(CommandError::Invalid("give `at` or `point`, not both".into())),
        };
        let mut changes = vec![Change::revise_body(object, Body::String(first.clone()))];
        let message = match second {
            None => format!("opened at {place}"),
            Some(second) => {
                let message = format!("split at {place}: {} and {} vertices", first.vertices.len(), second.vertices.len());
                changes.push(Change::Create {
                    name: format!("{} part 2", object.name),
                    layer: inputs.layer_name(object),
                    attributes: object.attributes.clone(),
                    body: Body::String(second),
                });
                message
            }
        };
        Ok(Outcome::new(changes).message(message))
    }
}

/// What a geometry refusal says, without its kind in front: the words are for the planner.
fn why(e: GeomError) -> String {
    match e {
        GeomError::InvalidInput(words) | GeomError::Refused(words) => words,
        other => other.to_string(),
    }
}

/// What a trim keeps: the side of the cutting string a point is on, or both sides, split.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrimKeep {
    #[default]
    Side,
    Both,
}

/// `string.trim` (String tools plan, workstream 1): every string picked cut where another string, open or closed,
/// crosses it in plan, keeping the parts on the side of it a point is on (inside or outside a closed one), or both
/// sides, split. A cut is at the string's own height there. The first part kept is the string's next revision; any
/// others are new strings on its layer, named for it, carrying its attributes. A string the cutting string doesn't
/// cross is left as it was and said; one that lies wholly on the side dropped refuses the run (decision 0078).
pub struct TrimString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrimStringParams {
    /// The string, or `strings` for several as one run.
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    /// The string that cuts them.
    pub by: ObjectId,
    #[serde(default)]
    pub keep: TrimKeep,
    /// With `keep` side: a point on the side to keep, in mine grid coordinates.
    #[serde(default)]
    pub side: Option<[f64; 3]>,
}

impl Command for TrimString {
    type Params = TrimStringParams;
    const NAME: &'static str = "string.trim";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Trim strings back to a cutting string, keeping one side or splitting at it",
            inputs: vec![string_input(), strings_input(), Field::object("by", ObjectKind::String, "The string that cuts them")],
            params: vec![
                Field::new("keep", FieldType::Text, "`side` (the side `side` is on) or `both` (split where it crosses)"),
                Field::new("side", FieldType::Point, "A point on the side to keep"),
            ],
            outputs: vec![Field::new("parts", FieldType::Objects, "Parts kept as new strings, past each string's first")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        let (cutter_object, cutter) = inputs.string(p.by)?;
        if picked.ids.contains(&p.by) {
            return Err(CommandError::Invalid(format!("'{}' is the cutting string, so it can't be trimmed by itself; leave it out of those picked", cutter_object.name)));
        }
        let keep = match (p.keep, p.side) {
            (TrimKeep::Side, Some(side)) => {
                finite(&side)?;
                Keep::Side([side[0], side[1]])
            }
            (TrimKeep::Side, None) => return Err(CommandError::Invalid("which side to keep isn't given: a point on it as `side`, or `keep` both to split".into())),
            (TrimKeep::Both, None) => Keep::Both,
            (TrimKeep::Both, Some(_)) => return Err(CommandError::Invalid("`side` is for keeping one side; with `keep` both every part is kept".into())),
        };
        let (mut changes, mut made, mut failed, mut left) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut cuts, mut dropped, mut trimmed) = (0, 0, 0);
        for &id in &picked.ids {
            let (object, line) = inputs.string(id)?;
            match trim_string(line, cutter, keep) {
                Ok(t) if t.cuts == 0 => left.push(object.name.as_str()),
                Ok(t) => {
                    trimmed += 1;
                    cuts += t.cuts;
                    dropped += t.dropped;
                    let mut pieces = t.pieces.into_iter();
                    changes.push(Change::revise_body(object, Body::String(pieces.next().expect("a part kept"))));
                    for (k, piece) in pieces.enumerate() {
                        made.push(Change::Create { name: format!("{} part {}", object.name, k + 2), layer: inputs.layer_name(object), attributes: object.attributes.clone(), body: Body::String(piece) });
                    }
                }
                Err(e) => failed.push(format!("'{}': {}", object.name, why(e))),
            }
        }
        if !failed.is_empty() {
            return Err(if picked.listed { cannot(failed, picked.ids.len()) } else { CommandError::Invalid(failed.remove(0)) });
        }
        if !picked.listed && !left.is_empty() {
            return Err(CommandError::Invalid(format!("'{}' isn't crossed by '{}', so there's nothing to trim", left[0], cutter_object.name)));
        }
        let new = made.len();
        let mut said = match keep {
            Keep::Side(_) => format!("trimmed {trimmed} of {} strings by '{}': cut in {}, {} dropped", picked.ids.len(), cutter_object.name, so_many(cuts, "place"), so_many(dropped, "part")),
            Keep::Both => format!("split {trimmed} of {} strings where '{}' crosses them: cut in {}", picked.ids.len(), cutter_object.name, so_many(cuts, "place")),
        };
        if new > 0 {
            said += &format!(", {} kept as new strings", so_many(new, "part"));
        }
        if !left.is_empty() {
            said += &format!("; {} not crossed and left as {}: {}", left.len(), if left.len() == 1 { "it was" } else { "they were" }, names(&left));
        }
        changes.extend(made);
        Ok(Outcome::new(changes).message(said))
    }
}

fn default_reach() -> f64 {
    100.0
}

/// `string.extend` (String tools plan, workstream 1): every string picked run on from its nearer end to another
/// string, along its last segment and at that segment's grade, no further than `reach`: the end vertex moves there,
/// so the line is the same line, longer. A string already ending on it is left as it was and said; one with neither
/// end meeting it within the reach refuses the run, each listed (decision 0078).
pub struct ExtendString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtendStringParams {
    /// The string, or `strings` for several as one run.
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    /// The string to run them on to.
    pub to: ObjectId,
    #[serde(default = "default_reach")]
    pub reach: f64,
}

impl Command for ExtendString {
    type Params = ExtendStringParams;
    const NAME: &'static str = "string.extend";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Run strings on from their nearer end to another string",
            inputs: vec![string_input(), strings_input(), Field::object("to", ObjectKind::String, "The string to run them on to")],
            params: vec![Field::new("reach", FieldType::Number { unit: "m" }, "Furthest an end runs on, in plan; 100 m by default")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        let (target, to) = inputs.string(p.to)?;
        if picked.ids.contains(&p.to) {
            return Err(CommandError::Invalid(format!("'{}' is the string to run on to, so it can't be run on to itself; leave it out of those picked", target.name)));
        }
        if !p.reach.is_finite() || p.reach <= 0.0 {
            return Err(CommandError::Invalid(format!("the reach must be more than nothing; it was {} m", p.reach)));
        }
        edit_each(inputs, &picked, &format!("ran on to '{}'", target.name), |object, line| match extend_string(line, to, p.reach) {
            Ok(Some(x)) => {
                let text = format!("ran on {} at its {} to '{}'", metres(x.length), if x.at_start { "start" } else { "end" }, target.name);
                Ok((x.line, Some(Note::info(text).measured(x.length, "m"))))
            }
            Ok(None) if picked.listed => Ok((line.clone(), None)),
            Ok(None) => Err(CommandError::Invalid(format!("'{}' already ends on '{}'", object.name, target.name))),
            Err(e) => Err(CommandError::Invalid(why(e))),
        })
    }
}

/// `string.join`: another open string joined onto this one at their nearest ends, in this one's
/// direction. This string keeps the result and the other is deleted (undo brings it back). Ends
/// that do not meet are bridged by a segment, and the gap is reported.
///
/// Or several open strings, given as `strings`, joined end to end into one (decision 0078), as a toe that came in as
/// pieces is put back together: the two nearest ends in plan first, then the next nearest left. The first string
/// keeps the result, its name, its attributes and the way it runs, and the others are deleted, in one run and one undo.
/// Ends within `tolerance` share a vertex, and pieces that make a ring close (String tools plan, workstream 1). An
/// attribute the first string hasn't got is taken from the others where they agree on it; where they don't, or where
/// one differs from the first's, it's said.
pub struct JoinStrings;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinStringsParams {
    /// The string another is joined onto, with `other`.
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub other: Option<ObjectId>,
    /// Or the pieces to join end to end into one, the first keeping the result (decision 0078).
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    /// With `strings`: ends this close in 3D share a vertex rather than being bridged (m).
    #[serde(default = "default_join_tolerance")]
    pub tolerance: f64,
}

fn default_join_tolerance() -> f64 {
    0.01
}

/// A value for a sentence.
fn value_words(value: &AttrValue) -> String {
    match value {
        AttrValue::Bool(b) => b.to_string(),
        AttrValue::Number(x) => x.to_string(),
        AttrValue::Text(t) => format!("'{t}'"),
    }
}

/// The joined string's attributes: its own, with any it hasn't got taken from the other pieces where they agree.
/// Where they don't agree, or one differs from the kept string's, it's said, and the kept string's value (or none)
/// stands. `None` when nothing is added.
fn joined_attributes(kept: &Object, others: &[&Object]) -> (Option<Attributes>, Vec<Note>) {
    let mut keys: Vec<&'static str> = Vec::new();
    for other in others {
        for key in other.attributes.keys() {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    let (mut attributes, mut added, mut notes) = (kept.attributes.clone(), false, Vec::new());
    for key in keys {
        let held: Vec<(&str, &AttrValue)> = others.iter().filter_map(|o| o.attributes.get(key).map(|v| (o.name.as_str(), v))).collect();
        match kept.attributes.get(key) {
            Some(own) => {
                let differ: Vec<String> = held.iter().filter(|(_, v)| *v != own).map(|(name, v)| format!("'{name}' had {}", value_words(v))).collect();
                if !differ.is_empty() {
                    let text = format!("'{key}' kept as '{}' had it, {}; {}", kept.name, value_words(own), differ.join(", "));
                    notes.push(Note::warning(Code::StringAttributesDiffer, text).on(kept.id));
                }
            }
            None if held.iter().all(|(_, v)| *v == held[0].1) => {
                attributes.insert(key.to_string(), held[0].1.clone());
                added = true;
            }
            None => {
                let all: Vec<String> = held.iter().map(|(name, v)| format!("'{name}' {}", value_words(v))).collect();
                let text = format!("'{key}' left off '{}': the pieces differ on it ({})", kept.name, all.join(", "));
                notes.push(Note::warning(Code::StringAttributesDiffer, text).on(kept.id));
            }
        }
    }
    (added.then_some(attributes), notes)
}

/// A length for a sentence: metres to the millimetre, and finer where that would read as nothing.
fn metres(length: f64) -> String {
    if length > 0.0 && length < 0.0005 { format!("{length:.6} m") } else { format!("{length:.3} m") }
}

fn so_many(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// Several open strings joined end to end into the first of them (decision 0078).
fn join_the_pieces(inputs: &Inputs<'_>, ids: &[ObjectId], tolerance: f64) -> Result<Outcome, CommandError> {
    let picked = picked(None, ids)?;
    if picked.ids.len() < 2 {
        return Err(CommandError::Invalid(format!("joining takes two strings or more; {} was given", picked.ids.len())));
    }
    let pieces = picked.ids.iter().map(|&id| inputs.string(id)).collect::<Result<Vec<_>, _>>()?;
    let closed: Vec<String> = pieces.iter().filter(|(_, line)| line.closed).map(|(object, _)| format!("'{}': a closed string has no ends to join; open it first", object.name)).collect();
    if !closed.is_empty() {
        return Err(cannot(closed, pieces.len()));
    }
    let lines: Vec<&Polyline> = pieces.iter().map(|&(_, line)| line).collect();
    let branching = branching_ends(&lines);
    if let Some((at, ending)) = branching.first() {
        let mut there: Vec<&str> = ending.iter().map(|&i| pieces[i].0.name.as_str()).collect();
        there.dedup();
        let more = if branching.len() > 1 { format!(", and at {} more", so_many(branching.len() - 1, "place")) } else { String::new() };
        return Err(CommandError::Invalid(format!(
            "{} ends meet at {:.3}, {:.3} ({}){more}, so which two join isn't known and nothing was changed. Join two of them first, or leave one out",
            ending.len(),
            at[0],
            at[1],
            names(&there)
        )));
    }
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(CommandError::Invalid(format!("the tolerance must be nothing or more; it was {tolerance} m")));
    }
    let (joined, joins) = join_pieces(&lines, tolerance).map_err(|e| CommandError::Invalid(e.to_string()))?;
    let (kept, _) = pieces[0];
    let closed = joined.closed;
    let met = joins.iter().filter(|j| j.gap == 0.0).count();
    let near = joins.iter().filter(|j| j.gap > 0.0 && j.gap <= tolerance).count();
    let gaps: Vec<_> = joins.iter().filter(|j| j.gap > tolerance).collect();
    let mut said = format!("joined {} strings into '{}': {} vertices; {} met", pieces.len(), kept.name, joined.vertices.len(), so_many(met, "join"));
    if near > 0 {
        said += &format!(", {near} within {} made one vertex", metres(tolerance));
    }
    if !gaps.is_empty() {
        said += &format!(", {} bridged, {} in all", so_many(gaps.len(), "gap"), metres(gaps.iter().map(|j| j.gap).sum()));
    }
    if closed {
        said += "; closed, as its two ends meet";
    }
    let others: Vec<&Object> = pieces[1..].iter().map(|(o, _)| o.as_ref()).collect();
    let (attributes, differ) = joined_attributes(kept, &others);
    let mut changes = vec![Change::Revise { id: kept.id, base: kept.revision, name: None, layer: None, attributes, body: Some(Body::String(joined)) }];
    changes.extend(pieces[1..].iter().map(|(other, _)| Change::Delete { id: other.id, base: other.revision }));
    let mut outcome = Outcome::new(changes).message(said);
    // Each gap, biggest first: the ones to look at.
    let mut biggest = gaps;
    biggest.sort_by(|a, b| b.gap.total_cmp(&a.gap));
    outcome.notes.extend(biggest.iter().map(|j| {
        let plan = if j.plan_gap == 0.0 { ", straight up: the two ends are over each other in plan".to_string() } else { format!(", {} in plan", metres(j.plan_gap)) };
        let text = format!("bridged a {} gap between '{}' and '{}' at {:.3}, {:.3}{plan}", metres(j.gap), pieces[j.before].0.name, pieces[j.after].0.name, j.at[0], j.at[1]);
        Note::warning(Code::StringGapBridged, text).on(kept.id).measured(j.gap, "m")
    }));
    outcome.notes.extend(differ);
    Ok(outcome)
}

impl Command for JoinStrings {
    type Params = JoinStringsParams;
    const NAME: &'static str = "string.join";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Join another string onto this one, or several end to end into one",
            inputs: vec![
                Field::object("string", ObjectKind::String, "The string to join onto, with `other`"),
                Field::object("other", ObjectKind::String, "The string to join on; it is deleted"),
                Field::new("strings", FieldType::Objects, "Or several open strings to join end to end into the first; the others are deleted"),
            ],
            params: vec![],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let (string, other) = match (p.string, p.other, p.strings.is_empty()) {
            (Some(string), Some(other), true) => (string, other),
            (None, None, false) => return join_the_pieces(inputs, &p.strings, p.tolerance),
            (None, None, true) => return Err(CommandError::Invalid("no strings to join: give `string` and `other`, or two or more as `strings`".into())),
            (_, _, false) => return Err(CommandError::Invalid("give `string` and `other`, or `strings`, not both".into())),
            _ => return Err(CommandError::Invalid("joining one string onto another takes both `string` and `other`".into())),
        };
        if string == other {
            return Err(CommandError::Invalid("a string cannot be joined to itself; close it instead".into()));
        }
        let (object, line) = inputs.string(string)?;
        let (other, other_line) = inputs.string(other)?;
        let (joined, gap) = join_strings(line, other_line).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let message = if gap == 0.0 {
            format!("joined '{}' on: {} vertices", other.name, joined.vertices.len())
        } else {
            format!("joined '{}' on, bridging a {gap:.3} m gap: {} vertices", other.name, joined.vertices.len())
        };
        Ok(Outcome::new(vec![Change::revise_body(object, Body::String(joined)), Change::Delete { id: other.id, base: other.revision }]).message(message))
    }
}

fn default_spacing() -> f64 {
    5.0
}

/// `string.smooth`: a smooth curve through every vertex (centripetal Catmull-Rom), with vertices
/// added so no step is longer than `spacing` in plan. The original vertices stay where they are. A
/// new revision of the string.
pub struct SmoothString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmoothStringParams {
    /// The string, or `strings` for several as one run (decision 0078).
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    #[serde(default = "default_spacing")]
    pub spacing: f64,
}

impl Command for SmoothString {
    type Params = SmoothStringParams;
    const NAME: &'static str = "string.smooth";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Smooth a string through its vertices",
            inputs: vec![string_input(), strings_input()],
            params: vec![Field::new("spacing", FieldType::Number { unit: "m" }, "Longest step along the curve in plan; 5 m by default")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        edit_each(inputs, &picked, &format!("smoothed at {} m", p.spacing), |_, line| {
            let smoothed = smooth_string(line, p.spacing).map_err(|e| CommandError::Invalid(e.to_string()))?;
            let message = format!("smoothed at {} m: {} vertices, was {}", p.spacing, smoothed.vertices.len(), line.vertices.len());
            Ok((smoothed, Some(Note::info(message))))
        })
    }
}

/// `string.fillet` (String tools plan, workstream 2, decision 0083): every corner of each string picked turning more
/// than `angle` degrees in plan rounded to `radius` metres, a vertex every `spacing` metres along each arc, heights graded
/// evenly through it from the legs' own. A corner whose legs can't hold the radius takes the biggest they can, and the
/// run says which and what it got. A new revision of each string; one with no corner past the angle is left as it was.
pub struct FilletString;

fn default_fillet_angle() -> f64 {
    10.0
}

fn default_fillet_spacing() -> f64 {
    2.0
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilletStringParams {
    /// The string, or `strings` for several as one run (decision 0078).
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    pub radius: f64,
    /// Corners turning more than this, in degrees, are rounded; 10 unless given.
    #[serde(default = "default_fillet_angle")]
    pub angle: f64,
    /// Metres between vertices along an arc; 2 unless given.
    #[serde(default = "default_fillet_spacing")]
    pub spacing: f64,
}

impl Command for FilletString {
    type Params = FilletStringParams;
    const NAME: &'static str = "string.fillet";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Round each string's corners turning more than an angle to a radius",
            inputs: vec![string_input(), strings_input()],
            params: vec![
                Field::new("radius", FieldType::Number { unit: "m" }, "The radius of each corner in plan"),
                Field::new("angle", FieldType::Number { unit: "deg" }, "Corners turning more than this are rounded (10 unless given)"),
                Field::new("spacing", FieldType::Number { unit: "m" }, "Metres between vertices along an arc (2 unless given)"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        let words = format!("corners over {}° rounded at {} m", p.angle, p.radius);
        edit_each(inputs, &picked, &words, |_, line| {
            let (round, corners) = fillet_string(line, p.radius, p.angle, p.spacing).map_err(|e| CommandError::Invalid(e.to_string()))?;
            if corners.is_empty() {
                return Ok((line.clone(), None));
            }
            let tight: Vec<String> = corners.iter().filter(|c| c.tightened).map(|c| format!("vertex {} at {:.2} m", c.vertex + 1, c.radius)).collect();
            let mut message = format!("{} corner{} rounded at {} m", corners.len(), if corners.len() == 1 { "" } else { "s" }, p.radius);
            if !tight.is_empty() {
                message += &format!("; legs too short for it, so the biggest that fits: {}", tight.join(", "));
            }
            Ok((round, Some(Note::info(message))))
        })
    }
}

/// `string.filter`: vertices within `tolerance` metres (in 3D) of the line through their kept
/// neighbours removed (Douglas-Peucker); the ends of an open string stay. A new revision.
pub struct FilterString;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterStringParams {
    /// The string, or `strings` for several as one run (decision 0078).
    #[serde(default)]
    pub string: Option<ObjectId>,
    #[serde(default)]
    pub strings: Vec<ObjectId>,
    pub tolerance: f64,
}

impl Command for FilterString {
    type Params = FilterStringParams;
    const NAME: &'static str = "string.filter";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Remove vertices within a tolerance of the line",
            inputs: vec![string_input(), strings_input()],
            params: vec![Field::new("tolerance", FieldType::Number { unit: "m" }, "Largest 3D distance a removed vertex may be from the line")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let picked = picked(p.string, &p.strings)?;
        edit_each(inputs, &picked, &format!("filtered at {} m", p.tolerance), |_, line| {
            let filtered = filter_string(line, p.tolerance).map_err(|e| CommandError::Invalid(e.to_string()))?;
            let removed = line.vertices.len() - filtered.vertices.len();
            let message = format!("filtered at {} m: {removed} of {} vertices removed", p.tolerance, line.vertices.len());
            Ok((filtered, Some(Note::info(message))))
        })
    }
}
