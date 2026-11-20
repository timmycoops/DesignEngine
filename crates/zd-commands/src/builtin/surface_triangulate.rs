//! `surface.triangulate` (Surface tools plan, workstream 4; decision 0096): one surface through the strings picked, each
//! segment kept as an edge, bounded by the outermost closed strings or the hull. New, on a layer; the strings are left as
//! they were.

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, Body, Change, ObjectId, ObjectKind, Polyline, Surface};
use zd_geom::{LONGEST_HULL_EDGE, TinBound, TinRefusal, triangulate_strings};

use super::trim_in_plan::thousands;
use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

pub struct TriangulateStrings;

fn default_layer() -> String {
    "triangulated".into()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriangulateStringsParams {
    /// The strings it's made through.
    pub strings: Vec<ObjectId>,
    /// With no closed string to bound it, the longest edge the hull keeps (m): 200 unless set.
    #[serde(default)]
    pub longest: Option<f64>,
    #[serde(default = "default_layer")]
    pub layer: String,
    /// What it's called: one string's name and "surface", or "Triangulated".
    #[serde(default)]
    pub name: Option<String>,
}

fn names(of: &[&str]) -> String {
    let shown: Vec<String> = of.iter().take(10).map(|n| format!("'{n}'")).collect();
    if of.len() > 10 { format!("{} and {} more", shown.join(", "), of.len() - 10) } else { shown.join(", ") }
}

impl Command for TriangulateStrings {
    type Params = TriangulateStringsParams;
    const NAME: &'static str = "surface.triangulate";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Triangulate a surface through strings, each segment kept as an edge, bounded by the outermost closed strings or the hull",
            inputs: vec![Field::new("strings", FieldType::Objects, "The strings it's made through")],
            params: vec![
                Field::new("longest", FieldType::Number { unit: "m" }, "With no closed string, the longest edge the hull keeps; 200 m unless set"),
                Field::new("layer", FieldType::Layer, "Layer for the surface"),
                Field::new("name", FieldType::Text, "What it's called"),
            ],
            outputs: vec![Field::object("surface", ObjectKind::Surface, "The surface made")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.strings.is_empty() {
            return Err(CommandError::Invalid("nothing to triangulate: pick the strings".into()));
        }
        let mut seen = std::collections::HashSet::new();
        if let Some(twice) = p.strings.iter().find(|id| !seen.insert(**id)) {
            return Err(CommandError::Invalid(format!("object {twice} is given twice")));
        }
        let mut objects = Vec::with_capacity(p.strings.len());
        let mut lines: Vec<Polyline> = Vec::with_capacity(p.strings.len());
        for &id in &p.strings {
            let (object, line) = inputs.string(id)?;
            objects.push(object);
            lines.push(line.clone());
        }
        let layer = p.layer.trim();
        if layer.is_empty() {
            return Err(CommandError::Invalid("no layer for the surface".into()));
        }
        let longest = p.longest.unwrap_or(LONGEST_HULL_EDGE);
        let named = |i: usize| objects[i].name.as_str();
        let made = triangulate_strings(&lines, longest).map_err(|refusal| {
            CommandError::Invalid(match refusal {
                TinRefusal::Crossings { places, count } => {
                    let listed: Vec<String> = places
                        .iter()
                        .map(|c| {
                            let who = if c.strings[0] == c.strings[1] { format!("'{}' over itself", named(c.strings[0])) } else { format!("'{}' and '{}'", named(c.strings[0]), named(c.strings[1])) };
                            format!("{who} at {} E {} N, RL {:.3} and {:.3}", thousands(c.at[0], 1), thousands(c.at[1], 1), c.heights[0], c.heights[1])
                        })
                        .collect();
                    let more = if count > places.len() { format!(" (the first {} of {count})", places.len()) } else { String::new() };
                    format!(
                        "the strings cross or touch in plan at different heights in {count} place{}, so they can't be one surface; set them to one height there, or leave one out{more}: {}",
                        if count == 1 { "" } else { "s" },
                        listed.join("; ")
                    )
                }
                TinRefusal::Outside(which) => {
                    let of: Vec<&str> = which.iter().map(|&i| named(i)).collect();
                    format!("{} {} partly outside every closed string picked, which bound the surface; leave {} out, or pick a closed string round all of them", names(&of), if of.len() == 1 { "lies" } else { "lie" }, if of.len() == 1 { "it" } else { "them" })
                }
                TinRefusal::Invalid(words) => words,
            })
        })?;

        let vertices: usize = lines.iter().map(|l| l.vertices.len()).sum();
        let by = match &made.bound {
            TinBound::Strings(outer) => format!("bounded by {}", names(&outer.iter().map(|&i| named(i)).collect::<Vec<_>>())),
            TinBound::Hull { longest, peeled: 0 } => format!("bounded by the hull, no edge along it over {longest} m"),
            TinBound::Hull { longest, peeled } => format!("bounded by the hull, {peeled} triangle{} with an edge over {longest} m along it peeled back", if *peeled == 1 { "" } else { "s" }),
        };
        let joined = if made.crossings_joined > 0 { format!(", {} where strings cross at one height made vertices", made.crossings_joined) } else { String::new() };
        let of = if objects.len() == 1 { format!("'{}'", named(0)) } else { format!("{} strings", objects.len()) };
        let said = format!(
            "triangulated {of} ({vertices} vertices): {} triangles, {} m² in plan, {by}{joined}, in {:.0} ms",
            thousands(made.mesh.triangles.len() as f64, 0),
            thousands(made.area, 1),
            made.ms
        );
        let name = p.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).map_or_else(|| if objects.len() == 1 { format!("{} surface", named(0)) } else { "Triangulated".to_string() }, str::to_string);
        Ok(Outcome::new(vec![Change::Create { name, layer: layer.to_string(), attributes: Attributes::new(), body: Body::Surface(Surface::new(made.mesh)) }]).message(said))
    }
}
