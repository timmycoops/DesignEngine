//! Blocks (decision 0062). A block is a named polygon in plan, and a solid is cut vertically where it sits. The
//! polygons are made the way a planner lays blocks out: a line across the strips offset every 100 m, lines drawn to close
//! them, and a polygon made of each area they close (`string.polygons_from_lines`). Each solid is then cut by each
//! polygon's vertical prism (`solid.cut_by_polygons`), the piece taking the polygon's name as its block. Strips are cut by
//! the seam model's horizons in one run (`solid.cut_by_seams`), each piece carrying its seam, material and horizon.

use std::collections::BTreeMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, Horizon, Lattice, Object, ObjectId, ObjectKind, Polyline, Solid};
use zd_geom::{GridCover, LayerCut, LayerStack, PolygonCut, Uncovered, cut_solid_by_layers, cut_solid_by_polygons, find_solid_crossings, layer_stack, polygons_from_lines, polygons_overlapping, polygons_uncovered, solid_off_the_grids};
use zd_io::read_stack_grids;

use super::{in_parallel, in_parallel_watched};
use super::solid::checked_input;
use super::surface::{default_tolerance as crossing_tolerance, site};
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Place, Severity};

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

/// `solid.cut_by_polygons`: each solid cut vertically by each block polygon (decision 0062). A piece keeps its solid's
/// attributes, takes the polygon's name as its block and any of the polygon's attributes the solid hasn't, and is named
/// from a pattern of attributes. The polygons must cover every solid in plan: a solid reaching outside them is refused
/// before anything is cut, each gap listed, as the polygons are what to fix ([`check_cut_by_polygons`] finds them on its
/// own, for the panel). Polygons that overlap are refused, as they'd count the overlap twice. The blocks add back to the
/// solid, and any difference is said. The inputs are left as they are.
pub struct CutByPolygons;

fn default_block() -> String {
    "block".into()
}

