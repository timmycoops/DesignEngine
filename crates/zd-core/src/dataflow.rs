//! The reserves dataflow (decisions 0070 and 0072): the fields its units carry, where each field's value comes from, the
//! steps it runs in Spry's order, and the parameter tables it reads. Running one never changes it; what a run makes is
//! a `Table` a case.
//!
//! Built to take more (Tim, 15 November 2026): fields are data, so a new quality or metric is a new field; steps are named,
//! so a new step slots in by name; parameter tables carry any columns.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zd_reserves::{Field, Fields};

use crate::{ObjectId, ParamValue};

/// Where a field's value comes from when a block becomes a unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum FieldSource {
    /// The solid's volume, the volume of record.
    Volume,
    /// The solid's footprint in plan, from its upward-facing triangles.
    PlanArea,
    /// Volume over plan area: a vertical thickness.
    Thickness,
    /// A block attribute of this name (an interrogated RD or quality); where a block has none, the parameter tables'
    /// column of the field's name.
    Attribute { attribute: String },
    /// The parameter tables' column of this name (a setting: a threshold, a cut-off, a waste RD).
    Parameter { column: String },
    /// Worked out by a step, none at the start.
    Worked,
}

/// One field: what the column engine needs of it, and where its value comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataflowField {
    #[serde(flatten)]
    pub field: Field,
    #[serde(flatten)]
    pub source: FieldSource,
}

/// A field of words on each unit (the Build Plan's "Waste allocation plan", decision 2): a process, a method, a zone, a
/// drill type. It starts from a block attribute or a parameter column of words, or empty; a step's script reads and sets
/// it; a split piece keeps it; and the table carries it as a words column beside the numbers, a leaf whose units differ
/// showing them joined (`Dragline; Poststrip`). It doesn't add up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordField {
    pub name: String,
    #[serde(flatten)]
    pub source: FieldSource,
}

impl WordField {
    pub fn new(name: &str, source: FieldSource) -> Self {
        WordField { name: name.to_string(), source }
    }
}

/// The words a unit carries that a word field can't take as its name: what the run already labels it by.
pub const UNIT_LABELS: [&str; 7] = ["seam", "horizon", "material", "bench", "became", "section", "rom"];

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

/// One step, by name, on or off, with its settings: the names of the fields and parameter columns it reads where they
/// aren't the usual ones (decision 0073, point 8), so a site's own names work without renaming them. A step with a
/// formula is a formula step (decision 0077): its name is its own, and it runs its Starlark on the units its filter fits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataflowStep {
    pub step: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub on: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<FormulaStep>,
    /// A table script step (decision 11 of 0099): run on the case's table after the final writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableStep>,
    /// An allocation step (Waste allocation plan, decisions 1, 3 and 4): run on each column's units after `rom`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocation: Option<AllocationStep>,
}

/// An allocation step's own part (the Waste allocation plan, decisions 1, 3 and 4): Starlark defining
/// `def allocate(column, p):`, run once on each block column's units after `rom` and before `final`. It may split a unit
/// by thickness from the top or by volume shares (the pieces adding back to it exactly), set fields and words on each,
/// and place schedule fields onto other leaves; the grouping and splitting are done in Rust.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationStep {
    pub script: String,
}

/// A table script step's own part (the Build Plan's "Tables and scripts plan", decision 11): Starlark defining
/// `def run(t, p):`, run on the case's reserves table once the final writes are done. The columns it gives back that the
/// table hasn't are the run's own, written again each time the dataflow runs, so a ratio or summary every case needs is
/// worked the same way every case and every month.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableStep {
    pub script: String,
}

/// A formula step's own parts (decision 0077): the units it runs on, its Starlark, and its known answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaStep {
    /// The units it runs on, in the bench rules' words, matched against the unit's labels (`material`, `seam`,
    /// `bench`, `became`) and its block's attributes: `{"material": "coal", "seam": "U, M"}`. None: every unit.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, String>,
    /// Starlark defining `def formula(unit, p):`.
    pub script: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<FormulaTest>,
}

