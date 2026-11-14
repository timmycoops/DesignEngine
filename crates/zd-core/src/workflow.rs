//! A workflow: saved steps run again in one go (decision 0065). Each step is one of the engine's commands with its
//! settings, a rule for each input, and a note on why; or a pause, where the run stops for hand work. It's one saved
//! object, so an edit is one revision and undoes in one go, and what each step's last run did is kept in it so a run
//! from step N reads what the earlier steps made.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AttrValue, ObjectId, ObjectKind, RunId};

fn yes() -> bool {
    true
}

fn is_true(on: &bool) -> bool {
    *on
}

/// The steps in order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub steps: Vec<WorkflowStep>,
}

impl Workflow {
    /// A step by its id.
    pub fn step(&self, id: u32) -> Option<&WorkflowStep> {
        self.steps.iter().find(|s| s.id == id)
    }

    /// Where a step is in the order, by its id.
    pub fn position(&self, id: u32) -> Option<usize> {
        self.steps.iter().position(|s| s.id == id)
    }

    /// An id no step has.
    pub fn next_id(&self) -> u32 {
        self.steps.iter().map(|s| s.id).max().map_or(1, |m| m + 1)
    }
}

/// One step. Its id stays with it when steps are inserted or moved, so a rule naming it still finds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowStep {
    pub id: u32,
    /// Why, in the designer's words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// A step switched off is passed over.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub on: bool,
    pub action: StepAction,
    /// What it did the last time it ran, or the run it was captured from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<LastRun>,
}

/// What a step does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StepAction {
    /// A command by its registry name, with its settings (every parameter but its inputs) and a rule for each input.
    Command {
        command: String,
        #[serde(default)]
        settings: Value,
        #[serde(default)]
        inputs: BTreeMap<String, InputRule>,
    },
    /// The run stops here for a look or hand work; Continue runs on from the next step.
    Pause,
}

/// How a step finds an input each time it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRule {
    pub from: Source,
    /// Only objects of this kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ObjectKind>,
    /// Only objects whose attributes have these values.
    #[serde(default, rename = "where", skip_serializing_if = "BTreeMap::is_empty")]
    pub filter: BTreeMap<String, AttrValue>,
    /// The input takes a list of objects; otherwise it takes exactly one.
    #[serde(default)]
    pub many: bool,
}

/// Where a rule looks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// What an earlier step made or changed, by the step's id, still there, in the order it made them.
    Step(u32),
    /// What's on a layer, by name, in the project's order.
    Layer(String),
    /// Objects by name (with the rule's kind): a seam model, a face design.
    Named(String),
    /// These objects, as picked: what a capture falls back on when no other rule finds them.
    Picked(Vec<ObjectId>),
}

/// What a step's last run did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastRun {
    /// The step's own command run, in the run log.
    pub run: RunId,
    /// The workflow run it was part of; none for a step captured from a run made by hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_run: Option<RunId>,
    pub at_unix_ms: u64,
    pub ms: f64,
    /// What it made, as new objects or as the next revisions of what it made the time before, in order.
    #[serde(default)]
    pub made: Vec<ObjectId>,
    /// Other objects it changed: its own inputs, as a string edited or a design saved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revised: Vec<ObjectId>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub deleted: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub warnings: usize,
    /// It was refused, and the run stopped there; the run log holds why.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refused: bool,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl LastRun {
    /// What a later step's rule naming this step finds: what it made, then what else it changed.
    pub fn outputs(&self) -> impl Iterator<Item = ObjectId> + '_ {
        self.made.iter().chain(&self.revised).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Workflow {
        let mut inputs = BTreeMap::new();
        inputs.insert("solids".to_string(), InputRule { from: Source::Step(1), kind: Some(ObjectKind::Solid), filter: [("material".to_string(), AttrValue::Text("coal".into()))].into(), many: true });
        inputs.insert("model".to_string(), InputRule { from: Source::Named("Seams Nov".into()), kind: Some(ObjectKind::SeamModel), filter: BTreeMap::new(), many: false });
        Workflow {
            steps: vec![
                WorkflowStep { id: 1, note: "the strips".into(), on: true, action: StepAction::Command { command: "solid.slice_by_faces".into(), settings: serde_json::json!({ "layer": "strips" }), inputs: BTreeMap::new() }, last: None },
                WorkflowStep { id: 2, note: String::new(), on: false, action: StepAction::Pause, last: None },
                WorkflowStep {
                    id: 4,
                    note: String::new(),
                    on: true,
                    action: StepAction::Command { command: "solid.interrogate".into(), settings: serde_json::json!({}), inputs },
                    last: Some(LastRun { run: RunId::new(), workflow_run: None, at_unix_ms: 7, ms: 1.5, made: vec![ObjectId::new()], revised: vec![], deleted: 0, warnings: 2, refused: false }),
                },
            ],
        }
    }

    #[test]
    fn a_workflow_round_trips_as_json() {
        let w = sample();
        let text = serde_json::to_string(&w).unwrap();
        assert_eq!(serde_json::from_str::<Workflow>(&text).unwrap(), w);
        assert!(text.contains(r#""where":{"material":"coal"}"#), "{text}");
        assert!(text.contains(r#""from":{"named":"Seams Nov"}"#), "{text}");
        assert!(text.contains(r#""kind":"pause""#), "{text}");
    }

    #[test]
    fn steps_are_found_by_id_and_new_ids_are_new() {
        let w = sample();
        assert_eq!(w.position(4), Some(2));
        assert_eq!(w.step(2).map(|s| s.on), Some(false));
        assert_eq!(w.next_id(), 5);
        assert_eq!(Workflow::default().next_id(), 1);
    }
}
