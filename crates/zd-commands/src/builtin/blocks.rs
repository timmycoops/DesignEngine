//! Blocks (decision 0062). A block is a named polygon in plan, and a solid is cut vertically where it sits. The
//! polygons are made the way a planner lays blocks out: a line across the strips offset every 100 m, lines drawn to close
//! them, and a polygon made of each area they close (`string.polygons_from_lines`).

use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ObjectId, ObjectKind, Polyline};
use zd_geom::polygons_from_lines;

use super::surface::site;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Place};

/// More joins or short ends than this and the run sums them up after listing the first ones.
const LINES_UP_TO: usize = 10;

/// `string.polygons_from_lines`: a closed string for every area a set of strings closes in plan. Lines are joined
/// where they cross; an end stopping short of another line by no more than the tolerance is joined to it and said.
/// What closes nothing is left out: a line running on past the last line it crosses is counted, a line ending inside
/// a polygon it was meant to split is listed with its gap, and a line closing nothing at all is named. The lines are
/// left as they are.
pub struct PolygonsFromLines;

fn default_tolerance() -> f64 {
    0.001
}

fn default_name() -> String {
    "Polygon #".into()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolygonsFromLinesParams {
    /// The lines, open or closed.
    pub strings: Vec<ObjectId>,
    /// Layer for the polygons.
    pub layer: String,
    /// An end this close to another line (m) is joined to it; 1 mm by default.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// Each polygon's name, numbered west to east in place of `#`, or on the end if there's no `#`.
    #[serde(default = "default_name")]
    pub name: String,
}

impl PolygonsFromLinesParams {
    fn name_of(&self, number: usize) -> String {
        if self.name.contains('#') { self.name.replace('#', &number.to_string()) } else { format!("{} {number}", self.name) }
    }
}

impl Command for PolygonsFromLines {
    type Params = PolygonsFromLinesParams;
    const NAME: &'static str = "string.polygons_from_lines";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Make a polygon of every area a set of lines closes in plan",
            inputs: vec![Field::new("strings", FieldType::Objects, "The lines, open or closed")],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the polygons"),
                Field::new("tolerance", FieldType::Number { unit: "m" }, "An end this close to another line is joined to it; 1 mm by default"),
                Field::new("name", FieldType::Text, "Each polygon's name, numbered west to east in place of #; Polygon # by default"),
            ],
            outputs: vec![Field::object("polygons", ObjectKind::String, "A closed string for each area, west to east")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.strings.is_empty() {
            return Err(CommandError::Invalid("give the lines".into()));
        }
        if !(p.tolerance.is_finite() && p.tolerance >= 0.0) {
            return Err(CommandError::Invalid(format!("a tolerance of {} m isn't a distance", p.tolerance)));
        }
        let mut objects = Vec::new();
        let mut lines: Vec<Polyline> = Vec::new();
        for &id in &p.strings {
            if objects.iter().any(|o: &&zd_core::Object| o.id == id) {
                return Err(CommandError::Invalid(format!("string {id} is given twice")));
            }
            let (object, line) = inputs.string(id)?;
            objects.push(object);
            lines.push(line.clone());
        }
        let started = Instant::now();
        let made = polygons_from_lines(&lines, p.tolerance).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        let name = |k: usize| objects[k].name.as_str();
        let mut notes = Vec::new();
        for (k, end) in made.short.iter().enumerate() {
            if k == LINES_UP_TO {
                notes.push(Note::warning(Code::StringEndShort, format!("{} more ends stop short inside polygons", made.short.len() - LINES_UP_TO)).measured((made.short.len() - LINES_UP_TO) as f64, "ends"));
                break;
            }
            let text = format!(
                "'{}' ends {:.3} m short of the nearest line at {}, inside {}, which it doesn't split: extend it to meet the line, or set the tolerance past the gap",
                name(end.line),
                end.gap,
                site(&end.at),
                p.name_of(end.inside + 1)
            );
            notes.push(Note::warning(Code::StringEndShort, text).on(objects[end.line].id).at(Place::Point { at: end.at }).measured(end.gap, "m"));
        }
        for &k in &made.unused {
            notes.push(Note::warning(Code::StringClosesNothing, format!("'{}' closes no area: left out", name(k))).on(objects[k].id));
        }
        for (at, k) in &made.islands {
            let text = format!("Lines at {} lie inside {} without touching its sides, so they don't split it, and their own polygons overlap it", site(at), p.name_of(k + 1));
            notes.push(Note::warning(Code::StringIsland, text).at(Place::Point { at: *at }));
        }
        for (k, joined) in made.joined.iter().enumerate() {
            if k == LINES_UP_TO {
                notes.push(Note::new(crate::Severity::Info, Some(Code::StringEndJoined), format!("{} more ends joined", made.joined.len() - LINES_UP_TO)).measured((made.joined.len() - LINES_UP_TO) as f64, "ends"));
                break;
            }
            let text = format!("'{}' joined to the nearest line across {:.1} mm at {}", name(joined.line), joined.gap * 1000.0, site(&joined.at));
            notes.push(Note::new(crate::Severity::Info, Some(Code::StringEndJoined), text).on(objects[joined.line].id).at(Place::Point { at: joined.at }).measured(joined.gap, "m"));
        }
        if made.polygons.is_empty() {
            let why = notes.first().map_or(String::new(), |n: &Note| format!(": {}", n.text));
            return Err(CommandError::Invalid(format!("these {} lines close no area{why}", lines.len())));
        }
        let (least, most) = made.polygons.iter().fold((f64::INFINITY, 0.0_f64), |(a, b), polygon| (a.min(polygon.area), b.max(polygon.area)));
        let mut outcome = Outcome::new(
            made.polygons
                .iter()
                .enumerate()
                .map(|(k, polygon)| Change::Create {
                    name: p.name_of(k + 1),
                    layer: p.layer.clone(),
                    attributes: Attributes::new(),
                    body: Body::String(Polyline { vertices: polygon.vertices.clone(), closed: true }),
                })
                .collect(),
        )
        .message(format!(
            "{} polygons from {} lines, {:.1} to {:.1} m² in plan, in {ms:.0} ms: {} segments in, {} crossings",
            made.polygons.len(),
            lines.len(),
            least,
            most,
            made.segments,
            made.crossings
        ));
        if made.run_past > 0 {
            outcome = outcome.message(format!("{} ends run on past the last line they cross, {:.1} m of line in all, outside every polygon: left out", made.run_past, made.run_past_length));
        }
        outcome.notes.extend(notes);
        Ok(outcome)
    }
}