fn default_pattern() -> String {
    "{strip}_{block}_{horizon}".into()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutByPolygonsParams {
    /// The solids to cut, each on its own.
    pub solids: Vec<ObjectId>,
    /// The block polygons: closed strings.
    pub polygons: Vec<ObjectId>,
    /// Layer for the blocks.
    pub layer: String,
    /// The attribute that takes each polygon's name; `block` by default.
    #[serde(default = "default_block")]
    pub attribute: String,
    /// Each block's name: attributes in braces, `{solid}` the solid's name and `{polygon}` the polygon's; one a block
    /// hasn't is left out with the separator before it. `{strip}_{block}_{horizon}` by default: a coal block's horizon is its
    /// seam, and waste's tells it from the coal of the seam it goes with (OB, U-M).
    #[serde(default = "default_pattern")]
    pub name: String,
}

/// Overlaps smaller than this (m²) are a shared edge drawn twice, not an overlap.
const OVERLAP: f64 = 0.01;

/// A difference between the pieces and their solid within this share of its volume is rounding.
const ADDS_UP: f64 = 1e-6;

/// More blocks with one name than this, or solids the polygons don't cover, and the run sums them up.
const NAMED_UP_TO: usize = 5;

/// What separates the parts of a name.
fn separator(c: char) -> bool {
    "_- ".contains(c)
}

/// A value as it goes in a name: whole numbers without a decimal point.
fn word(value: &AttrValue) -> String {
    match value {
        AttrValue::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
        AttrValue::Number(n) => n.to_string(),
        AttrValue::Text(t) => t.trim().to_string(),
        AttrValue::Bool(b) => b.to_string(),
    }
}

/// The name from the pattern: each `{key}` its value; one with no value is left out with the separator before it.
fn block_name(pattern: &str, attributes: &Attributes, solid: &str, polygon: &str) -> String {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        let key = &rest[open + 1..open + close];
        let value = match key {
            "solid" => solid.to_string(),
            "polygon" => polygon.to_string(),
            _ => attributes.get(key).map(word).unwrap_or_default(),
        };
        let before = &rest[..open];
        if value.is_empty() {
            // Words before it stay; the separator just before it goes.
            out.push_str(before.trim_end_matches(separator));
        } else if out.is_empty() {
            out.push_str(before.trim_start_matches(separator));
            out.push_str(&value);
        } else {
            out.push_str(before);
            out.push_str(&value);
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    let name = out.trim_matches(separator);
    if name.is_empty() { format!("{solid} {polygon}") } else { name.to_string() }
}

/// A closed string's ring in plan, its last point dropped where it repeats the first.
fn plan_ring(line: &Polyline) -> Vec<[f64; 2]> {
    let mut ring: Vec<[f64; 2]> = line.vertices.iter().map(|v| [v[0], v[1]]).collect();
    if ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    ring
}

/// The first few names, quoted, and how many more.
fn named(names: &[String]) -> String {
    let first = names.iter().take(NAMED_UP_TO).map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
    if names.len() > NAMED_UP_TO { format!("{first} and {} more", names.len() - NAMED_UP_TO) } else { first }
}

/// What a cut by polygons takes, checked: the polygons as plan rings and the solids, refusing what can't be cut.
#[allow(clippy::type_complexity)]
fn polygon_inputs<'a>(inputs: &Inputs<'a>, p: &CutByPolygonsParams, notes: &mut Vec<Note>) -> Result<(Vec<&'a Object>, Vec<Vec<[f64; 2]>>, Vec<(&'a Object, &'a Solid)>), CommandError> {
    if p.solids.is_empty() {
        return Err(CommandError::Invalid("give the solids to cut".into()));
    }
    if p.polygons.is_empty() {
        return Err(CommandError::Invalid("give the block polygons".into()));
    }
    if p.attribute.trim().is_empty() {
        return Err(CommandError::Invalid("the block's name needs an attribute to go in".into()));
    }
    let mut polygons: Vec<&Object> = Vec::new();
    let mut rings = Vec::new();
    let mut open = Vec::new();
    for &id in &p.polygons {
        if polygons.iter().any(|o| o.id == id) {
            return Err(CommandError::Invalid(format!("polygon {id} is given twice")));
        }
        let (object, line) = inputs.string(id)?;
        let ring = plan_ring(line);
        if ring.len() < 3 {
            return Err(CommandError::Invalid(format!("'{}' has fewer than three points in plan, so it closes no area", object.name)));
        }
        if !line.closed {
            open.push(object.name.clone());
        }
        polygons.push(object);
        rings.push(ring);
    }
    if !open.is_empty() {
        let text = format!("{} open strings taken as closed, from their last point back to their first: {}", open.len(), named(&open));
        notes.push(Note::new(Severity::Info, Some(Code::SolidOutlineOpen), text).measured(open.len() as f64, "strings"));
    }
    let overlaps = polygons_overlapping(&rings, OVERLAP);
    if let Some(&(i, j, area, at)) = overlaps.first() {
        let more = if overlaps.len() > 1 { format!(", and {} more pairs overlap", overlaps.len() - 1) } else { String::new() };
        return Err(CommandError::Invalid(format!(
            "'{}' and '{}' overlap by {area:.2} m² about {:.3} {:.3}, so a block there would be counted twice{more}: nothing cut",
            polygons[i].name, polygons[j].name, at[0], at[1]
        )));
    }

    let mut solids: Vec<(&Object, &Solid)> = Vec::new();
    for &id in &p.solids {
        if solids.iter().any(|(o, _)| o.id == id) {
            return Err(CommandError::Invalid(format!("solid {id} is given twice")));
        }
        let (object, solid) = inputs.solid(id)?;
        checked_input("solid", &object.name, solid.crossings(), solid.mesh(), true, notes)?;
        solids.push((object.as_ref(), solid));
    }
    Ok((polygons, rings, solids))
}

/// Where the polygons leave a solid uncovered.
fn uncovered(object: &Object, solid: &Solid, rings: &[Vec<[f64; 2]>]) -> Result<Vec<Uncovered>, CommandError> {
    polygons_uncovered(solid.mesh(), rings).map_err(|e| CommandError::Geometry(format!("'{}': {e}", object.name)))
}

/// A gap in words: its area and where it is.
fn gap_words(gap: &Uncovered) -> String {
    format!("{:.1} m² about {:.3} {:.3}", gap.area, gap.at[0], gap.at[1])
}

/// The refusal for solids the polygons don't cover, each with its largest gap and how many more.
fn refusal(short: &[(&Object, Vec<Uncovered>)]) -> String {
    let each: Vec<String> = short
        .iter()
        .take(NAMED_UP_TO)
        .map(|(object, gaps)| {
            let total: f64 = gaps.iter().map(|g| g.area).sum();
            let more = if gaps.len() > 1 { format!(" and {} more places, {total:.1} m² in all", gaps.len() - 1) } else { String::new() };
            format!("'{}' reaches outside them by {}{more}", object.name, gap_words(&gaps[0]))
        })
        .collect();
    let more = if short.len() > NAMED_UP_TO { format!("; and {} more solids", short.len() - NAMED_UP_TO) } else { String::new() };
    let solids = if short.len() == 1 { "a solid".to_string() } else { format!("{} solids", short.len()) };
    format!("the block polygons don't cover {solids}, so nothing is cut: {}{more}. Carry the polygons on over the whole of each solid, then cut", each.join("; "))
}

/// Where block polygons leave the solids uncovered, read off the project as it stands, for the Cut solids panel to show
/// before a cut: a warning for each gap, on its solid and at its place, or a line saying the polygons cover them all.
pub fn check_cut_by_polygons(project: &zd_core::Project, p: &CutByPolygonsParams) -> Result<Vec<Note>, CommandError> {
    let inputs = Inputs::of(project);
    let mut notes = Vec::new();
    let (_, rings, solids) = polygon_inputs(&inputs, p, &mut notes)?;
    let gaps = in_parallel(&solids, |(object, solid)| uncovered(object, solid, &rings));
    let mut covered = 0;
    for ((object, _), gaps) in solids.iter().zip(gaps) {
        let gaps = gaps?;
        if gaps.is_empty() {
            covered += 1;
        }
        for gap in gaps {
            let text = format!("'{}' reaches outside the polygons by {}", object.name, gap_words(&gap));
            notes.push(Note::warning(Code::SolidOutsidePolygons, text).on(object.id).at(Place::Point { at: [gap.at[0], gap.at[1], 0.0] }).measured(gap.area, "m2"));
        }
    }
    if covered == solids.len() {
        notes.push(Note::info(format!("the polygons cover all {} solids", solids.len())));
    }
    Ok(notes)
}

type Cut = (PolygonCut, Vec<Result<Solid, CommandError>>);

impl Command for CutByPolygons {
    type Params = CutByPolygonsParams;
    const NAME: &'static str = "solid.cut_by_polygons";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Cut solids into blocks by polygons: a vertical cut where each polygon sits, the piece taking its name",
            inputs: vec![Field::new("solids", FieldType::Objects, "The solids to cut, each on its own"), Field::new("polygons", FieldType::Objects, "The block polygons: closed strings")],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the blocks"),
                Field::new("attribute", FieldType::Text, "The attribute taking each polygon's name; block by default"),
                Field::new("name", FieldType::Text, "Each block's name from its attributes in braces; {strip}_{block}_{horizon} by default"),
            ],
            outputs: vec![Field::new("blocks", FieldType::Objects, "Each solid's blocks in the polygons' order")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let mut notes = Vec::new();
        let (polygons, rings, solids) = polygon_inputs(inputs, p, &mut notes)?;
        let gaps = in_parallel_watched(inputs, "solids", &solids, |(object, solid)| uncovered(object, solid, &rings))?;
        let mut short: Vec<(&Object, Vec<Uncovered>)> = Vec::new();
        for ((object, _), gaps) in solids.iter().zip(gaps) {
            let gaps = gaps?;
            if !gaps.is_empty() {
                short.push((object, gaps));
            }
        }
        if !short.is_empty() {
            return Err(CommandError::Invalid(refusal(&short)));
        }

        let started = Instant::now();
        let cuts = in_parallel_watched(inputs, "solids", &solids, |(object, solid)| -> Result<Cut, CommandError> {
            let cut = cut_solid_by_polygons(solid.mesh(), &rings).map_err(|e| CommandError::Geometry(format!("'{}': {e}", object.name)))?;
            // Each piece searched for crossings here, on its own thread, so the check after takes it as it is.
            // What's left outside them, once the polygons are checked to cover the solid, is slivers along their edges.
            let made = cut
                .pieces
                .iter()
                .map(|piece| {
                    let crossings = find_solid_crossings(&piece.mesh, crossing_tolerance());
                    Solid::new(piece.mesh.clone()).map(|s| s.with_crossings(crossings)).map_err(CommandError::NotWatertight)
                })
                .collect();
            Ok((cut, made))
        })?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;

        let mut changes = Vec::new();
        let mut names: BTreeMap<String, usize> = BTreeMap::new();
        let mut blocks = 0;
        let (mut worst, mut worst_of) = (0.0_f64, String::new());
        let (mut volume_in, mut volume_blocks, mut volume_slivers) = (0.0, 0.0, 0.0);
        let (mut triangles_in, mut triangles_out) = (0, 0);
        for ((object, solid), result) in solids.iter().zip(cuts) {
            let (cut, made) = result?;
            let mut made = made.into_iter();
            triangles_in += solid.mesh().triangle_count();
            volume_in += cut.input_volume;
            let slivers = cut.remainder.as_ref().map_or(0.0, |rest| rest.volume);
            volume_slivers += slivers;
            let difference = cut.difference() - slivers;
            let share = difference.abs() / cut.input_volume.abs().max(f64::MIN_POSITIVE);
            if share > worst {
                worst = share;
                worst_of = object.name.clone();
            }
            if share > ADDS_UP {
                let text = format!("'{}': its blocks come to {difference:+.3} m³ against its {:.3} m³", object.name, cut.input_volume);
                notes.push(Note::warning(Code::BlockVolumeDiffers, text).on(object.id).measured(difference, "m3"));
            }
            for piece in &cut.pieces {
                let solid = made.next().expect("a solid for each piece")?;
                let polygon = polygons[piece.polygon];
                let mut attributes = object.attributes.clone();
                attributes.insert(p.attribute.clone(), AttrValue::Text(polygon.name.clone()));
                for (key, value) in polygon.attributes.iter() {
                    if !attributes.contains_key(key) {
                        attributes.insert(key.to_string(), value.clone());
                    }
                }
                let name = block_name(&p.name, &attributes, &object.name, &polygon.name);
                *names.entry(name.clone()).or_default() += 1;
                blocks += 1;
                volume_blocks += piece.volume;
                triangles_out += piece.mesh.triangle_count();
                changes.push(Change::Create { name, layer: p.layer.clone(), attributes, body: Body::Solid(solid) });
            }
        }
        if blocks == 0 {
            return Err(CommandError::Invalid(format!("none of the {} solids reaches into any of the {} polygons: nothing cut", solids.len(), polygons.len())));
        }
        if volume_slivers > 0.0 {
            let text = format!("{volume_slivers:.3} m³ in slivers under {} mm wide along the polygons' edges is in no block", zd_geom::COVER_SLIVER * 1000.0);
            notes.push(Note::info(text).measured(volume_slivers, "m3"));
        }
        let repeated: Vec<String> = names.iter().filter(|(_, n)| **n > 1).map(|(name, _)| name.clone()).collect();
        if !repeated.is_empty() {
            let sharing: usize = names.values().filter(|n| **n > 1).sum();
            let text = format!("{sharing} blocks share {} names: {}. Block names should be unique: put an attribute that tells them apart in the name pattern", repeated.len(), named(&repeated));
            notes.push(Note::warning(Code::BlockNameRepeated, text).measured(repeated.len() as f64, "names"));
        }
        let mut outcome = Outcome::new(changes)
            .message(format!("{} solids, every one inside the polygons, cut by {} polygons into {blocks} blocks, in {ms:.0} ms: {triangles_in} triangles in, {triangles_out} out", solids.len(), polygons.len()))
            .message(format!("Volumes: {volume_in:.3} m³ in, {volume_blocks:.3} m³ in blocks"))
            .message(if worst_of.is_empty() {
                "Blocks add back to every solid exactly".to_string()
            } else {
                format!("Blocks add back to each solid within {worst:.1e} of its volume, the most for '{worst_of}'")
            });
        // Blocks in more than one part are said by the solid rules' check, which every run's solids go through.
        outcome.notes.extend(notes);
        Ok(outcome)
    }
}

