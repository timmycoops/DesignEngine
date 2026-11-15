//! The reserves dataflow (decisions 0070 and 0072): the fields its units carry, where each field's value comes from, the
//! steps it runs in Spry's order, and the parameter tables it reads. Running one never changes it; what a run makes is
//! a `Table` a case.
//!
//! Built to take more (Tim, 15 November 2026): fields are data, so a new quality or metric is a new field; steps are named,
//! so a new step slots in by name; parameter tables carry any columns.

use serde::{Deserialize, Serialize};
use zd_reserves::{Field, Fields};

use crate::ObjectId;

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

/// One step, by name, on or off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataflowStep {
    pub step: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub on: bool,
}

impl DataflowStep {
    pub fn on(step: &str) -> Self {
        DataflowStep { step: step.to_string(), on: true }
    }
}

/// A dataflow: its fields, its steps in order and the parameter tables it reads, first to last.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dataflow {
    pub fields: Vec<DataflowField>,
    #[serde(default)]
    pub steps: Vec<DataflowStep>,
    /// Parameter table objects, read in this order: a value comes from the first that gives it.
    #[serde(default)]
    pub parameters: Vec<ObjectId>,
}

impl Dataflow {
    /// The fields as the column engine checks them: names unique, one volume and one thickness, each summed, every mass
    /// average naming a density.
    pub fn engine_fields(&self) -> Result<Fields, String> {
        Fields::new(self.fields.iter().map(|f| f.field.clone()).collect()).map_err(|e| e.to_string())
    }

    /// Whether the dataflow holds together: its fields, and its steps named, each once.
    pub fn check(&self) -> Result<(), String> {
        self.engine_fields()?;
        for (i, s) in self.steps.iter().enumerate() {
            if s.step.trim().is_empty() {
                return Err(format!("step {} has no name", i + 1));
            }
            if self.steps[..i].iter().any(|t| t.step == s.step) {
                return Err(format!("the step {} is listed twice", s.step));
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
            fields: vec![
                field("volume", FieldKind::Volume, AddUp::Sum, FieldSource::Volume),
                field("thickness", FieldKind::Thickness, AddUp::Sum, FieldSource::Thickness),
                field("area", FieldKind::Area, AddUp::Max, FieldSource::PlanArea),
                field("rd", FieldKind::Density, AddUp::Volume, FieldSource::Attribute { attribute: "rd".into() }),
                field("ash", FieldKind::Quality, AddUp::Mass { density: "rd".into() }, FieldSource::Attribute { attribute: "as".into() }),
                field("ash_cutoff", FieldKind::Setting, AddUp::Min, FieldSource::Parameter { column: "ash_cutoff".into() }),
            ],
            steps: vec![DataflowStep::on("initialise"), DataflowStep { step: "in_situ".into(), on: false }],
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
