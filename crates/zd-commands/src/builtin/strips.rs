//! One face a strip, and strip solids (decision 0049): `face.join` makes one face for each strip number from the faces
//! given, and `solid.slice_by_faces` slices solids by a strip's faces into a solid a strip.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, Object, ObjectId, ObjectKind, Solid, Surface, TriMesh};
use zd_geom::{COINCIDENT, JoinLog, SliceRefusal, StripSlice, find_crossings, find_solid_crossings, join_faces, place, short_words, slice_solid_by_faces_watched, solid_between, triangulate_grid};

use super::solid::checked_input;
use super::surface::{default_tolerance, site};
use super::face::crossing_note;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Place, Severity};

fn default_attribute() -> String {
    "strip".into()
}

fn default_join_tolerance() -> f64 {
    0.001
}

/// A strip number as a name shows it: as it is, with no trailing zeros.
fn number(n: f64) -> String {
    if n.fract() == 0.0 { format!("{n:.0}") } else { format!("{n}") }
}

/// What tells a strip's faces from the rest: its number, or a name where the attribute holds one, as a build's does.
/// Numbers come first, lowest first, then names.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Key {
    Number(f64),
    Name(String),
}

impl Key {
    fn label(&self) -> String {
        match self {
            Key::Number(n) => number(*n),
            Key::Name(name) => name.clone(),
        }
    }

    fn value(&self) -> AttrValue {
        match self {
            Key::Number(n) => AttrValue::Number(*n),
            Key::Name(name) => AttrValue::Text(name.clone()),
        }
    }
}

/// The strip after the last, going on as the faces' numbers go from the pit out: the last two's step (6 after 4 and 5,
/// 8 after 4 and 6, 2 after 4 and 3), or one more after a single face. None where they're named by text or two share a
/// number.
fn next_strip(order: &[&Key]) -> Option<f64> {
    let numbered = |k: &Key| match k {
        Key::Number(n) => Some(*n),
        Key::Name(_) => None,
    };
    let last = numbered(order.last()?)?;
    let step = match order.len() {
        1 => 1.0,
        n => last - numbered(order[n - 2])?,
    };
    (step != 0.0).then_some(last + step)
}

/// The faces of one strip.
struct StripFaces<'a> {
    key: Key,
    faces: Vec<(&'a Arc<Object>, &'a TriMesh)>,
}

impl StripFaces<'_> {
    fn names(&self) -> String {
        self.faces.iter().map(|(object, _)| format!("'{}'", object.name)).collect::<Vec<_>>().join(", ")
    }

    /// The strip as a run names it: "strip 5", or a build's name.
    fn called(&self, attribute: &str) -> String {
        match &self.key {
            Key::Number(n) => format!("{attribute} {}", number(*n)),
            Key::Name(name) => name.clone(),
        }
    }
}

/// The faces given, grouped by their strip number, lowest first, or by name where the attribute holds text that isn't a
/// number. A face with nothing in the attribute is refused, saying which.
fn by_strip<'a>(inputs: &Inputs<'a>, ids: &[ObjectId], attribute: &str) -> Result<Vec<StripFaces<'a>>, CommandError> {
    if ids.is_empty() {
        return Err(CommandError::Invalid("give the faces".into()));
    }
    let mut groups: BTreeMap<(u8, i64, String), StripFaces<'a>> = BTreeMap::new();
    let mut seen = Vec::new();
    for &id in ids {
        if seen.contains(&id) {
            return Err(CommandError::Invalid(format!("face {id} is given twice")));
        }
        seen.push(id);
        let (object, surface) = inputs.surface(id)?;
        let key = match object.attributes.get(attribute) {
            Some(AttrValue::Number(n)) if n.is_finite() => Some(Key::Number(*n)),
            Some(AttrValue::Text(text)) if !text.trim().is_empty() => Some(match text.trim().parse::<f64>() {
                Ok(n) if n.is_finite() => Key::Number(n),
                _ => Key::Name(text.trim().to_string()),
            }),
            _ => None,
        };
        let Some(key) = key else {
            return Err(CommandError::Invalid(format!(
                "the face '{}' has no {attribute} number: number its line with a line drawn across the strips, and build it again",
                object.name
            )));
        };
        // Numbers grouped to a millionth, so a number read as text and one set as a number go together.
        let order = match &key {
            Key::Number(n) => (0, (n * 1e6).round() as i64, String::new()),
            Key::Name(name) => (1, 0, name.clone()),
        };
        groups.entry(order).or_insert_with(|| StripFaces { key, faces: Vec::new() }).faces.push((object, surface.mesh()));
    }
    Ok(groups.into_values().collect())
}