/// `solid.cut_by_seams`: each solid cut by a seam model's horizons in one run (decision 0062), into the overburden, each
/// seam, each interburden and what's below the last floor. Each layer is a slab between its two grids, read from the
/// model's linked files, made where it's thicker than a minimum; what's thinner is left out and its volume said. A piece
/// keeps its solid's attributes and carries `seam` (its own for coal, the seam below it for waste), `material` and
/// `horizon`. The model's grids must reach over the whole of each solid in plan with no hole under it: that's checked
/// before anything is cut, and a solid past their edge or over a hole is refused with each place listed, the geology
/// model being what to fix (decision 0063). The inputs are left as they are.
pub struct CutBySeams;

fn default_min_thickness() -> f64 {
    0.01
}

fn default_flap_thickness() -> f64 {
    0.01
}

fn default_seam_pattern() -> String {
    "{solid} {horizon}".into()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutBySeamsParams {
    /// The solids to cut, each on its own.
    pub solids: Vec<ObjectId>,
    /// The seam model whose working section cuts them.
    pub model: ObjectId,
    /// Layer for the pieces.
    pub layer: String,
    /// A layer is made only where it's thicker than this (m); 0.01 by default.
    #[serde(default = "default_min_thickness")]
    pub min_thickness: f64,
    /// Where a piece is thinner than this between its top and bottom (m), a flap, it's taken off and its volume said
    /// (decision 0064); 0.01 by default, 0 for none.
    #[serde(default = "default_flap_thickness")]
    pub flap_thickness: f64,
    /// Each piece's name: attributes in braces, `{solid}` the solid's name. `{solid} {horizon}` by default.
    #[serde(default = "default_seam_pattern")]
    pub name: String,
}

/// What one layer of the stack is: its horizon's name, its material and the seam it goes with.
struct LayerKind {
    horizon: String,
    material: String,
    seam: Option<String>,
}

/// The layers of a working section, top down: overburden, then each horizon and the waste below it.
fn layer_kinds(working: &[&Horizon]) -> Vec<LayerKind> {
    let waste = |horizon: String, seam: Option<&Horizon>| LayerKind { horizon, material: "waste".into(), seam: seam.map(|h| h.code.clone()) };
    let mut kinds = vec![waste("OB".into(), working.first().copied())];
    for (k, h) in working.iter().enumerate() {
        let material = if h.material.trim().is_empty() { "coal".to_string() } else { h.material.trim().to_string() };
        kinds.push(LayerKind { horizon: h.code.clone(), material, seam: Some(h.code.clone()) });
        kinds.push(match working.get(k + 1) {
            Some(below) => waste(format!("{}-{}", h.code, below.code), Some(below)),
            None => waste(format!("below {}", h.code), None),
        });
    }
    kinds
}

type SeamCut = (LayerCut, Vec<Result<Solid, CommandError>>);

/// What a cut by seams reads: the model, its working section, the solids, the layers between its grids and where any
/// working horizon has a hole.
struct SeamInputs<'a> {
    model: &'a Object,
    working: Vec<&'a Horizon>,
    solids: Vec<(&'a Object, &'a Solid)>,
    stack: LayerStack,
    holes: Vec<bool>,
    changed: Vec<String>,
}