/// A worked example a formula must give, run whenever the dataflow is saved: a unit's fields and labels, its block's
/// attributes and its parameters, and the fields the formula must leave it with (`null`: with no value), or, with
/// `stops`, words the formula must stop the block with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaTest {
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub unit: BTreeMap<String, ParamValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub block: BTreeMap<String, ParamValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, ParamValue>,
    /// The ROM coal section's coal added up, for a unit in one (`unit.rom_coal`); none, it's in none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rom_coal: BTreeMap<String, ParamValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub expect: BTreeMap<String, Option<f64>>,
    /// Words the formula must stop the block with (`fail("…")`, or a parameter no row gives): it mustn't run on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stops: Option<String>,
}

impl DataflowStep {
    pub fn on(step: &str) -> Self {
        DataflowStep { step: step.to_string(), on: true, settings: BTreeMap::new(), formula: None, table: None, allocation: None }
    }

    /// An allocation step of this name (Waste allocation plan, decision 4), run on each column after `rom`.
    pub fn allocation(step: &str, script: &str) -> Self {
        DataflowStep { allocation: Some(AllocationStep { script: script.to_string() }), ..DataflowStep::on(step) }
    }

    /// Its script, for a formula, table script or allocation step; none for a built-in step.
    pub fn script(&self) -> Option<&str> {
        self.formula.as_ref().map(|f| f.script.as_str()).or(self.table.as_ref().map(|t| t.script.as_str())).or(self.allocation.as_ref().map(|a| a.script.as_str()))
    }

    /// Whether it's a built-in step, named for what the engine does: no script of its own.
    pub fn is_built_in(&self) -> bool {
        self.script().is_none()
    }

    /// A table script step of this name (decision 11 of 0099), run on the case's table after the final writes.
    pub fn table_script(step: &str, script: &str) -> Self {
        DataflowStep { table: Some(TableStep { script: script.to_string() }), ..DataflowStep::on(step) }
    }

    /// A formula step of this name, running this script on every unit.
    pub fn formula(step: &str, script: &str) -> Self {
        DataflowStep { formula: Some(FormulaStep { when: BTreeMap::new(), script: script.to_string(), tests: Vec::new() }), ..DataflowStep::on(step) }
    }

    /// A setting's value, or the usual name where the step doesn't set it.
    pub fn setting<'a>(&'a self, key: &'a str) -> &'a str {
        self.settings.get(key).map(String::as_str).filter(|s| !s.trim().is_empty()).unwrap_or(key)
    }
}

/// One level of the reserves table's tree (decision 0074): a block attribute, and the value a block without it takes,
/// if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub attribute: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

impl Level {
    pub fn new(attribute: &str) -> Self {
        Level { attribute: attribute.to_string(), default: None }
    }
}

/// Pit, strip, block, seam and bench, a block with no pit in pit `Pit` (decision 0074).
pub fn default_levels() -> Vec<Level> {
    let mut levels: Vec<Level> = ["pit", "strip", "block", "seam", "bench"].into_iter().map(Level::new).collect();
    levels[0].default = Some("Pit".into());
    levels
}

/// A dataflow: its levels, fields, steps in order and the parameter tables it reads, first to last.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dataflow {
    /// The reserves table's levels top down, block attributes (Tim: "a custom attribute essentially ... these control
    /// the grouping logic and how the table tree looks"): those above `seam` make a column; all of them the tree.
    #[serde(default = "default_levels")]
    pub levels: Vec<Level>,
    pub fields: Vec<DataflowField>,
    #[serde(default)]
    pub steps: Vec<DataflowStep>,
    /// Parameter table objects, read in this order: a value comes from the first that gives it.
    #[serde(default)]
    pub parameters: Vec<ObjectId>,
    /// The project's scripts its formulas load (decision 0099), bound by name when it's saved: a run reads them by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<ObjectId>,
    /// The parameter tables its scripts look up by name (Schedule quantities plan, decision 1), bound when it's saved: a
    /// run reads them by id and records their revisions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lookups: Vec<ObjectId>,
    /// Its fields of words (Waste allocation plan, decision 2), carried by each unit beside the number fields.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<WordField>,
    /// An add-on (Table setup plan, decision 2): only fields, words, parameter tables and its own steps (formula,
    /// allocation, table script), run with a main dataflow at their stage. It has no built-in step of its own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub add_on: bool,
    /// The add-ons a main dataflow runs by default, in order; a table's setup may give its own list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_ons: Vec<ObjectId>,
}