/// One strip's faces as one face, wound upward, with what joining them did. A strip of one face goes through the
/// join too, so a gap in it, as where a wall had no height, is filled and said.
fn one_face(strip: &StripFaces<'_>, tolerance: f64, attribute: &str) -> Result<(TriMesh, JoinLog), CommandError> {
    let meshes: Vec<&TriMesh> = strip.faces.iter().map(|(_, mesh)| *mesh).collect();
    // A face on its own welds only corners exactly on each other: there's nothing for it to meet.
    let tolerance = if meshes.len() == 1 { 0.0 } else { tolerance };
    join_faces(&meshes, tolerance).map_err(|e| CommandError::Invalid(format!("{} ({}): {e}", strip.called(attribute), strip.names())))
}

/// Whether a join changed anything but the faces' order.
fn changed(log: &JoinLog) -> bool {
    let weld = &log.weld;
    log.faces > 1 || weld.vertices_welded + weld.collapsed_dropped + weld.degenerate_dropped + weld.duplicates_dropped + log.stitched > 0 || !log.filled.is_empty()
}

/// What joining one strip's faces did, in a line, then each gap it filled, said where.
fn join_words(attribute: &str, strip: &StripFaces<'_>, log: &JoinLog, triangles: usize) -> Vec<Note> {
    let mut words = match log.faces {
        1 => format!("{}: one face ({}) of {triangles} triangles", strip.called(attribute), strip.names()),
        n => format!("{}: {n} faces joined ({}) into one of {triangles} triangles", strip.called(attribute), strip.names()),
    };
    let weld = &log.weld;
    if weld.vertices_welded > 0 && weld.largest_move > 0.0 {
        words.push_str(&format!("; {} shared corners welded within {:.0} mm, the most moved {:.2} mm", weld.vertices_welded, weld.tolerance * 1e3, weld.largest_move * 1e3));
    } else if weld.vertices_welded > 0 {
        words.push_str(&format!("; {} shared corners made one", weld.vertices_welded));
    }
    let dropped = weld.collapsed_dropped + weld.degenerate_dropped + weld.duplicates_dropped;
    if dropped > 0 {
        words.push_str(&format!("; {dropped} triangles dropped ({} collapsed by the weld, {} with no area, {} twice over)", weld.collapsed_dropped, weld.degenerate_dropped, weld.duplicates_dropped));
    }
    match log.stitched {
        0 => {}
        1 => words.push_str("; 1 corner lying on an edge, stitched to it"),
        n => words.push_str(&format!("; {n} corners lying on an edge, stitched to it")),
    }
    let mut notes = vec![Note::info(words)];
    for gap in &log.filled {
        let area = if gap.area < 0.01 { "under 0.01 m²".to_string() } else { format!("{:.2} m²", gap.area) };
        let text = format!("{}: a gap of {area} filled near {}", strip.called(attribute), place(gap.at));
        notes.push(Note::warning(Code::FaceGapFilled, text).at(Place::Point { at: gap.at }).measured(gap.area, "m2"));
    }
    notes
}

/// `face.join`: one face for each strip number, from the faces given (decision 0049). A strip's faces, up and down or
/// from several runs, are welded where they meet, stitched where they share a line but not its corners, and any gap
/// they close all round is filled, every change said. A strip of one face goes through the same, so a gap in it is filled. The faces given stay.
pub struct JoinFaces;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinFacesParams {
    /// The faces, each with a strip number.
    pub faces: Vec<ObjectId>,
    /// Layer for the joined faces.
    pub layer: String,
    /// How close corners weld (m), 1 mm by default.
    #[serde(default = "default_join_tolerance")]
    pub tolerance: f64,
    /// The attribute that numbers the strips, `strip` by default.
    #[serde(default = "default_attribute")]
    pub attribute: String,
}

