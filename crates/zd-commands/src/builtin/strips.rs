//! One face a strip, and strip solids (decision 0049): `face.join` makes one face for each strip number from the faces
//! given, and `solid.slice_by_faces` slices solids by a strip's faces into a solid a strip.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, Object, ObjectId, ObjectKind, Solid, Surface, TriMesh};
use zd_geom::{COINCIDENT, JoinLog, SliceRefusal, StripSlice, find_crossings, find_solid_crossings, join_faces, orient_upward, place, short_words, slice_solid_by_faces, solid_between, triangulate_grid};

use super::solid::checked_input;
use super::surface::{default_tolerance, site};
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

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

/// The faces of one strip.
struct StripFaces<'a> {
    number: f64,
    faces: Vec<(&'a Arc<Object>, &'a TriMesh)>,
}

impl StripFaces<'_> {
    fn names(&self) -> String {
        self.faces.iter().map(|(object, _)| format!("'{}'", object.name)).collect::<Vec<_>>().join(", ")
    }
}

/// The faces given, grouped by their strip number, lowest first. A face with no number, or one that isn't a number, is
/// refused, saying which.
fn by_strip<'a>(inputs: &Inputs<'a>, ids: &[ObjectId], attribute: &str) -> Result<Vec<StripFaces<'a>>, CommandError> {
    if ids.is_empty() {
        return Err(CommandError::Invalid("give the faces".into()));
    }
    let mut groups: BTreeMap<i64, StripFaces<'a>> = BTreeMap::new();
    let mut seen = Vec::new();
    for &id in ids {
        if seen.contains(&id) {
            return Err(CommandError::Invalid(format!("face {id} is given twice")));
        }
        seen.push(id);
        let (object, surface) = inputs.surface(id)?;
        let value = match object.attributes.get(attribute) {
            Some(AttrValue::Number(n)) => Some(*n),
            Some(AttrValue::Text(text)) => text.trim().parse::<f64>().ok(),
            _ => None,
        };
        let Some(n) = value.filter(|n| n.is_finite()) else {
            return Err(CommandError::Invalid(format!(
                "the face '{}' has no {attribute} number: number its line with a line drawn across the strips, and build it again",
                object.name
            )));
        };
        // Grouped to a millionth, so a number read as text and one set as a number go together.
        groups.entry((n * 1e6).round() as i64).or_insert_with(|| StripFaces { number: n, faces: Vec::new() }).faces.push((object, surface.mesh()));
    }
    Ok(groups.into_values().collect())
}

/// One strip's faces as one face, wound upward, with what joining them did.
fn one_face(strip: &StripFaces<'_>, tolerance: f64, attribute: &str) -> Result<(TriMesh, Option<JoinLog>), CommandError> {
    let refuse = |e: zd_geom::GeomError| CommandError::Invalid(format!("{attribute} {}: {e}", number(strip.number)));
    match strip.faces.as_slice() {
        [(_, mesh)] => Ok((orient_upward(mesh).map_err(refuse)?.0, None)),
        faces => {
            let meshes: Vec<&TriMesh> = faces.iter().map(|(_, mesh)| *mesh).collect();
            let (mesh, log) = join_faces(&meshes, tolerance).map_err(refuse)?;
            Ok((mesh, Some(log)))
        }
    }
}

/// What joining one strip's faces did, in a line.
fn join_words(attribute: &str, strip: &StripFaces<'_>, log: &JoinLog, triangles: usize) -> String {
    let mut words = format!("{attribute} {}: {} faces joined ({}) into one of {triangles} triangles", number(strip.number), log.faces, strip.names());
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
    if log.stitched > 0 {
        words.push_str(&format!("; {} corners of one face on another's edge, stitched", log.stitched));
    }
    for gap in &log.filled {
        words.push_str(&format!("; a gap of {:.1} m² between them filled near {}", gap.area, place(gap.at)));
    }
    words
}

