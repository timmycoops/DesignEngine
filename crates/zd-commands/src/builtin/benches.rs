//! Benches (decision 0071): solids cut at bench levels before the blocks are cut, each piece labelled with its bench, so
//! the reserves are reported by bench as a schedule needs them. What's benched, and at what levels, is set by bench rules:
//! a short table read top down, the first rule that fits a solid winning. A rule picks solids by the attributes they
//! carry after the seam cut (horizon, material, seam, strip, block or any other), and says benched or not and, if
//! benched, from what RL to what RL every so many metres, each bench named by its bottom or its top.

use std::collections::BTreeMap;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, Object, ObjectId, Solid};
use zd_geom::{BenchCut, bench_levels, cut_solid_by_levels, find_solid_crossings};

use super::blocks::{ADDS_UP, block_name, named, word};
use super::in_parallel_watched;
use super::solid::checked_input;
use super::surface::default_tolerance as crossing_tolerance;
use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Severity};

/// `solid.cut_by_benches`: each solid cut at bench levels by the first bench rule that fits it, each piece keeping its
/// solid's attributes and taking its bench (its floor RL, or its top RL if the rule says so; `below <RL>` and
/// `above <RL>` past the levels). A solid no rule benches is carried over whole, so the layer made holds everything for
/// the block cut. Flaps thinner than the flap thickness are taken off and said. The inputs are left as they are.
pub struct CutByBenches;

/// The attributes each bench carries its floor and top RL in, where it has them, so a later step needn't read them off
/// its name: the interrogation's column check takes the seam inside them (decision 0071).
pub const BENCH_FLOOR: &str = "bench_floor";
pub const BENCH_TOP: &str = "bench_top";

/// The attribute each bench carries the id of the solid it was benched from in: a block's benches of a seam are told apart
/// by it, their block and their seam, so the column check can take them together (decision 0071).
pub const BENCHED_FROM: &str = "benched_from";

/// Which level names a bench.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchEnd {
    /// Its floor RL: the bench from RL 70 to 80 is 70.
    #[default]
    Bottom,
    /// Its top RL: the bench from RL 70 to 80 is 80.
    Top,
}

fn yes() -> bool {
    true
}

fn default_from() -> f64 {
    0.0
}

fn default_to() -> f64 {
    500.0
}

fn default_every() -> f64 {
    10.0
}

/// One bench rule: which solids it takes, and how they're benched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchRule {
    /// What a solid must carry to take this rule: an attribute's name to the values it may hold, a comma list of words,
    /// numbers or ranges such as `1-5` (`*` or blank is any). Every attribute named must fit. Left out, every solid fits.
    #[serde(default)]
    pub when: BTreeMap<String, String>,
    /// Whether the solids it takes are benched; `false` carries them over whole.
    #[serde(default = "yes")]
    pub benched: bool,
    /// The lowest level (RL); 0 by default.
    #[serde(default = "default_from")]
    pub from: f64,
    /// The highest level (RL), or the last under it on the interval; 500 by default.
    #[serde(default = "default_to")]
    pub to: f64,
    /// Metres between levels; 10 by default.
    #[serde(default = "default_every")]
    pub every: f64,
    /// Whether a bench is named by its floor (the default) or its top.
    #[serde(default)]
    pub named_by: BenchEnd,
    /// The layer for what this rule takes, benched or carried over whole; blank or left out, the run's layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
}

impl Default for BenchRule {
    fn default() -> Self {
        BenchRule { when: BTreeMap::new(), benched: true, from: default_from(), to: default_to(), every: default_every(), named_by: BenchEnd::Bottom, layer: None }
    }
}

fn default_rules() -> Vec<BenchRule> {
    vec![BenchRule::default()]
}

fn default_flap_thickness() -> f64 {
    0.01
}

fn default_pattern() -> String {
    "{solid} {bench}".into()
}

fn default_attribute() -> String {
    "bench".into()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutByBenchesParams {
    /// The solids to cut, each on its own.
    pub solids: Vec<ObjectId>,
    /// Layer for the benches and the solids left whole, unless their rule names its own.
    pub layer: String,
    /// The bench rules, read top down, the first that fits a solid winning; by default one: everything from RL 0 to 500
    /// every 10 m, named by the floor.
    #[serde(default = "default_rules")]
    pub rules: Vec<BenchRule>,
    /// Where a piece is thinner than this between its top and bottom (m), a flap, it's taken off and its volume said
    /// (decision 0064); 0.01 by default, 0 for none.
    #[serde(default = "default_flap_thickness")]
    pub flap_thickness: f64,
    /// Each piece's name: attributes in braces, `{solid}` the solid's name. `{solid} {bench}` by default.
    #[serde(default = "default_pattern")]
    pub name: String,
    /// The attribute that takes each piece's bench; `bench` by default.
    #[serde(default = "default_attribute")]
    pub attribute: String,
}

/// An attribute's value by name, the name's case aside.
fn attribute<'a>(attributes: &'a Attributes, key: &str) -> Option<&'a AttrValue> {
    attributes.get(key).or_else(|| attributes.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v))
}