impl Command for JoinFaces {
    type Params = JoinFacesParams;
    const NAME: &'static str = "face.join";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "One face for each strip number: a strip's faces, up and down or from several runs, joined into one surface",
            inputs: vec![Field::new("faces", FieldType::Objects, "The faces, each with a strip number")],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the joined faces"),
                Field::new("tolerance", FieldType::Number { unit: "m" }, "How close corners weld, 1 mm by default"),
                Field::new("attribute", FieldType::Text, "The attribute numbering the strips, strip by default"),
            ],
            outputs: vec![Field::new("faces", FieldType::Objects, "One face a strip, lowest number first")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let strips = by_strip(inputs, &p.faces, &p.attribute)?;
        let start = Instant::now();
        let mut outcome = Outcome::default();
        let mut joined = 0;
        for strip in &strips {
            let (mesh, log) = one_face(strip, p.tolerance, &p.attribute)?;
            let crossings = find_crossings(&mesh, COINCIDENT);
            if log.faces > 1 {
                joined += 1;
            }
            // A strip's notes are on the face this run makes for it.
            let face = outcome.changes.len();
            outcome.notes.extend(join_words(&p.attribute, strip, &log, mesh.triangle_count()).into_iter().map(|note| note.on_made(face)));
            if let Some(first) = crossings.at.first() {
                let text = format!("warning: {}: {} pairs of its triangles cross, the first at {}; it can't slice a solid until they're put right", strip.called(&p.attribute), crossings.pairs, site(first));
                outcome.notes.push(crossing_note(text, &crossings).on_made(face));
            }
            let mut attributes = Attributes::new();
            attributes.insert(p.attribute.clone(), strip.key.value());
            outcome.changes.push(Change::Create {
                name: format!("{} face", strip.called(&p.attribute)),
                layer: p.layer.clone(),
                attributes,
                body: Body::Surface(Surface::new(mesh).with_crossings(crossings)),
            });
        }
        let made = if strips.len() == 1 { "1 face".to_string() } else { format!("{} faces, one a {}", strips.len(), p.attribute) };
        outcome.notes.push(Note::info(format!("{} faces into {made}, {joined} of them joined, in {:.0} ms", p.faces.len(), start.elapsed().as_secs_f64() * 1e3)));
        Ok(outcome)
    }
}

/// `solid.slice_by_faces`: solids sliced into strips by faces (decision 0049; engine rule 5). The faces are grouped by
/// strip number and each strip's joined into one; each face is closed into the half-space behind it, past the solid,
/// and taken from the pit out by how much of the solid lies in front of it. Strip n is the solid in front of face n
/// that the strips before it left, so the strips and what's behind the last face add up to the solid. The solids and
/// faces stay as they are.
pub struct SliceByFaces;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SliceByFacesParams {
    /// One solid to slice, or give `solids`.
    #[serde(default)]
    pub solid: Option<ObjectId>,
    /// Solids to slice, each on its own.
    #[serde(default)]
    pub solids: Vec<ObjectId>,
    /// The faces, each with a strip number.
    pub faces: Vec<ObjectId>,
    /// Layer for the strips.
    pub layer: String,
    /// How close corners weld where a strip's faces are joined (m), 1 mm by default.
    #[serde(default = "default_join_tolerance")]
    pub tolerance: f64,
    /// The attribute that numbers the strips, `strip` by default.
    #[serde(default = "default_attribute")]
    pub attribute: String,
}

/// More strips than this and a run sums them up instead of a line each.
const LINES_PER_STRIP_UP_TO: usize = 30;

