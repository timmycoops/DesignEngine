//! `object.trim` (Surface tools plan, workstream 3; decision 0095): anything picked, strings, surfaces, solids and
//! grids, cut by a closed string in plan, keeping the inside or the outside, in one run and one undo. Each kind by its own
//! rule (`zd_geom::trim_in_plan`). In place, each thing's next revision; run as new (decision 0091), a copy on a layer.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Body, Change, Object, ObjectId, ObjectKind, Solid, Surface};
use zd_geom::{GeomError, InPlan, find_solid_crossings, trim_grid_in_plan, trim_ring, trim_solid_in_plan, trim_string_in_plan, trim_surface_in_plan};

use super::ClipKeep;
use super::surface::default_tolerance as crossing_tolerance;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

pub struct TrimInPlan;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrimInPlanParams {
    /// The strings, surfaces, solids and grids to trim.
    pub objects: Vec<ObjectId>,
    /// The closed string they're trimmed by.
    pub by: ObjectId,
    #[serde(default)]
    pub keep: ClipKeep,
}

/// What became of one thing.
enum Done {
    Left,
    Dropped,
    Trimmed { changes: Vec<Change>, said: String },
}

fn so_many(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn names(of: &[&str]) -> String {
    let shown: Vec<String> = of.iter().take(10).map(|n| format!("'{n}'")).collect();
    if of.len() > 10 { format!("{} and {} more", shown.join(", "), of.len() - 10) } else { shown.join(", ") }
}

fn why(e: GeomError) -> String {
    match e {
        GeomError::InvalidInput(words) | GeomError::Refused(words) => words,
        other => other.to_string(),
    }
}

/// A number for a sentence with thousands marked, to `places` decimals.
pub(super) fn thousands(v: f64, places: usize) -> String {
    let text = format!("{:.*}", places, v.abs());
    let (whole, part) = text.split_once('.').map_or((text.as_str(), None), |(w, p)| (w, Some(p)));
    let mut grouped = String::new();
    for (k, c) in whole.chars().enumerate() {
        if k > 0 && (whole.len() - k).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    format!("{}{grouped}{}", if v < 0.0 { "-" } else { "" }, part.map_or(String::new(), |p| format!(".{p}")))
}

impl Command for TrimInPlan {
    type Params = TrimInPlanParams;
    const NAME: &'static str = "object.trim";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Trim strings, surfaces, solids and grids to a closed string in plan, keeping the inside or the outside",
            inputs: vec![
                Field::new("objects", FieldType::Objects, "The strings, surfaces, solids and grids to trim"),
                Field::object("by", ObjectKind::String, "The closed string they're trimmed by"),
            ],
            params: vec![Field::new("keep", FieldType::Text, "`inside` (the default) or `outside`")],
            outputs: vec![Field::new("parts", FieldType::Objects, "A string's parts kept as new strings, past its first")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.objects.is_empty() {
            return Err(CommandError::Invalid("nothing to trim: pick the strings, surfaces, solids or grids".into()));
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(twice) = p.objects.iter().find(|id| !seen.insert(**id)) {
            return Err(CommandError::Invalid(format!("object {twice} is given twice")));
        }
        let (limit_object, limit) = inputs.string(p.by)?;
        if p.objects.contains(&p.by) {
            return Err(CommandError::Invalid(format!("'{}' is the string trimmed by, so it can't be trimmed by itself; leave it out of those picked", limit_object.name)));
        }
        let ring = trim_ring(limit).map_err(|e| CommandError::Invalid(format!("'{}': {}; close it first, or pick a closed string", limit_object.name, why(e))))?;
        let inside = p.keep == ClipKeep::Inside;
        let side = if inside { "inside" } else { "outside" };
        let dropped_side = if inside { "outside" } else { "inside" };

        let mut objects = Vec::with_capacity(p.objects.len());
        for &id in &p.objects {
            objects.push(inputs.object(id)?);
        }
        let (mut changes, mut made, mut failed, mut left, mut gone, mut said) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for object in &objects {
            match trim_one(inputs, object, limit, &ring, inside) {
                Ok(Done::Left) => left.push(object.name.as_str()),
                Ok(Done::Dropped) => gone.push(object.name.as_str()),
                Ok(Done::Trimmed { changes: mut c, said: s }) => {
                    changes.push(c.remove(0));
                    made.extend(c);
                    said.push(format!("'{}' {s}", object.name));
                }
                Err(words) => failed.push(format!("'{}': {words}", object.name)),
            }
        }
        let of = objects.len();
        if !failed.is_empty() {
            return Err(CommandError::Invalid(format!("{} of {of} can't be trimmed, so nothing was changed. {}", failed.len(), failed.join("; "))));
        }
        if !gone.is_empty() {
            let lying = if gone.len() == 1 { format!("{} lies", names(&gone)) } else { format!("{} lie", names(&gone)) };
            return Err(CommandError::Invalid(format!("{lying} wholly {dropped_side} '{}', so nothing of {} would be left; leave {} out of those picked", limit_object.name, if gone.len() == 1 { "it" } else { "them" }, if gone.len() == 1 { "it" } else { "them" })));
        }
        if said.is_empty() {
            let lying = if left.len() == 1 { format!("'{}' lies", left[0]) } else { format!("all {of} lie") };
            return Err(CommandError::Invalid(format!("{lying} wholly {side} '{}' already, so there's nothing to trim", limit_object.name)));
        }
        let mut message = format!("trimmed {} of {of} to {side} '{}': {}", said.len(), limit_object.name, said.join("; "));
        if !made.is_empty() {
            message += &format!("; {} kept as new strings", so_many(made.len(), "part"));
        }
        if !left.is_empty() {
            message += &format!("; {} wholly {side} already and left as {}: {}", left.len(), if left.len() == 1 { "it was" } else { "they were" }, names(&left));
        }
        changes.extend(made);
        Ok(Outcome::new(changes).message(message))
    }
}

/// One thing trimmed by its own kind's rule: its next revision first, then any new strings.
fn trim_one(inputs: &Inputs<'_>, object: &Arc<Object>, limit: &zd_core::Polyline, ring: &[[f64; 2]], inside: bool) -> Result<Done, String> {
    let kept = |dropped: bool| if dropped { Done::Dropped } else { Done::Left };
    match &object.body {
        Body::String(line) => match trim_string_in_plan(line, limit, inside).map_err(why)? {
            InPlan::Untouched => Ok(kept(false)),
            InPlan::Dropped => Ok(kept(true)),
            InPlan::Trimmed(t) => {
                let mut pieces = t.pieces.into_iter();
                let mut changes = vec![Change::revise_body(object, Body::String(pieces.next().expect("a part kept")))];
                for (k, piece) in pieces.enumerate() {
                    changes.push(Change::Create { name: format!("{} part {}", object.name, k + 2), layer: inputs.layer_name(object), attributes: object.attributes.clone(), body: Body::String(piece) });
                }
                Ok(Done::Trimmed { changes, said: format!("cut in {}, {} dropped", so_many(t.cuts, "place"), so_many(t.dropped, "part")) })
            }
        },
        Body::Surface(s) => {
            let t = trim_surface_in_plan(s.mesh(), ring, inside).map_err(why)?;
            match t.kept {
                InPlan::Untouched => Ok(kept(false)),
                InPlan::Dropped => Ok(kept(true)),
                InPlan::Trimmed(mesh) => {
                    let said = format!("{} m² of {} m² kept in plan, {} triangles, in {:.0} ms", thousands(t.area_kept, 1), thousands(t.area_in, 1), mesh.triangles.len(), t.ms);
                    Ok(Done::Trimmed { changes: vec![Change::revise_body(object, Body::Surface(Surface::new(mesh)))], said })
                }
            }
        }
        Body::Solid(s) => {
            let t = trim_solid_in_plan(s.mesh(), ring, inside).map_err(why)?;
            match t.kept {
                InPlan::Untouched => Ok(kept(false)),
                InPlan::Dropped => Ok(kept(true)),
                InPlan::Trimmed(mesh) => {
                    let crossings = find_solid_crossings(&mesh, crossing_tolerance());
                    let solid = Solid::new(mesh).map_err(|v| format!("the piece left isn't closed ({v:?}): report it"))?.with_crossings(crossings);
                    let parts = if t.parts > 1 { format!(", in {} parts", t.parts) } else { String::new() };
                    let said = format!("{} m³ of {} m³ kept{parts}, in {:.0} ms", thousands(t.volume_kept, 1), thousands(t.volume_in, 1), t.ms);
                    Ok(Done::Trimmed { changes: vec![Change::revise_body(object, Body::Solid(solid))], said })
                }
            }
        }
        Body::Grid(g) => {
            let t = trim_grid_in_plan(g, ring, inside);
            match t.kept {
                InPlan::Untouched => Ok(kept(false)),
                InPlan::Dropped => Ok(kept(true)),
                InPlan::Trimmed(grid) => {
                    let said = format!("{} of {} nodes with a height kept, {} by {}", thousands(t.nodes_kept as f64, 0), thousands(t.nodes_in as f64, 0), grid.cols, grid.rows);
                    Ok(Done::Trimmed { changes: vec![Change::revise_body(object, Body::Grid(Arc::new(grid)))], said })
                }
            }
        }
        other => Err(format!("a {} can't be trimmed in plan; only strings, surfaces, solids and grids", format!("{:?}", other.kind()).to_lowercase())),
    }
}