/// A range such as `1-5` or `-20--5`: two numbers either side of a dash, lowest first. `U-M` is no range.
fn range(item: &str) -> Option<(f64, f64)> {
    item.char_indices().skip(1).filter(|&(_, c)| c == '-').find_map(|(i, _)| {
        let (a, b) = (item[..i].trim().parse::<f64>().ok()?, item[i + 1..].trim().parse::<f64>().ok()?);
        Some((a.min(b), a.max(b)))
    })
}

/// Whether one item of a filter's list fits a value: `*`, a range or a number for a value that's a number (or text that
/// reads as one), or the same word, case aside.
fn item_fits(item: &str, value: &AttrValue) -> bool {
    if item == "*" {
        return true;
    }
    let number = match value {
        AttrValue::Number(n) => Some(*n),
        AttrValue::Text(t) => t.trim().parse::<f64>().ok(),
        AttrValue::Bool(_) => None,
    };
    if let Some(n) = number {
        if let Some((a, b)) = range(item) {
            return a <= n && n <= b;
        }
        if let Ok(x) = item.parse::<f64>() {
            return (x - n).abs() <= 1e-9 * x.abs().max(1.0);
        }
    }
    word(value).eq_ignore_ascii_case(item)
}

/// Whether a filter's comma list fits a value; a blank list fits anything.
fn list_fits(list: &str, value: Option<&AttrValue>) -> bool {
    let items: Vec<&str> = list.split(',').map(str::trim).filter(|i| !i.is_empty()).collect();
    if items.is_empty() || items.contains(&"*") {
        return true;
    }
    value.is_some_and(|v| items.iter().any(|item| item_fits(item, v)))
}

impl BenchRule {
    /// Whether a solid carrying these attributes takes this rule.
    pub fn fits(&self, attributes: &Attributes) -> bool {
        self.when.iter().all(|(key, list)| list_fits(list, attribute(attributes, key)))
    }

    /// The layer for what this rule takes: its own, or the run's where it names none.
    pub fn layer_or<'a>(&'a self, run: &'a str) -> &'a str {
        self.layer.as_deref().map(str::trim).filter(|l| !l.is_empty()).unwrap_or(run)
    }

    /// The rule in a few words, for what a run says.
    fn words(&self) -> String {
        let when: Vec<String> = self.when.iter().filter(|(_, l)| !l.trim().is_empty()).map(|(k, l)| format!("{k} {}", l.trim())).collect();
        let which = if when.is_empty() { "everything".to_string() } else { when.join(", ") };
        let onto = self.layer.as_deref().map(str::trim).filter(|l| !l.is_empty()).map(|l| format!(", onto '{l}'")).unwrap_or_default();
        if self.benched {
            let by = if self.named_by == BenchEnd::Top { ", named by the top" } else { "" };
            format!("{which}: RL {} to {} every {} m{by}{onto}", self.from, self.to, self.every)
        } else {
            format!("{which}: not benched{onto}")
        }
    }
}

/// The index of the first rule a solid fits.
pub fn rule_for(rules: &[BenchRule], attributes: &Attributes) -> Option<usize> {
    rules.iter().position(|r| r.fits(attributes))
}

/// A piece's bench as its attribute: the RL of the level naming it, or `below <RL>` / `above <RL>` past the levels.
fn bench_value(floor: Option<f64>, top: Option<f64>, by: BenchEnd) -> AttrValue {
    match (floor, top, by) {
        (None, Some(top), _) => AttrValue::Text(format!("below {}", word(&AttrValue::Number(top)))),
        (Some(floor), None, _) => AttrValue::Text(format!("above {}", word(&AttrValue::Number(floor)))),
        (Some(floor), Some(_), BenchEnd::Bottom) => AttrValue::Number(floor),
        (Some(_), Some(top), BenchEnd::Top) => AttrValue::Number(top),
        (None, None, _) => AttrValue::Text("whole".into()),
    }
}