fn seam_inputs<'a>(inputs: &Inputs<'a>, p: &CutBySeamsParams, notes: &mut Vec<Note>) -> Result<SeamInputs<'a>, CommandError> {
    if p.solids.is_empty() {
        return Err(CommandError::Invalid("give the solids to cut".into()));
    }
    let model_object = inputs.object(p.model)?;
    let Body::SeamModel(model) = &model_object.body else {
        return Err(CommandError::WrongKind { id: p.model, expected: ObjectKind::SeamModel, found: model_object.body.kind() });
    };
    if !model.is_usable() {
        return Err(CommandError::Invalid(format!("'{}' has blocking issues in its stack: correct the grids and build it again first", model_object.name)));
    }
    let working: Vec<&Horizon> = model.working_section().collect();
    if working.is_empty() {
        return Err(CommandError::Invalid(format!("'{}' has no working horizons to cut by", model_object.name)));
    }
    let mut solids: Vec<(&Object, &Solid)> = Vec::new();
    for &id in &p.solids {
        if solids.iter().any(|(o, _)| o.id == id) {
            return Err(CommandError::Invalid(format!("solid {id} is given twice")));
        }
        let (object, solid) = inputs.solid(id)?;
        checked_input("solid", &object.name, solid.crossings(), solid.mesh(), true, notes)?;
        solids.push((object.as_ref(), solid));
    }

    // The working section's roofs and floors, read where the model links them, absent seams blanked, holes flagged.
    let grids = read_stack_grids(model).map_err(|e| CommandError::Invalid(format!("'{}': {e}", model_object.name)))?;
    let mut pairs: Vec<(&[f64], &[f64])> = Vec::new();
    let mut holes = vec![false; model.lattice.cols * model.lattice.rows];
    for (k, (h, (roof, floor))) in model.horizons.iter().zip(grids.roofs.iter().zip(&grids.floors)).enumerate() {
        if !h.working {
            continue;
        }
        if Lattice::from(roof) != model.lattice || Lattice::from(floor) != model.lattice {
            return Err(CommandError::Invalid(format!("'{}': {}'s roof or floor isn't on the model's lattice: build the model again", model_object.name, h.code)));
        }
        pairs.push((&roof.z, &floor.z));
        for (any, &hole) in holes.iter_mut().zip(&grids.holes[k]) {
            *any |= hole;
        }
    }
    let stack = layer_stack(model.lattice, &pairs).map_err(|e| CommandError::Invalid(e.to_string()))?;
    Ok(SeamInputs { model: model_object.as_ref(), working, solids, stack, holes, changed: grids.changed })
}