/// Whether a field is a schedule field (Waste allocation plan, decision 5): named with a folder, `Waste\Dragline\Prime
/// Volume`, and written as `Scheduled\Waste\Dragline\Prime Volume` only.
pub fn is_schedule_field(name: &str) -> bool {
    name.contains('\\')
}

impl Default for Dataflow {
    fn default() -> Self {
        Dataflow { levels: default_levels(), fields: Vec::new(), steps: Vec::new(), parameters: Vec::new(), scripts: Vec::new(), lookups: Vec::new(), words: Vec::new(), add_on: false, add_ons: Vec::new() }
    }
}

impl Dataflow {
    /// Whether its word fields hold together: each named once, not as a number field or a unit's own label, and coming
    /// from a block attribute, a parameter column or a step, as words do.
    pub fn check_words(&self) -> Result<(), String> {
        for (i, w) in self.words.iter().enumerate() {
            let name = w.name.trim();
            if name.is_empty() {
                return Err(format!("word field {} has no name", i + 1));
            }
            if name != w.name {
                return Err(format!("the word field {:?} has spaces at its ends", w.name));
            }
            if self.words[..i].iter().any(|v| v.name.eq_ignore_ascii_case(name)) {
                return Err(format!("the word field {name} is listed twice"));
            }
            if self.fields.iter().any(|f| f.field.name.eq_ignore_ascii_case(name)) {
                return Err(format!("{name} is a number field and a word field: give one another name"));
            }
            if UNIT_LABELS.iter().any(|l| l.eq_ignore_ascii_case(name)) {
                return Err(format!("{name} is what the run labels a unit by already: give the word field another name"));
            }
            if !matches!(w.source, FieldSource::Attribute { .. } | FieldSource::Parameter { .. } | FieldSource::Worked) {
                return Err(format!("the word field {name} comes from a block attribute, a parameter column or a step, not a solid's measure"));
            }
        }
        Ok(())
    }

    /// Whether its schedule fields hold together (Waste allocation plan, decision 5): a field named with a folder,
    /// `Waste\Dragline\Prime Volume`, is written under `Scheduled` only, so each part of its name is words, it doesn't
    /// start at `Scheduled`, and it isn't the unit's volume or thickness, which every stage writes.
    pub fn check_schedule_fields(&self) -> Result<(), String> {
        let names = self.fields.iter().map(|f| (f.field.name.as_str(), Some(f.field.kind))).chain(self.words.iter().map(|w| (w.name.as_str(), None)));
        for (name, kind) in names.filter(|(n, _)| is_schedule_field(n)) {
            let parts: Vec<&str> = name.split('\\').collect();
            if parts.iter().any(|p| p.trim().is_empty()) {
                return Err(format!("the schedule field {name} has an empty folder: name it as Waste\\Dragline\\Prime Volume"));
            }
            if parts.iter().any(|p| p.trim() != *p) {
                return Err(format!("the schedule field {name} has spaces at the ends of a folder"));
            }
            if parts[0].eq_ignore_ascii_case("Scheduled") {
                return Err(format!("{name} is written under Scheduled already: name it from below, {}", parts[1..].join("\\")));
            }
            if matches!(kind, Some(zd_reserves::FieldKind::Volume | zd_reserves::FieldKind::Thickness)) {
                return Err(format!("{name} is the unit's own volume or thickness, which every stage writes: a schedule field is another kind (other), worked or read from the volume"));
            }
        }
        Ok(())
    }

    /// Whether an add-on holds together (Table setup plan, decision 2): its fields each named once, its words and
    /// schedule fields as a dataflow's, and its steps named once, each a formula, an allocation or a table script, the
    /// table scripts last. It lists no add-ons of its own.
    pub fn check_add_on(&self) -> Result<(), String> {
        for (i, f) in self.fields.iter().enumerate() {
            if f.field.name.trim().is_empty() {
                return Err(format!("field {} has no name", i + 1));
            }
            if self.fields[..i].iter().any(|g| g.field.name == f.field.name) {
                return Err(format!("the field {} is listed twice", f.field.name));
            }
        }
        self.check_words()?;
        self.check_schedule_fields()?;
        if !self.add_ons.is_empty() {
            return Err("an add-on runs with a main dataflow and lists no add-ons of its own".into());
        }
        let mut table_seen = false;
        for (i, s) in self.steps.iter().enumerate() {
            if s.step.trim().is_empty() {
                return Err(format!("step {} has no name", i + 1));
            }
            if self.steps[..i].iter().any(|t| t.step == s.step) {
                return Err(format!("the step {} is listed twice", s.step));
            }
            if s.is_built_in() {
                return Err(format!("{} is a built-in step: an add-on holds formula, allocation and table script steps, and runs with a main dataflow's", s.step));
            }
            if s.table.is_some() {
                table_seen = true;
            } else if table_seen {
                return Err(format!("the step {} comes after a table script step: table scripts run last, after the final writes", s.step));
            }
            if s.script().is_some_and(|x| x.trim().is_empty()) {
                return Err(format!("the step {} has no script", s.step));
            }
        }
        Ok(())
    }

