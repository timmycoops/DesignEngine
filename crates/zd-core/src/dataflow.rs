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
        DataflowStep { step: step.to_string(), on: true, settings: BTreeMap::new(), formula: None }
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
}

impl Default for Dataflow {
    fn default() -> Self {
        Dataflow { levels: default_levels(), fields: Vec::new(), steps: Vec::new(), parameters: Vec::new() }
    }
}

impl Dataflow {
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
        self.engine_fields()?;
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
        }
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
}