/// Where a solid reaches past the model's grids or over a hole in them.
fn off_the_grids(object: &Object, solid: &Solid, seams: &SeamInputs<'_>) -> Result<GridCover, CommandError> {
    solid_off_the_grids(solid.mesh(), seams.stack.lattice, &seams.holes).map_err(|e| CommandError::Geometry(format!("'{}': {e}", object.name)))
}

/// Places in words: the largest, and how many more with their area in all.
fn places_words(places: &[Uncovered]) -> String {
    let total: f64 = places.iter().map(|g| g.area).sum();
    let more = if places.len() > 1 { format!(" and {} more places, {total:.1} m² in all", places.len() - 1) } else { String::new() };
    format!("{}{more}", gap_words(&places[0]))
}

/// The refusal for solids the grids don't reach over, each with its largest place past the edge and over a hole.
fn grid_refusal(model: &str, short: &[(&Object, GridCover)]) -> String {
    let each: Vec<String> = short
        .iter()
        .take(NAMED_UP_TO)
        .map(|(object, cover)| {
            let mut said = Vec::new();
            if !cover.past_edge.is_empty() {
                said.push(format!("past their edge by {}", places_words(&cover.past_edge)));
            }
            if !cover.over_holes.is_empty() {
                said.push(format!("over holes in them by {}", places_words(&cover.over_holes)));
            }
            format!("'{}' reaches {}", object.name, said.join(", and "))
        })
        .collect();
    let more = if short.len() > NAMED_UP_TO { format!("; and {} more solids", short.len() - NAMED_UP_TO) } else { String::new() };
    let solids = if short.len() == 1 { "a solid".to_string() } else { format!("{} solids", short.len()) };
    format!(
        "the grids of '{model}' don't reach over {solids} with no hole under it, so nothing is cut: {}{more}. Fix the geology model and import the grids again, or keep the solids to where the grids are, then cut",
        each.join("; ")
    )
}