/// More solids than this and a list of names stops.
const NAMED_UP_TO: usize = 5;

/// A volume (m³) this small is nothing.
const NOTHING: f64 = 1e-6;

type Benched = (BenchCut, Vec<Result<Solid, CommandError>>);

impl Command for CutByBenches {
    type Params = CutByBenchesParams;
    const NAME: &'static str = "solid.cut_by_benches";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Cut solids at bench levels by bench rules, each piece taking its bench",
            inputs: vec![Field::new("solids", FieldType::Objects, "The solids to cut, each on its own")],
            params: vec![
                Field::new("layer", FieldType::Layer, "Layer for the benches and the solids left whole, unless their rule names its own"),
                Field::new("rules", FieldType::Text, "Bench rules read top down, the first a solid fits winning: when (attribute to values), benched, from, to, every, named_by, layer"),
                Field::new("flap_thickness", FieldType::Number { unit: "m" }, "Flaps thinner than this are taken off each piece and said; 0.01 by default, 0 for none"),
                Field::new("name", FieldType::Text, "Each piece's name from its attributes in braces; {solid} {bench} by default"),
                Field::new("attribute", FieldType::Text, "The attribute that takes each piece's bench; bench by default"),
            ],
            outputs: vec![Field::new("pieces", FieldType::Objects, "Each solid's benches bottom up, or the solid whole where no rule benches it")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.solids.is_empty() {
            return Err(CommandError::Invalid("give the solids to cut".into()));
        }
        if p.rules.is_empty() {
            return Err(CommandError::Invalid("give at least one bench rule".into()));
        }
        if !(p.flap_thickness >= 0.0 && p.flap_thickness.is_finite()) {
            return Err(CommandError::Invalid(format!("a flap thickness of {} m is less than nothing", p.flap_thickness)));
        }
        if p.attribute.trim().is_empty() {
            return Err(CommandError::Invalid("name the attribute that takes the bench".into()));
        }
        let levels: Vec<Option<Vec<f64>>> = p
            .rules
            .iter()
            .enumerate()
            .map(|(k, r)| if r.benched { bench_levels(r.from, r.to, r.every).map(Some).map_err(|e| CommandError::Invalid(format!("bench rule {}: {e}", k + 1))) } else { Ok(None) })
            .collect::<Result<_, _>>()?;

        let mut notes = Vec::new();
        let mut solids: Vec<(&Object, &Solid, Option<usize>)> = Vec::new();
        for &id in &p.solids {
            if solids.iter().any(|(o, _, _)| o.id == id) {
                return Err(CommandError::Invalid(format!("solid {id} is given twice")));
            }
            let (object, solid) = inputs.solid(id)?;
            checked_input("solid", &object.name, solid.crossings(), solid.mesh(), true, &mut notes)?;
            solids.push((object.as_ref(), solid, rule_for(&p.rules, &object.attributes)));
        }

        // Rules no solid takes, and attributes no solid carries: likely a word mistyped.
        for (k, rule) in p.rules.iter().enumerate() {
            let missing: Vec<&str> = rule.when.iter().filter(|(_, l)| !l.trim().is_empty()).map(|(key, _)| key.as_str()).filter(|key| solids.iter().all(|(o, _, _)| attribute(&o.attributes, key).is_none())).collect();
            if !missing.is_empty() {
                let text = format!("bench rule {} ({}) asks for {}, which none of the solids carries", k + 1, rule.words(), missing.join(", "));
                notes.push(Note::warning(Code::BenchRuleUnused, text));
            } else if solids.iter().all(|(_, _, r)| *r != Some(k)) {
                notes.push(Note::new(Severity::Info, Some(Code::BenchRuleUnused), format!("bench rule {} ({}) takes none of the solids", k + 1, rule.words())));
            }
        }

        let started = Instant::now();
        let cuts = in_parallel_watched(inputs, "solids", &solids, |(object, solid, rule)| -> Option<Result<Benched, CommandError>> {
            let levels = rule.and_then(|k| levels[k].as_ref())?;
            Some((|| {
                let cut = cut_solid_by_levels(solid.mesh(), levels, p.flap_thickness).map_err(|e| CommandError::Geometry(format!("'{}': {e}", object.name)))?;
                let made = cut
                    .pieces
                    .iter()
                    .map(|piece| {
                        let crossings = find_solid_crossings(&piece.mesh, crossing_tolerance());
                        Solid::new(piece.mesh.clone()).map(|s| s.with_crossings(crossings)).map_err(CommandError::NotWatertight)
                    })
                    .collect();
                Ok((cut, made))
            })())
        })?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;

        let mut changes = Vec::new();
        let (mut benched, mut pieces, mut whole, mut flapped, mut flaps) = (0, 0, Vec::new(), 0, 0.0);
        let (mut volume_in, mut volume_out, mut triangles_in, mut triangles_out) = (0.0, 0.0, 0, 0);
        let mut by_bench: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        let (mut worst, mut worst_of) = (0.0_f64, String::new());
        let mut taken = vec![0usize; p.rules.len()];
        let mut onto: BTreeMap<String, usize> = BTreeMap::new();
        for ((object, solid, rule), cut) in solids.iter().zip(cuts) {
            if let Some(k) = rule {
                taken[*k] += 1;
            }
            let layer = rule.map_or(p.layer.as_str(), |k| p.rules[k].layer_or(&p.layer)).to_string();
            let Some(result) = cut else {
                // No rule benches it: carried over whole, as it is.
                whole.push(object.name.clone());
                let name = block_name(&p.name, &object.attributes, &object.name, "");
                *onto.entry(layer.clone()).or_default() += 1;
                changes.push(Change::Create { name, layer, attributes: object.attributes.clone(), body: Body::Solid((*solid).clone()) });
                continue;
            };
            let (cut, made) = result?;
            let by = p.rules[rule.expect("a cut has its rule")].named_by;
            benched += 1;
            triangles_in += solid.mesh().triangle_count();
            volume_in += cut.input_volume;
            flapped += cut.flapped;
            flaps += cut.flaps;
            let share = cut.difference().abs() / cut.input_volume.abs().max(f64::MIN_POSITIVE);
            if share > worst {
                worst = share;
                worst_of = object.name.clone();
            }
            if share > ADDS_UP {
                let text = format!("'{}': its benches and flaps come to {:+.3} m³ against its {:.3} m³", object.name, cut.difference(), cut.input_volume);
                notes.push(Note::warning(Code::BenchCutDiffers, text).on(object.id).measured(cut.difference(), "m3"));
            }
            for (piece, solid) in cut.pieces.iter().zip(made) {
                let solid = solid?;
                let bench = bench_value(piece.floor, piece.top, by);
                let mut attributes = object.attributes.clone();
                attributes.insert(p.attribute.clone(), bench.clone());
                attributes.insert(BENCHED_FROM.into(), AttrValue::Text(object.id.to_string()));
                for (key, level) in [(BENCH_FLOOR, piece.floor), (BENCH_TOP, piece.top)] {
                    match level {
                        Some(rl) => attributes.insert(key.into(), AttrValue::Number(rl)),
                        None => attributes.remove(key),
                    };
                }
                let entry = by_bench.entry(word(&bench)).or_default();
                entry.0 += piece.volume;
                entry.1 += 1;
                volume_out += piece.volume;
                triangles_out += piece.mesh.triangle_count();
                pieces += 1;
                let name = block_name(&p.name, &attributes, &object.name, "");
                *onto.entry(layer.clone()).or_default() += 1;
                changes.push(Change::Create { name, layer: layer.clone(), attributes, body: Body::Solid(solid) });
            }
        }
        if !whole.is_empty() {
            notes.push(Note::info(format!("{} solids no rule benches, carried over whole: {}", whole.len(), named(&whole))).measured(whole.len() as f64, "solids"));
        }
        if flaps > NOTHING {
            let text = format!("{flaps:.3} m³ in flaps thinner than {} m taken off {flapped} benches, left out", p.flap_thickness);
            notes.push(Note::new(Severity::Info, Some(Code::BenchFlapsLeftOut), text).measured(flaps, "m3"));
        }
        let rules: Vec<String> = p.rules.iter().zip(&taken).enumerate().map(|(k, (r, n))| format!("{}. {} ({n} solids)", k + 1, r.words())).collect();
        // Benches by name, the numbers in order, then those past the levels.
        let mut benches: Vec<(&String, &(f64, usize))> = by_bench.iter().collect();
        benches.sort_by(|a, b| match (a.0.parse::<f64>(), b.0.parse::<f64>()) {
            (Ok(x), Ok(y)) => x.total_cmp(&y),
            (Ok(_), Err(_)) => std::cmp::Ordering::Less,
            (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
            _ => a.0.cmp(b.0),
        });
        let listed: Vec<String> = benches.iter().take(60).map(|(b, (v, n))| format!("{b} {v:.3} ({n})")).collect();
        let more = if benches.len() > 60 { format!(" and {} more", benches.len() - 60) } else { String::new() };
        let mut outcome = Outcome::new(changes)
            .message(format!(
                "{} solids: {benched} benched into {pieces} pieces, {} carried over whole, in {ms:.0} ms: {triangles_in} triangles in, {triangles_out} out",
                solids.len(),
                whole.len()
            ))
            .message(format!("Rules: {}", rules.join("; ")))
            .message(format!("Onto layers: {}", onto.iter().map(|(l, n)| format!("'{l}' {n}")).collect::<Vec<_>>().join(", ")))
            .message(format!("Volumes (m³): {volume_in:.3} benched in, {volume_out:.3} out, {flaps:.3} in flaps; by bench: {}{more}", listed.join(", ")))
            .message(if worst_of.is_empty() {
                "Benches add back to every solid exactly".to_string()
            } else {
                format!("Benches add back to each solid within {worst:.1e} of its volume, the most for '{worst_of}'")
            });
        if whole.len() > NAMED_UP_TO {
            outcome = outcome.message(format!("{} solids carried over whole", whole.len()));
        }
        outcome.notes.extend(notes);
        Ok(outcome)
    }
}

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