impl Command for SliceByFaces {
    type Params = SliceByFacesParams;
    const NAME: &'static str = "solid.slice_by_faces";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Slice solids into strips by faces: a solid a strip, in front of its face, adding up to the solid",
            inputs: vec![
                Field::object("solid", ObjectKind::Solid, "The solid to slice (or give solids)"),
                Field::new("solids", FieldType::Objects, "Solids to slice, each on its own"),
                Field::new("faces", FieldType::Objects, "The faces, each with a strip number"),
            ],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the strips"),
                Field::new("tolerance", FieldType::Number { unit: "m" }, "How close corners weld where a strip's faces join, 1 mm by default"),
                Field::new("attribute", FieldType::Text, "The attribute numbering the strips, strip by default"),
            ],
            outputs: vec![Field::new("strips", FieldType::Objects, "Each solid's strips from the pit out, then what's behind the last face")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let ids: Vec<ObjectId> = p.solid.into_iter().chain(p.solids.iter().copied()).collect();
        if ids.is_empty() {
            return Err(CommandError::Invalid("give the solid (or solids) to slice".into()));
        }
        if let Some(twice) = ids.iter().enumerate().find_map(|(i, id)| ids[..i].contains(id).then_some(id)) {
            return Err(CommandError::Invalid(format!("solid {twice} is given twice")));
        }
        let mut outcome = Outcome::default();
        let solids = ids.iter().map(|&id| inputs.solid(id)).collect::<Result<Vec<_>, _>>()?;
        for (object, solid) in &solids {
            checked_input("solid", &object.name, solid.crossings(), solid.mesh(), true, &mut outcome.notes)?;
        }
        let strips = by_strip(inputs, &p.faces, &p.attribute)?;
        for strip in &strips {
            for (object, _) in &strip.faces {
                let (_, surface) = inputs.surface(object.id)?;
                checked_input("face", &object.name, surface.crossings(), surface.mesh(), false, &mut outcome.notes)?;
            }
        }
        let mut faces = Vec::with_capacity(strips.len());
        for strip in &strips {
            let (mesh, log) = one_face(strip, p.tolerance, &p.attribute)?;
            if changed(&log) {
                outcome.notes.extend(join_words(&p.attribute, strip, &log, mesh.triangle_count()));
            }
            faces.push(mesh);
        }
        let face_refs: Vec<&TriMesh> = faces.iter().collect();
        let face_name = |k: usize| format!("{} face", strips[k].called(&p.attribute));

        let of = solids.len() * face_refs.len();
        for (k, (object, solid)) in solids.iter().enumerate() {
            let start = Instant::now();
            // Strips counted over every solid, and stopped between one strip and the next when cancelled.
            let watched = slice_solid_by_faces_watched(solid.mesh(), &face_refs, |done, _| {
                inputs.progress(k * face_refs.len() + done, of, "strips");
                inputs.stop_if_cancelled().is_ok()
            });
            let slice = watched.map_err(|refusal| match refusal {
                SliceRefusal::Stopped => CommandError::Cancelled,
                SliceRefusal::Solid(e) => CommandError::Invalid(format!("'{}': {e}", object.name)),
                SliceRefusal::Face { face, error } => CommandError::Invalid(format!("{} ({}): {error}", face_name(face), strips[face].names())),
                SliceRefusal::Short { face, end } => CommandError::Invalid(format!("{} ({}): {}", face_name(face), strips[face].names(), short_words(&end))),
            })?;
            let ms = start.elapsed().as_secs_f64() * 1e3;
            self.report(&mut outcome, object, solid.mesh(), &slice, &strips, &face_refs, ms, p)?;
        }
        Ok(outcome)
    }
}