/// Where a seam model's grids don't reach over the solids, read off the project as it stands, for the Cut solids panel
/// to show before a cut: a warning for each place past their edge or over a hole, on its solid and at its place, or a
/// line saying the grids reach over them all.
pub fn check_cut_by_seams(project: &zd_core::Project, p: &CutBySeamsParams) -> Result<Vec<Note>, CommandError> {
    let inputs = Inputs::of(project);
    let mut notes = Vec::new();
    let seams = seam_inputs(&inputs, p, &mut notes)?;
    let covers = in_parallel(&seams.solids, |(object, solid)| off_the_grids(object, solid, &seams));
    let mut covered = 0;
    for ((object, _), cover) in seams.solids.iter().zip(covers) {
        let cover = cover?;
        if cover.covered() {
            covered += 1;
        }
        let place = |u: &Uncovered| Place::Point { at: [u.at[0], u.at[1], 0.0] };
        for gap in &cover.past_edge {
            let text = format!("'{}' reaches past the edge of the grids by {}", object.name, gap_words(gap));
            notes.push(Note::warning(Code::SolidOffGrids, text).on(object.id).at(place(gap)).measured(gap.area, "m2"));
        }
        for gap in &cover.over_holes {
            let text = format!("'{}' lies over a hole in the grids by {}", object.name, gap_words(gap));
            notes.push(Note::warning(Code::SolidOverGridHoles, text).on(object.id).at(place(gap)).measured(gap.area, "m2"));
        }
    }
    if covered == seams.solids.len() {
        notes.push(Note::info(format!("the grids reach over all {} solids with no hole under them", seams.solids.len())));
    }
    Ok(notes)
}