    /// The main dataflow with its add-ons put in (Table setup plan, decision 2), each by its name, in order: their
    /// formula and allocation steps before `final`, after the main's own there; their table script steps last; their
    /// fields, words, parameter tables, scripts and lookups after the main's. A field or word both have must be the same
    /// in both; a step name both have is refused; so is an add-on with steps before `final` on a dataflow without `rom`
    /// and `final`.
    pub fn with_add_ons(&self, add_ons: &[(&str, &Dataflow)]) -> Result<Dataflow, String> {
        let mut d = self.clone();
        d.add_ons.clear();
        for (name, a) in add_ons {
            if !a.add_on {
                return Err(format!("{name} is a main dataflow, not an add-on"));
            }
            for f in &a.fields {
                match d.fields.iter().find(|g| g.field.name == f.field.name) {
                    Some(g) if g == f => {}
                    Some(_) => return Err(format!("{name}'s field {} isn't the same as the one already there: give one another name, or make them the same", f.field.name)),
                    None => d.fields.push(f.clone()),
                }
            }
            for w in &a.words {
                match d.words.iter().find(|v| v.name == w.name) {
                    Some(v) if v == w => {}
                    Some(_) => return Err(format!("{name}'s word field {} isn't the same as the one already there", w.name)),
                    None => d.words.push(w.clone()),
                }
            }
            for id in &a.parameters {
                if !d.parameters.contains(id) {
                    d.parameters.push(*id);
                }
            }
            for id in &a.scripts {
                if !d.scripts.contains(id) {
                    d.scripts.push(*id);
                }
            }
            for id in &a.lookups {
                if !d.lookups.contains(id) {
                    d.lookups.push(*id);
                }
            }
            for s in &a.steps {
                if d.steps.iter().any(|t| t.step == s.step) {
                    return Err(format!("{name}'s step {} has the name of a step already run: give it its own", s.step));
                }
                if s.table.is_some() {
                    d.steps.push(s.clone());
                    continue;
                }
                let Some(at) = d.steps.iter().position(|t| t.step == "final" && t.is_built_in()) else {
                    return Err(format!("{name}'s step {} runs before final, and the dataflow has no final", s.step));
                };
                if !d.steps[..at].iter().any(|t| t.step == "rom" && t.is_built_in()) {
                    return Err(format!("{name}'s step {} runs after rom, and the dataflow has no rom before final", s.step));
                }
                d.steps.insert(at, s.clone());
            }
        }
        Ok(d)
    }

    /// How many of the levels make a column: those above `seam`, else all but the last.
    pub fn column_levels(&self) -> usize {
        self.levels.iter().position(|l| l.attribute.eq_ignore_ascii_case("seam")).unwrap_or(self.levels.len().saturating_sub(1))
    }

    /// The fields as the column engine checks them: names unique, one volume and one thickness, each summed, every mass
    /// average naming a density.
    pub fn engine_fields(&self) -> Result<Fields, String> {
        Fields::new(self.fields.iter().map(|f| f.field.clone()).collect()).map_err(|e| e.to_string())
    }