    fn rule(when: &[(&str, &str)], benched: bool) -> BenchRule {
        BenchRule { when: when.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), benched, ..BenchRule::default() }
    }

    #[test]
    fn filters_take_words_numbers_and_ranges() {
        let ob = attributes(&[("horizon", AttrValue::Text("U-M".into())), ("material", AttrValue::Text("waste".into())), ("strip", AttrValue::Number(3.0))]);
        assert!(rule(&[("horizon", "U-M, M-L")], false).fits(&ob), "U-M is a word, not a range");
        assert!(rule(&[("Horizon", "u-m")], false).fits(&ob), "names and words, case aside");
        assert!(rule(&[("strip", "1-5")], false).fits(&ob));
        assert!(!rule(&[("strip", "4-5")], false).fits(&ob));
        assert!(rule(&[("strip", "1, 3")], false).fits(&ob));
        assert!(rule(&[("strip", ""), ("material", "*")], false).fits(&ob), "blank and * are any");
        assert!(!rule(&[("seam", "U")], false).fits(&ob), "an attribute it hasn't doesn't fit");
        assert!(rule(&[("material", "waste"), ("strip", "3")], false).fits(&ob), "every attribute named must fit");
        assert!(!rule(&[("material", "coal"), ("strip", "3")], false).fits(&ob));
        let text_strip = attributes(&[("strip", AttrValue::Text("12".into()))]);
        assert!(rule(&[("strip", "10-15")], false).fits(&text_strip), "text that reads as a number");
        assert_eq!(range("-20--5"), Some((-20.0, -5.0)));
        assert_eq!(range("U-M"), None);
    }

    #[test]
    fn the_first_rule_that_fits_wins() {
        let rules = vec![rule(&[("horizon", "U-M, M-L")], false), rule(&[("material", "coal"), ("strip", "1-5")], false), rule(&[("material", "coal")], true), BenchRule::default()];
        let coal = |strip: f64| attributes(&[("horizon", AttrValue::Text("U".into())), ("material", AttrValue::Text("coal".into())), ("strip", AttrValue::Number(strip))]);
        let waste = |h: &str| attributes(&[("horizon", AttrValue::Text(h.into())), ("material", AttrValue::Text("waste".into())), ("strip", AttrValue::Number(2.0))]);
        assert_eq!(rule_for(&rules, &waste("M-L")), Some(0));
        assert_eq!(rule_for(&rules, &coal(2.0)), Some(1));
        assert_eq!(rule_for(&rules, &coal(7.0)), Some(2));
        assert_eq!(rule_for(&rules, &waste("OB")), Some(3));
        assert_eq!(rule_for(&rules[..3], &waste("OB")), None);
    }

    #[test]
    fn a_bench_is_named_by_its_floor_or_its_top() {
        assert_eq!(bench_value(Some(70.0), Some(80.0), BenchEnd::Bottom), AttrValue::Number(70.0));
        assert_eq!(bench_value(Some(70.0), Some(80.0), BenchEnd::Top), AttrValue::Number(80.0));
        assert_eq!(bench_value(None, Some(0.0), BenchEnd::Top), AttrValue::Text("below 0".into()));
        assert_eq!(bench_value(Some(500.0), None, BenchEnd::Bottom), AttrValue::Text("above 500".into()));
        assert_eq!(bench_value(Some(-2.5), Some(0.0), BenchEnd::Bottom), AttrValue::Number(-2.5));
    }
}