impl Command for CutBySeams {
    type Params = CutBySeamsParams;
    const NAME: &'static str = "solid.cut_by_seams";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Cut solids by a seam model's horizons in one run: overburden, each seam, each interburden and below",
            inputs: vec![
                Field::new("solids", FieldType::Objects, "The solids to cut, each on its own"),
                Field::object("model", ObjectKind::SeamModel, "The seam model whose working section cuts them"),
            ],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the pieces"),
                Field::new("min_thickness", FieldType::Number { unit: "m" }, "A layer is made only where it's thicker than this; 0.01 by default"),
                Field::new("flap_thickness", FieldType::Number { unit: "m" }, "Flaps thinner than this are taken off each piece and said; 0.01 by default, 0 for none"),
                Field::new("name", FieldType::Text, "Each piece's name from its attributes in braces; {solid} {horizon} by default"),
            ],
            outputs: vec![Field::new("pieces", FieldType::Objects, "Each solid's pieces top down")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if !(p.min_thickness > 0.0 && p.min_thickness.is_finite()) {
            return Err(CommandError::Invalid(format!("a minimum thickness of {} m isn't more than nothing", p.min_thickness)));
        }
        if !(p.flap_thickness >= 0.0 && p.flap_thickness.is_finite()) {
            return Err(CommandError::Invalid(format!("a flap thickness of {} m is less than nothing", p.flap_thickness)));
        }
        let mut notes = Vec::new();
        let started = Instant::now();
        let seams = seam_inputs(inputs, p, &mut notes)?;
        let covers = in_parallel_watched(inputs, "solids", &seams.solids, |(object, solid)| off_the_grids(object, solid, &seams))?;
        let mut short: Vec<(&Object, GridCover)> = Vec::new();
        for ((object, _), cover) in seams.solids.iter().zip(covers) {
            let cover = cover?;
            if !cover.covered() {
                short.push((object, cover));
            }
        }
        if !short.is_empty() {
            return Err(CommandError::Invalid(grid_refusal(&seams.model.name, &short)));
        }
        let (model_object, working, solids, stack) = (seams.model, &seams.working, &seams.solids, &seams.stack);
        if !seams.changed.is_empty() {
            let text = format!("{} grid files differ from when '{}' was built, and were read as they are now: {}", seams.changed.len(), model_object.name, named(&seams.changed));
            notes.push(Note::warning(Code::LinkChanged, text).measured(seams.changed.len() as f64, "files").on(model_object.id));
        }
        if stack.absent > 0 {
            let text = format!("{} horizon nodes hold no seam: each laid on the floor of the seam above, or the roof of the one below at the top, so the waste there goes with the seam below", stack.absent);
            notes.push(Note::info(text).measured(stack.absent as f64, "nodes"));
        }
        if stack.lowered > 0 {
            let text = format!("{} roof and floor nodes lay above the surface over them, by up to {:.4} m, within the model's tolerance: lowered onto it for the cut", stack.lowered, stack.most_lowered);
            notes.push(Note::info(text).measured(stack.most_lowered, "m"));
        }
        let read_ms = started.elapsed().as_secs_f64() * 1000.0;

        let cuts = in_parallel_watched(inputs, "solids", solids, |(object, solid)| -> Result<SeamCut, CommandError> {
            let cut = cut_solid_by_layers(solid.mesh(), stack, p.min_thickness, p.flap_thickness).map_err(|e| CommandError::Geometry(format!("'{}': {e}", object.name)))?;
            let made = cut
                .pieces
                .iter()
                .map(|piece| {
                    let crossings = find_solid_crossings(&piece.mesh, crossing_tolerance());
                    Solid::new(piece.mesh.clone()).map(|s| s.with_crossings(crossings)).map_err(CommandError::NotWatertight)
                })
                .collect();
            Ok((cut, made))
        })?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;

        let kinds = layer_kinds(working);
        let mut changes = Vec::new();
        let mut by_layer = vec![0.0; kinds.len()];
        let (mut coal, mut waste) = (0, 0);
        let (mut volume_in, mut volume_off, mut thin) = (0.0, 0.0, 0.0);
        let (mut flapped, mut flaps) = (0, 0.0);
        let (mut triangles_in, mut triangles_out, mut slab_triangles) = (0, 0, 0);
        let (mut worst, mut worst_of) = (0.0_f64, String::new());
        for ((object, solid), result) in solids.iter().zip(cuts) {
            let (cut, made) = result?;
            let mut made = made.into_iter();
            triangles_in += solid.mesh().triangle_count();
            slab_triangles += cut.slab_triangles;
            volume_in += cut.input_volume;
            thin += cut.thin;
            flapped += cut.flapped;
            flaps += cut.flaps;
            let share = cut.difference().abs() / cut.input_volume.abs().max(f64::MIN_POSITIVE);
            if share > worst {
                worst = share;
                worst_of = object.name.clone();
            }
            if share > ADDS_UP {
                let text = format!("'{}': its pieces, what's too thin, its flaps and any sliver past the grids' edge come to {:+.3} m³ against its {:.3} m³", object.name, cut.difference(), cut.input_volume);
                notes.push(Note::warning(Code::SeamCutDiffers, text).on(object.id).measured(cut.difference(), "m3"));
            }
            for piece in &cut.pieces {
                let solid = made.next().expect("a solid for each piece")?;
                let kind = &kinds[piece.layer];
                let mut attributes = object.attributes.clone();
                attributes.insert("horizon".into(), AttrValue::Text(kind.horizon.clone()));
                attributes.insert("material".into(), AttrValue::Text(kind.material.clone()));
                match &kind.seam {
                    Some(seam) => attributes.insert("seam".into(), AttrValue::Text(seam.clone())),
                    None => attributes.remove("seam"),
                };
                if kind.material == "waste" { waste += 1 } else { coal += 1 }
                by_layer[piece.layer] += piece.volume;
                triangles_out += piece.mesh.triangle_count();
                let name = block_name(&p.name, &attributes, &object.name, &kind.horizon);
                changes.push(Change::Create { name, layer: p.layer.clone(), attributes, body: Body::Solid(solid) });
            }
            // The check found nothing past the edge wider than a sliver: what's there is said, in no piece.
            volume_off += cut.outside.as_ref().map_or(0.0, |rest| rest.volume);
        }
        if coal + waste == 0 {
            return Err(CommandError::Invalid(format!("none of the {} solids lies on the grids of '{}': nothing cut", solids.len(), model_object.name)));
        }
        if volume_off > NOTHING {
            notes.push(Note::info(format!("{volume_off:.3} m³ in slivers under 10 mm wide past the edge of the grids is in no piece")).measured(volume_off, "m3"));
        }
        if thin > NOTHING {
            let text = format!("{thin:.3} m³ left out where a layer is thinner than {} m", p.min_thickness);
            notes.push(Note::new(Severity::Info, Some(Code::SeamThinLeftOut), text).measured(thin, "m3"));
        }
        if flaps > NOTHING {
            let text = format!("{flaps:.3} m³ in flaps thinner than {} m taken off {flapped} pieces, left out", p.flap_thickness);
            notes.push(Note::new(Severity::Info, Some(Code::SeamFlapsLeftOut), text).measured(flaps, "m3"));
        }
        let layers: Vec<String> = kinds.iter().zip(&by_layer).filter(|(_, v)| **v > 0.0).map(|(k, v)| format!("{} {v:.3}", k.horizon)).collect();
        let mut outcome = Outcome::new(changes)
            .message(format!(
                "{} solids, every one on the grids with no hole under it, cut by the {} seams of '{}' into {} pieces ({coal} coal, {waste} waste), in {ms:.0} ms ({read_ms:.0} ms reading the grids): {triangles_in} triangles in, {slab_triangles} in the layers, {triangles_out} out",
                solids.len(),
                working.len(),
                model_object.name,
                coal + waste
            ))
            .message(format!("Volumes (m³): {volume_in:.3} in; {}; {thin:.3} thinner than {} m; {flaps:.3} in flaps", layers.join(", "), p.min_thickness))
            .message(if worst_of.is_empty() {
                "Pieces add back to every solid exactly".to_string()
            } else {
                format!("Pieces add back to each solid within {worst:.1e} of its volume, the most for '{worst_of}'")
            });
        outcome.notes.extend(notes);
        Ok(outcome)
    }
}