/// `face.join`: one face for each strip number, from the faces given (decision 0049). A strip's faces, up and down or
/// from several runs, are welded where they meet, stitched where they share a line but not its corners, and any gap
/// they close all round is filled, every change said. A strip with one face is copied as it is. The faces given stay.
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
            let line = match &log {
                Some(log) => {
                    joined += 1;
                    join_words(&p.attribute, strip, log, mesh.triangle_count())
                }
                None => format!("{} {}: one face ({}), copied", p.attribute, number(strip.number), strip.names()),
            };
            outcome = outcome.message(line);
            if let Some(first) = crossings.at.first() {
                outcome = outcome.message(format!("warning: {} {}: {} pairs of its triangles cross, the first at {}; it can't slice a solid until they're put right", p.attribute, number(strip.number), crossings.pairs, site(first)));
            }
            let mut attributes = Attributes::new();
            attributes.insert(p.attribute.clone(), AttrValue::Number(strip.number));
            outcome.changes.push(Change::Create {
                name: format!("{} {} face", p.attribute, number(strip.number)),
                layer: p.layer.clone(),
                attributes,
                body: Body::Surface(Surface::new(mesh).with_crossings(crossings)),
            });
        }
        Ok(outcome.message(format!("{} faces into {} faces, one a {}, {joined} of them joined, in {:.0} ms", p.faces.len(), strips.len(), p.attribute, start.elapsed().as_secs_f64() * 1e3)))
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
            checked_input("solid", &object.name, solid.crossings(), solid.mesh(), true, &mut outcome.messages)?;
        }
        let strips = by_strip(inputs, &p.faces, &p.attribute)?;
        for strip in &strips {
            for (object, _) in &strip.faces {
                let (_, surface) = inputs.surface(object.id)?;
                checked_input("face", &object.name, surface.crossings(), surface.mesh(), false, &mut outcome.messages)?;
            }
        }
        let mut faces = Vec::with_capacity(strips.len());
        for strip in &strips {
            let (mesh, log) = one_face(strip, p.tolerance, &p.attribute)?;
            if let Some(log) = log {
                outcome = outcome.message(join_words(&p.attribute, strip, &log, mesh.triangle_count()));
            }
            faces.push(mesh);
        }
        let face_refs: Vec<&TriMesh> = faces.iter().collect();
        let face_name = |k: usize| format!("{} {} face", p.attribute, number(strips[k].number));

        for (object, solid) in &solids {
            let start = Instant::now();
            let slice = slice_solid_by_faces(solid.mesh(), &face_refs).map_err(|refusal| {
                CommandError::Invalid(match refusal {
                    SliceRefusal::Solid(e) => format!("'{}': {e}", object.name),
                    SliceRefusal::Face { face, error } => format!("{} ({}): {error}", face_name(face), strips[face].names()),
                    SliceRefusal::Short { face, end } => format!("{} ({}): {}", face_name(face), strips[face].names(), short_words(&end)),
                })
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
        let numbers: Vec<String> = slice.order.iter().map(|&k| number(strips[k].number)).collect();
        let t = slice.timings;
        let triangles_in: usize = faces.iter().map(|f| f.triangle_count()).sum();
        let triangles_out: usize = slice.strips.iter().map(|s| s.mesh.triangle_count()).sum();
        let mut lines = vec![format!(
            "'{name}' sliced by {} faces in {ms:.0} ms (closing them {:.0}, strips {:.0}, from Manifold {:.0}): {} + {triangles_in} triangles in, {triangles_out} out",
            faces.len(),
            t.close_ms,
            t.strips_ms,
            t.from_manifold_ms,
            mesh.triangle_count()
        )];
        let ascending = slice.order.windows(2).all(|w| strips[w[0]].number < strips[w[1]].number);
        let descending = slice.order.windows(2).all(|w| strips[w[0]].number > strips[w[1]].number);
        lines.push(format!(
            "faces from the pit out, by how much of '{name}' lies in front of each: {attribute} {}{}",
            numbers.join(", "),
            if ascending || faces.len() == 1 {
                ""
            } else if descending {
                " (numbered from the far side in)"
            } else {
                " (not in the order of their numbers)"
            }
        ));

        let few = slice.strips.len() <= LINES_PER_STRIP_UP_TO;
        let mut kept = 0;
        for (k, piece) in slice.strips.iter().enumerate() {
            let last = k == slice.order.len();
            let (piece_name, which) = match last {
                false => (format!("{name} {attribute} {}", numbers[k]), format!("{attribute} {}", numbers[k])),
                true => (format!("{name} past {attribute} {}", numbers[k - 1]), format!("past {attribute} {}", numbers[k - 1])),
            };
            if piece.mesh.triangles.is_empty() || piece.volume <= tolerance {
                lines.push(format!("{which}: nothing of '{name}' lies there"));
                continue;
            }
            let crossings = find_solid_crossings(&piece.mesh, default_tolerance());
            if let Some(first) = crossings.at.first() {
                return Err(CommandError::Invalid(format!("'{name}': {which} would cross itself, {} pairs, the first at {}; nothing saved", crossings.pairs, site(first))));
            }
            let solid = Solid::new(piece.mesh.clone()).map_err(CommandError::NotWatertight)?.with_crossings(crossings);
            if few {
                let parts = if piece.parts > 1 { format!(" in {} parts", piece.parts) } else { String::new() };
                lines.push(format!("{which}: {:.3} m3{parts}", piece.volume));
            }
            let mut attributes = object.attributes.clone();
            match last {
                false => {
                    attributes.insert(attribute.clone(), AttrValue::Number(strips[slice.order[k]].number));
                }
                true => {
                    attributes.remove(attribute);
                    attributes.insert(format!("past_{attribute}"), AttrValue::Number(strips[slice.order[k - 1]].number));
                }
            }
            kept += 1;
            outcome.changes.push(Change::Create { name: piece_name, layer: p.layer.clone(), attributes, body: Body::Solid(solid) });
        }
        let total: f64 = slice.strips.iter().map(|s| s.volume).sum();
        let parted = slice.strips.iter().filter(|s| s.parts > 1).count();
        lines.push(format!(
            "'{name}' {:.3} m3 into {kept} solids adding up to {total:.3} m3, within {:.6} m3{}",
            slice.input_volume,
            (total - slice.input_volume).abs(),
            if parted > 0 { format!("; {parted} in more than one part, each kept as one solid") } else { String::new() }
        ));
        for (k, cut) in slice.faces.iter().enumerate() {
            let face = format!("{attribute} {} face", number(strips[k].number));
            if let Some((rl, volume)) = cut.below_lowest {
                lines.push(format!("{face}: {volume:.3} m3 of '{name}' lies below its lowest line at RL {rl:.1}, and the cut goes straight down there"));
            }
            if let Some((top, solid_top)) = cut.top_below {
                lines.push(format!("{face}: its top at RL {top:.1} is below the top of '{name}' at RL {solid_top:.1}, and above it the cut goes straight up"));
            }
        }
        for crossing in &slice.crossings {
            lines.push(format!(
                "warning: the {attribute} {} face crosses faces before it inside '{name}': {:.3} m3 in front of them lies behind it, near {}, and stays with the strips before",
                number(strips[crossing.face].number),
                crossing.volume,
                place(crossing.at)
            ));
        }
        outcome.messages.extend(lines);
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
                    checked_input(what, &object.name, surface.crossings(), surface.mesh(), false, &mut outcome.messages)?;
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
            outcome = outcome.message(format!("'{}' isn't closed: it's taken as closed from its last point back to its first", string.name));
        }
        if between.mesh.triangles.is_empty() {
            return Err(CommandError::Invalid(format!("nothing lies between '{top_name}' and '{bottom_name}' inside '{}'", string.name)));
        }
        let crossings = find_solid_crossings(&between.mesh, default_tolerance());
        if let Some(first) = crossings.at.first() {
            return Err(CommandError::Invalid(format!("the solid would cross itself, {} pairs, the first at {}; nothing saved", crossings.pairs, site(first))));
        }
        outcome = outcome.message(format!(
            "'{}': {:.3} m3 between '{top_name}' and '{bottom_name}' over {:.1} m2, in {:.0} ms: {} + {} triangles in, {} out{}",
            string.name,
            between.volume,
            between.area,
            between.ms,
            top.triangle_count(),
            bottom.triangle_count(),
            between.mesh.triangle_count(),
            if between.parts > 1 { format!(", in {} parts", between.parts) } else { String::new() }
        ));
        for (name, uncovered) in [(&top_name, between.top_uncovered), (&bottom_name, between.bottom_uncovered)] {
            if uncovered > 0.01 {
                outcome = outcome.message(format!("warning: '{name}' doesn't cover {uncovered:.1} m2 of '{}' in plan, and there's no solid there", string.name));
            }
        }
        let solid = Solid::new(between.mesh).map_err(CommandError::NotWatertight)?.with_crossings(crossings);
        let name = p.name.clone().unwrap_or_else(|| format!("{} {top_name} to {bottom_name}", string.name));
        outcome.changes.push(Change::Create { name, layer: p.layer.clone(), attributes: Attributes::new(), body: Body::Solid(solid) });
        Ok(outcome)
    }
}