impl SliceByFaces {
    /// One solid's strips as new solids, and what the slice did in words.
    #[allow(clippy::too_many_arguments)]
    fn report(&self, outcome: &mut Outcome, object: &Object, mesh: &TriMesh, slice: &StripSlice, strips: &[StripFaces<'_>], faces: &[&TriMesh], ms: f64, p: &SliceByFacesParams) -> Result<(), CommandError> {
        let attribute = &p.attribute;
        let name = &object.name;
        let tolerance = (1e-6 * slice.input_volume.abs()).max(1e-3);
        let numbers: Vec<String> = slice.order.iter().map(|&k| strips[k].key.label()).collect();
        let t = slice.timings;
        let triangles_in: usize = faces.iter().map(|f| f.triangle_count()).sum();
        let triangles_out: usize = slice.strips.iter().map(|s| s.mesh.triangle_count()).sum();
        let mut lines = vec![Note::info(format!(
            "'{name}' sliced by {} faces in {ms:.0} ms (closing them {:.0}, strips {:.0}, from Manifold {:.0}): {} + {triangles_in} triangles in, {triangles_out} out",
            faces.len(),
            t.close_ms,
            t.strips_ms,
            t.from_manifold_ms,
            mesh.triangle_count()
        ))];
        let ascending = slice.order.windows(2).all(|w| strips[w[0]].key < strips[w[1]].key);
        let descending = slice.order.windows(2).all(|w| strips[w[0]].key > strips[w[1]].key);
        lines.push(Note::info(format!(
            "faces from the pit out, by how much of '{name}' lies in front of each: {attribute} {}{}",
            numbers.join(", "),
            if ascending || faces.len() == 1 {
                ""
            } else if descending {
                " (numbered from the far side in)"
            } else {
                " (not in the order of their numbers)"
            }
        )));

        // What's behind the last face is numbered as the next strip, going on as the faces' numbers go, so it's named and
        // carried like the rest, to delete or keep. Strips named by text, or a next number one of the faces already has,
        // leave it `past` the last.
        let next = next_strip(slice.order.iter().map(|&k| &strips[k].key).collect::<Vec<_>>().as_slice());
        let next = next.filter(|n| !strips.iter().any(|s| s.key == Key::Number(*n)));
        let few = slice.strips.len() <= LINES_PER_STRIP_UP_TO;
        let mut kept = 0;
        for (k, piece) in slice.strips.iter().enumerate() {
            let last = k == slice.order.len();
            let (piece_name, which) = match (last, next) {
                (false, _) => (format!("{name} {attribute} {}", numbers[k]), format!("{attribute} {}", numbers[k])),
                (true, Some(n)) => (format!("{name} {attribute} {}", number(n)), format!("{attribute} {} (behind the last face)", number(n))),
                (true, None) => (format!("{name} past {attribute} {}", numbers[k - 1]), format!("past {attribute} {}", numbers[k - 1])),
            };
            if piece.mesh.triangles.is_empty() || piece.volume <= tolerance {
                lines.push(Note::warning(Code::SliceEmpty, format!("{which}: nothing of '{name}' lies there")).on(object.id));
                continue;
            }
            let crossings = find_solid_crossings(&piece.mesh, default_tolerance());
            if let Some(first) = crossings.at.first() {
                // Where the faces either side of a strip meet, it has no thickness, and its solid touches itself there.
                let sides: Vec<usize> = [k.checked_sub(1), (!last).then_some(k)].into_iter().flatten().map(|j| slice.order[j]).collect();
                let meet = match sides.as_slice() {
                    [a, b] if heights(faces[*a], *first).iter().any(|za| heights(faces[*b], *first).iter().any(|zb| (za - zb).abs() < 0.01)) => format!(
                        ": the {} and {} faces meet there, so {which} has no thickness; move the designs apart there",
                        strips[*a].called(attribute),
                        strips[*b].called(attribute)
                    ),
                    _ => String::new(),
                };
                return Err(CommandError::Invalid(format!("'{name}': {which} would cross itself, {} pairs, the first at {}{meet}; nothing saved", crossings.pairs, site(first))));
            }
            let solid = Solid::new(piece.mesh.clone()).map_err(CommandError::NotWatertight)?.with_crossings(crossings);
            if few {
                let parts = if piece.parts > 1 { format!(" in {} parts", piece.parts) } else { String::new() };
                lines.push(Note::info(format!("{which}: {:.3} m3{parts}", piece.volume)).on_made(outcome.changes.len()).measured(piece.volume, "m3"));
            }
            let mut attributes = object.attributes.clone();
            match (last, next) {
                (false, _) => {
                    attributes.insert(attribute.clone(), strips[slice.order[k]].key.value());
                }
                (true, Some(n)) => {
                    attributes.insert(attribute.clone(), AttrValue::Number(n));
                }
                (true, None) => {
                    attributes.remove(attribute);
                    attributes.insert(format!("past_{attribute}"), strips[slice.order[k - 1]].key.value());
                }
            }
            kept += 1;
            outcome.changes.push(Change::Create { name: piece_name, layer: p.layer.clone(), attributes, body: Body::Solid(solid) });
        }
        let total: f64 = slice.strips.iter().map(|s| s.volume).sum();
        let mut tiny = zd_geom::TinyEdges::default();
        slice.strips.iter().for_each(|s| tiny.add(s.tiny_edges));
        if let Some(line) = super::solid::tiny_edge_words(tiny, slice.strips.iter().filter(|s| s.tiny_edges.joined > 0).count()) {
            lines.push(Note::info(line));
        }
        let parted = slice.strips.iter().filter(|s| s.parts > 1).count();
        lines.push(Note::info(format!(
            "'{name}' {:.3} m3 into {kept} solids adding up to {total:.3} m3, within {:.6} m3{}",
            slice.input_volume,
            (total - slice.input_volume).abs(),
            if parted > 0 { format!("; {parted} in more than one part, each kept as one solid") } else { String::new() }
        )).on(object.id).measured(total, "m3"));
        for (k, cut) in slice.faces.iter().enumerate() {
            let face = format!("{} face", strips[k].called(attribute));
            if let Some((by, at)) = cut.toe_above_floor {
                let text = format!("{face}: its toe stands up to {by:.2} m above the floor of '{name}', near {}, and below it the cut goes straight down", place(at));
                lines.push(Note::warning(Code::SliceToeAboveFloor, text).at(Place::Point { at }).measured(by, "m"));
            }
            if let Some((by, at)) = cut.top_below_top {
                let text = format!("{face}: its top is up to {by:.2} m below the top of '{name}', near {}, and above it the cut goes straight up", place(at));
                lines.push(Note::warning(Code::SliceTopBelowTop, text).at(Place::Point { at }).measured(by, "m"));
            }
        }
        for crossing in &slice.crossings {
            let text = format!(
                "warning: the {} face crosses faces before it inside '{name}': {:.3} m3 in front of them lies behind it, near {}, and stays with the strips before",
                strips[crossing.face].called(attribute),
                crossing.volume,
                place(crossing.at)
            );
            lines.push(Note::warning(Code::SliceFacesCross, text).at(Place::Point { at: crossing.at }).measured(crossing.volume, "m3"));
        }
        outcome.notes.extend(lines);
        Ok(())
    }
}

/// `solid.between_surfaces`: the solid inside a closed string, below one surface or grid and above another, with
/// vertical sides on the string (decision 0049): the large solid a strip's faces slice, such as the topography down to
/// the lowest floor inside the pit's limit. Where the bottom is above the top there's nothing; where either doesn't
/// cover the string in plan there's nothing either, and the area is said.
pub struct SolidBetween;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolidBetweenParams {
    /// The surface or grid on top.
    pub top: ObjectId,
    /// The surface or grid underneath.
    pub bottom: ObjectId,
    /// The closed string around it.
    pub string: ObjectId,
    /// Layer for the solid.
    pub layer: String,
    /// Default "<string> <top> to <bottom>".
    #[serde(default)]
    pub name: Option<String>,
}

impl Command for SolidBetween {
    type Params = SolidBetweenParams;
    const NAME: &'static str = "solid.between_surfaces";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "A solid inside a closed string, between a surface or grid on top and one underneath",
            inputs: vec![
                Field::new("top", FieldType::Objects, "The surface or grid on top"),
                Field::new("bottom", FieldType::Objects, "The surface or grid underneath"),
                Field::object("string", ObjectKind::String, "The closed string around it"),
            ],
            params: vec![Field::new("layer", FieldType::Layer, "Layer for the solid"), Field::new("name", FieldType::Text, "Optional name for the solid")],
            outputs: vec![Field::object("solid", ObjectKind::Solid, "The solid")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let mut outcome = Outcome::default();
        let mut mesh_of = |id: ObjectId, what: &str| -> Result<(String, std::borrow::Cow<'_, TriMesh>), CommandError> {
            let object = inputs.object(id)?;
            match &object.body {
                Body::Surface(surface) => {
                    checked_input(what, &object.name, surface.crossings(), surface.mesh(), false, &mut outcome.notes)?;
                    Ok((object.name.clone(), std::borrow::Cow::Borrowed(surface.mesh())))
                }
                Body::Grid(grid) => Ok((object.name.clone(), std::borrow::Cow::Owned(triangulate_grid(grid)))),
                other => Err(CommandError::WrongKind { id, expected: ObjectKind::Surface, found: other.kind() }),
            }
        };
        let (top_name, top) = mesh_of(p.top, "top")?;
        let (bottom_name, bottom) = mesh_of(p.bottom, "bottom")?;
        let (string, line) = inputs.string(p.string)?;
        let outline: Vec<[f64; 2]> = line.vertices.iter().map(|v| [v[0], v[1]]).collect();
        let between = solid_between(&top, &bottom, &outline).map_err(|e| CommandError::Invalid(format!("'{}' between '{top_name}' and '{bottom_name}': {e}", string.name)))?;
        if !line.closed {
            outcome.notes.push(Note::new(Severity::Info, Some(Code::SolidOutlineOpen), format!("'{}' isn't closed: it's taken as closed from its last point back to its first", string.name)).on(string.id));
        }
        if between.mesh.triangles.is_empty() {
            return Err(CommandError::Invalid(format!("nothing lies between '{top_name}' and '{bottom_name}' inside '{}'", string.name)));
        }
        let crossings = find_solid_crossings(&between.mesh, default_tolerance());
        if let Some(first) = crossings.at.first() {
            return Err(CommandError::Invalid(format!("the solid would cross itself, {} pairs, the first at {}; nothing saved", crossings.pairs, site(first))));
        }
        outcome.notes.push(Note::info(format!(
            "'{}': {:.3} m3 between '{top_name}' and '{bottom_name}' over {:.1} m2, in {:.0} ms: {} + {} triangles in, {} out{}",
            string.name,
            between.volume,
            between.area,
            between.ms,
            top.triangle_count(),
            bottom.triangle_count(),
            between.mesh.triangle_count(),
            if between.parts > 1 { format!(", in {} parts", between.parts) } else { String::new() }
        )).measured(between.volume, "m3"));
        if let Some(line) = super::solid::tiny_edge_words(between.tiny_edges, 1) {
            outcome.notes.push(Note::info(line));
        }
        for (name, uncovered) in [(&top_name, between.top_uncovered), (&bottom_name, between.bottom_uncovered)] {
            if uncovered > 0.01 {
                let text = format!("warning: '{name}' doesn't cover {uncovered:.1} m2 of '{}' in plan, and there's no solid there", string.name);
                outcome.notes.push(Note::warning(Code::SolidNotCovered, text).measured(uncovered, "m2").on(string.id));
            }
        }
        let solid = Solid::new(between.mesh).map_err(CommandError::NotWatertight)?.with_crossings(crossings);
        let name = p.name.clone().unwrap_or_else(|| format!("{} {top_name} to {bottom_name}", string.name));
        outcome.changes.push(Change::Create { name, layer: p.layer.clone(), attributes: Attributes::new(), body: Body::Solid(solid) });
        Ok(outcome)
    }
}

/// The heights of a face at a place in plan, one for each triangle over it.
fn heights(face: &TriMesh, at: [f64; 3]) -> Vec<f64> {
    let (x, y) = (at[0], at[1]);
    let mut found = Vec::new();
    for t in &face.triangles {
        let [a, b, c] = t.map(|i| face.vertices[i as usize]);
        let d = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if d.abs() < 1e-12 {
            continue;
        }
        let l1 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / d;
        let l2 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / d;
        let l3 = 1.0 - l1 - l2;
        if l1 >= -1e-9 && l2 >= -1e-9 && l3 >= -1e-9 {
            found.push(l1 * a[2] + l2 * b[2] + l3 * c[2]);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strip_after_the_last_goes_on_as_the_numbers_go() {
        let n = |v: &[f64]| next_strip(&v.iter().map(|&x| Key::Number(x)).collect::<Vec<_>>().iter().collect::<Vec<_>>());
        assert_eq!(n(&[1.0, 2.0, 3.0]), Some(4.0));
        assert_eq!(n(&[20.0, 22.0, 24.0]), Some(26.0), "every other");
        assert_eq!(n(&[6.0, 5.0, 4.0]), Some(3.0), "numbered from the far side in");
        assert_eq!(n(&[7.0]), Some(8.0), "one face");
        assert_eq!(n(&[3.0, 3.0]), None);
        let named = [Key::Number(1.0), Key::Name("A".into())];
        assert_eq!(next_strip(&named.iter().collect::<Vec<_>>()), None, "named by text");
    }
}