/// A volume (m³) this small is nothing.
const NOTHING: f64 = 1e-6;

#[cfg(test)]
mod tests {
    use super::*;

    fn attributes(pairs: &[(&str, AttrValue)]) -> Attributes {
        let mut a = Attributes::new();
        for (k, v) in pairs {
            a.insert(k.to_string(), v.clone());
        }
        a
    }

    #[test]
    fn names_come_from_the_pattern_and_leave_out_what_a_block_hasnt() {
        let all = attributes(&[("strip", AttrValue::Number(5.0)), ("block", AttrValue::Text("B07".into())), ("seam", AttrValue::Text("U".into()))]);
        assert_eq!(block_name("{strip}_{block}_{seam}", &all, "s", "p"), "5_B07_U");
        let no_seam = attributes(&[("strip", AttrValue::Number(5.0)), ("block", AttrValue::Text("B07".into()))]);
        assert_eq!(block_name("{strip}_{block}_{seam}", &no_seam, "s", "p"), "5_B07");
        let no_strip = attributes(&[("block", AttrValue::Text("B07".into())), ("seam", AttrValue::Text("U".into()))]);
        assert_eq!(block_name("{strip}_{block}_{seam}", &no_strip, "s", "p"), "B07_U");
        assert_eq!(block_name("S{strip} {polygon} of {solid}", &all, "Pit strip 5", "Block 7"), "S5 Block 7 of Pit strip 5");
        assert_eq!(block_name("{nothing}", &all, "s", "p"), "s p");
        assert_eq!(word(&AttrValue::Number(2.5)), "2.5");
    }
}
