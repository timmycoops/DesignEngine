//! `surface.between_strings` (decision 0079): one surface between the strings picked, for a road, a batter or a berm
//! drawn by hand as its edges. Two strings make the sheet between them; more, as the toe of a fill, a road's two edges
//! and the crest of a cut, make one sheet across them all, in order across whatever order they're given in. The strings
//! are left as they are.

use std::collections::HashSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ObjectId, ObjectKind, Polyline, Surface, TriMesh};
use zd_geom::{COINCIDENT, find_crossings, sheet_between, sheets_as_one, strings_across};

use super::face::crossing_note;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

pub struct SurfaceBetweenStrings;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceBetweenStringsParams {
    /// Two strings or more, in any order: they're put in order across.
    pub strings: Vec<ObjectId>,
    /// Defaults to the first string's layer.
    #[serde(default)]
    pub layer: Option<String>,
    /// Defaults to the names of the strings at either side.
    #[serde(default)]
    pub name: Option<String>,
}

/// What the surface takes from its strings where they all agree: the strip it's on, and the pit (decision 0029).
const KEPT: [&str; 2] = ["strip", "pit"];

/// A sheet's area in plan: its triangles' areas seen from above (m²).
fn plan_area(mesh: &TriMesh) -> f64 {
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0
        })
        .sum()
}

impl Command for SurfaceBetweenStrings {
    type Params = SurfaceBetweenStringsParams;
    const NAME: &'static str = "surface.between_strings";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Make one surface between two strings or more: a road between its edges, with its batters",
            inputs: vec![Field::new("strings", FieldType::Objects, "Two strings or more, in any order: they're put in order across")],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the surface; the first string's by default"),
                Field::new("name", FieldType::Text, "The surface's name; the strings at either side by default"),
            ],
            outputs: vec![Field::object("surface", ObjectKind::Surface, "The sheet between the strings, wound upward")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let mut seen = HashSet::new();
        if let Some(twice) = p.strings.iter().find(|id| !seen.insert(**id)) {
            return Err(CommandError::Invalid(format!("string {twice} is given twice")));
        }
        let mut objects = Vec::with_capacity(p.strings.len());
        let mut lines: Vec<&Polyline> = Vec::with_capacity(p.strings.len());
        for &id in &p.strings {
            let (object, line) = inputs.string(id)?;
            objects.push(object);
            lines.push(line);
        }
        let started = Instant::now();
        let across = strings_across(&lines).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let name_of = |k: usize| objects[across.order[k]].name.as_str();
        let mut sheets = Vec::with_capacity(across.lines.len() - 1);
        let mut failed = Vec::new();
        for (k, pair) in across.lines.windows(2).enumerate() {
            match sheet_between(&pair[0], &pair[1]) {
                Ok(sheet) => sheets.push(sheet),
                Err(why) => failed.push(format!("between '{}' and '{}': {why}", name_of(k), name_of(k + 1))),
            }
        }
        if !failed.is_empty() {
            return Err(CommandError::Invalid(format!("no surface was made. {}", failed.join("; "))));
        }
        let mesh = sheets_as_one(&sheets);
        let (area, in_plan, triangles) = (mesh.area(), plan_area(&mesh), mesh.triangles.len());
        let crossings = find_crossings(&mesh, COINCIDENT);
        let last = across.order.len() - 1;
        let name = p.name.clone().unwrap_or_else(|| format!("{} to {}", name_of(0), name_of(last)));
        let layer = p.layer.clone().unwrap_or_else(|| inputs.layer_name(objects[0]));
        let mut attributes = Attributes::new();
        for key in KEPT {
            if let Some(value) = objects[0].attributes.get(key).filter(|value| objects.iter().all(|o| o.attributes.get(key) == Some(value))) {
                attributes.insert(key.to_string(), value.clone());
            }
        }
        let names: Vec<String> = (0..=last).map(|k| format!("'{}'", name_of(k))).collect();
        let mut message = format!(
            "a surface between {} strings{}: {area:.1} m2, {in_plan:.1} m2 in plan, {triangles} triangles",
            names.len(),
            if names.len() > 2 { format!(", in order across {}", names.join(", ")) } else { format!(", {} and {}", names[0], names[1]) },
        );
        if !across.reversed.is_empty() {
            let turned: Vec<String> = across.reversed.iter().map(|&i| format!("'{}'", objects[i].name)).collect();
            message += &format!("; {} taken the other way along, to run with '{}'", turned.join(", "), objects[0].name);
        }
        message += &format!("; {:.0} ms", started.elapsed().as_secs_f64() * 1e3);
        let mut outcome = Outcome::new(vec![Change::Create { name, layer, attributes, body: Body::Surface(Surface::new(mesh).with_crossings(crossings.clone())) }]).message(message);
        if crossings.pairs > 0 {
            outcome.notes.push(crossing_note(format!("{} pairs of the surface's triangles cross", crossings.pairs), &crossings).on_made(0));
        }
        Ok(outcome)
    }
}