    /// Whether the dataflow holds together: its fields, and its steps named, each once.
    pub fn check(&self) -> Result<(), String> {
        if self.add_on {
            return self.check_add_on();
        }
        self.engine_fields()?;
        self.check_words()?;
        self.check_schedule_fields()?;
        if self.levels.is_empty() {
            return Err("the dataflow has no levels".into());
        }
        for (i, l) in self.levels.iter().enumerate() {
            if l.attribute.trim().is_empty() {
                return Err(format!("level {} names no attribute", i + 1));
            }
            if self.levels[..i].iter().any(|m| m.attribute.eq_ignore_ascii_case(&l.attribute)) {
                return Err(format!("the level {} is listed twice", l.attribute));
            }
        }
        for (i, s) in self.steps.iter().enumerate() {
            if s.step.trim().is_empty() {
                return Err(format!("step {} has no name", i + 1));
            }
            if self.steps[..i].iter().any(|t| t.step == s.step) {
                return Err(format!("the step {} is listed twice", s.step));
            }
            if let Some(a) = &s.allocation {
                if s.formula.is_some() || s.table.is_some() {
                    return Err(format!("the step {} is an allocation step and a formula or table script too: make them steps of their own", s.step));
                }
                if a.script.trim().is_empty() {
                    return Err(format!("the allocation step {} has no script", s.step));
                }
                if !s.settings.is_empty() {
                    return Err(format!("the allocation step {} has settings: its script reads what it needs from the parameters", s.step));
                }
            }
            if let Some(t) = &s.table {
                if s.formula.is_some() {
                    return Err(format!("the step {} is both a formula and a table script: make them two steps", s.step));
                }
                if t.script.trim().is_empty() {
                    return Err(format!("the table script step {} has no script", s.step));
                }
                if !s.settings.is_empty() {
                    return Err(format!("the table script step {} has settings: a script reads what it needs itself", s.step));
                }
            } else if self.steps[..i].iter().any(|t| t.table.is_some()) {
                return Err(format!("the step {} comes after a table script step: table scripts run last, after the final writes", s.step));
            }
            if let Some(f) = &s.formula {
                if f.script.trim().is_empty() {
                    return Err(format!("the formula step {} has no script", s.step));
                }
                if !s.settings.is_empty() {
                    return Err(format!("the formula step {} has settings: a formula reads what it needs itself", s.step));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use zd_reserves::{AddUp, FieldKind};

    use super::*;

    fn field(name: &str, kind: FieldKind, add_up: AddUp, source: FieldSource) -> DataflowField {
        DataflowField { field: Field::new(name, kind, add_up), source }
    }

    fn dataflow() -> Dataflow {
        Dataflow {
            levels: default_levels(),
            fields: vec![
                field("volume", FieldKind::Volume, AddUp::Sum, FieldSource::Volume),
                field("thickness", FieldKind::Thickness, AddUp::Sum, FieldSource::Thickness),
                field("area", FieldKind::Area, AddUp::Max, FieldSource::PlanArea),
                field("rd", FieldKind::Density, AddUp::Volume, FieldSource::Attribute { attribute: "rd".into() }),
                field("ash", FieldKind::Quality, AddUp::Mass { density: "rd".into() }, FieldSource::Attribute { attribute: "as".into() }),
                field("ash_cutoff", FieldKind::Setting, AddUp::Min, FieldSource::Parameter { column: "ash_cutoff".into() }),
            ],
            steps: vec![DataflowStep::on("initialise"), DataflowStep { on: false, ..DataflowStep::on("in_situ") }],
            parameters: vec![ObjectId::new()],
            scripts: Vec::new(),
            lookups: Vec::new(),
            words: Vec::new(),
            add_on: false,
            add_ons: Vec::new(),
        }
    }

    /// Word fields (Waste allocation plan, decision 2): saved and read back; each named once, not as a number field or a
    /// unit's own label, and from an attribute, a parameter or a step.
    #[test]
    fn word_fields_hold_together() {
        let mut d = dataflow();
        d.words = vec![WordField::new("process", FieldSource::Worked), WordField::new("method", FieldSource::Parameter { column: "method".into() }), WordField::new("pit_area", FieldSource::Attribute { attribute: "area_code".into() })];
        d.check().unwrap();
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains(r#""words":[{"name":"process","from":"worked"}"#), "{json}");
        assert_eq!(serde_json::from_str::<Dataflow>(&json).unwrap(), d);
        assert!(!serde_json::to_string(&dataflow()).unwrap().contains("words"), "none written where it has none");
        let said = |words: Vec<WordField>| {
            let mut e = dataflow();
            e.words = words;
            e.check().unwrap_err()
        };
        assert!(said(vec![WordField::new("process", FieldSource::Worked), WordField::new("Process", FieldSource::Worked)]).contains("listed twice"));
        assert!(said(vec![WordField::new("rd", FieldSource::Worked)]).contains("a number field and a word field"));
        assert!(said(vec![WordField::new("seam", FieldSource::Worked)]).contains("labels a unit by already"));
        assert!(said(vec![WordField::new("process", FieldSource::Volume)]).contains("not a solid's measure"));
        assert!(said(vec![WordField::new(" process", FieldSource::Worked)]).contains("spaces at its ends"));
    }

    /// Schedule fields (Waste allocation plan, decision 5): a number field and a word field named with folders hold
    /// together; an empty folder, spaces at a folder's ends, a name starting at Scheduled, and the unit's own volume named
    /// so are each refused.
    #[test]
    fn schedule_fields_hold_together() {
        let schedule = |name: &str, kind: FieldKind| field(name, kind, AddUp::Sum, FieldSource::Worked);
        let mut d = dataflow();
        d.fields.push(schedule("Waste\\Dragline\\Prime Volume", FieldKind::Other));
        d.words.push(WordField::new("Waste\\Dragline\\Method", FieldSource::Worked));
        d.check().unwrap();
        assert!(is_schedule_field("Waste\\Dragline\\Prime Volume") && !is_schedule_field("volume"));
        let said = |name: &str, kind: FieldKind| {
            let mut e = dataflow();
            e.fields.push(schedule(name, kind));
            e.check().unwrap_err()
        };
        assert!(said("Waste\\\\Prime Volume", FieldKind::Other).contains("an empty folder"), "{}", said("Waste\\\\Prime Volume", FieldKind::Other));
        assert!(said("Waste\\ Dragline\\Prime Volume", FieldKind::Other).contains("spaces at the ends"));
        assert!(said("Scheduled\\Waste\\Dragline\\Prime Volume", FieldKind::Other).contains("name it from below, Waste\\Dragline\\Prime Volume"));
        let mut e = dataflow();
        e.fields.retain(|f| f.field.kind != FieldKind::Volume);
        e.fields.insert(0, schedule("Waste\\Dragline\\volume", FieldKind::Volume));
        assert!(e.check().unwrap_err().contains("own volume or thickness"));
        let mut e = dataflow();
        e.words.push(WordField::new("\\Method", FieldSource::Worked));
        assert!(e.check().unwrap_err().contains("an empty folder"));
    }

    #[test]
    fn a_dataflow_saves_as_plain_json_and_reads_back() {
        let d = dataflow();
        d.check().unwrap();
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains(r#"{"name":"ash","kind":"quality","add_up":{"by":"mass","density":"rd"},"from":"attribute","attribute":"as"}"#), "{json}");
        assert!(json.contains(r#"{"step":"initialise"}"#), "a step that's on says only its name: {json}");
        let back: Dataflow = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn a_dataflow_that_doesn_t_hold_together_is_refused() {
        let mut d = dataflow();
        d.fields.remove(1);
        assert!(d.check().unwrap_err().contains("thickness"));
        let mut d = dataflow();
        d.steps.push(DataflowStep::on("initialise"));
        assert!(d.check().unwrap_err().contains("listed twice"));
    }

    /// An allocation step (Waste allocation plan, decision 4) saves with its script and reads back; with no script, with
    /// settings, or as a formula too, it's refused.
    #[test]
    fn an_allocation_step_holds_together() {
        let mut d = dataflow();
        d.steps.push(DataflowStep::allocation("loose_ground", "def allocate(column, p):
    pass
"));
        d.check().unwrap();
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains(r#""step":"loose_ground","allocation":{"script":"#), "{json}");
        assert_eq!(serde_json::from_str::<Dataflow>(&json).unwrap(), d);
        assert!(d.steps.last().is_some_and(|s| !s.is_built_in() && s.script().is_some_and(|x| x.contains("allocate"))));
        let refused = |change: fn(&mut DataflowStep)| {
            let mut e = d.clone();
            change(e.steps.last_mut().unwrap());
            e.check().unwrap_err()
        };
        assert!(refused(|s| s.allocation.as_mut().unwrap().script = " ".into()).contains("has no script"));
        assert!(refused(|s| _ = s.settings.insert("x".into(), "y".into())).contains("has settings"));
        assert!(refused(|s| s.formula = DataflowStep::formula("f", "def formula(unit):
    pass
").formula).contains("a formula or table script too"));
    }
}
