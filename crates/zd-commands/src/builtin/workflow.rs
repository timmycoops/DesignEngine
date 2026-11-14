//! `workflow.save`: a workflow saved, new or as the next revision of one (decision 0065). Capture, Add to workflow and
//! every edit in the Workflow panel save through it, so each is logged and undone like any run.

use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{Attributes, BASE_CASE, Body, Change, ObjectId, ObjectKind, Source, StepAction, Workflow};

use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome};

pub struct SaveWorkflow;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveWorkflowParams {
    /// The workflow to revise; a new one if left out.
    #[serde(default)]
    pub workflow: Option<ObjectId>,
    pub name: String,
    pub content: Workflow,
}

/// The layer workflows sit on.
pub const WORKFLOW_LAYER: &str = "workflows";

impl Command for SaveWorkflow {
    type Params = SaveWorkflowParams;
    const NAME: &'static str = "workflow.save";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Save a workflow: its steps, their settings, inputs by rule and notes",
            inputs: vec![],
            params: vec![
                Field::new("workflow", FieldType::Text, "The workflow to revise; a new one if left out"),
                Field::new("name", FieldType::Text, "Name of the workflow"),
                Field::new("content", FieldType::Text, "The steps"),
            ],
            outputs: vec![Field::object("workflow", ObjectKind::Workflow, "The saved workflow")],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let notes = check_steps(&p.content)?;
        let body = Body::Workflow(Arc::new(p.content.clone()));
        let change = match p.workflow {
            Some(id) => {
                let object = inputs.object(id)?;
                if !matches!(object.body, Body::Workflow(_)) {
                    return Err(CommandError::WrongKind { id, expected: ObjectKind::Workflow, found: object.kind() });
                }
                Change::Revise { id, base: object.revision, name: Some(p.name.clone()), layer: None, attributes: None, body: Some(body) }
            }
            None => Change::Create { name: p.name.clone(), layer: WORKFLOW_LAYER.into(), attributes: Attributes::new(), body },
        };
        let steps = p.content.steps.len();
        let mut outcome = Outcome::new(vec![change]).message(format!("saved {} with {steps} step{}", p.name, if steps == 1 { "" } else { "s" }));
        outcome.notes.extend(notes);
        Ok(outcome)
    }
}

/// Refuses steps that can't run as they stand: two with one id, or a rule naming a step that isn't before it. Warns of
/// inputs that are the objects as picked, which no rule finds.
fn check_steps(workflow: &Workflow) -> Result<Vec<Note>, CommandError> {
    let mut seen = HashSet::new();
    let mut notes = Vec::new();
    for (at, step) in workflow.steps.iter().enumerate() {
        if !seen.insert(step.id) {
            return Err(CommandError::Invalid(format!("two steps have the id {}", step.id)));
        }
        let StepAction::Command { command, inputs, settings } = &step.action else { continue };
        if command.trim().is_empty() {
            return Err(CommandError::Invalid(format!("step {} names no command", at + 1)));
        }
        if !(settings.is_object() || settings.is_null()) {
            return Err(CommandError::Invalid(format!("step {}'s settings aren't a set of named values", at + 1)));
        }
        for (field, rule) in inputs {
            match &rule.from {
                Source::Step(id) if !workflow.steps[..at].iter().any(|s| s.id == *id) => {
                    return Err(CommandError::Invalid(format!("step {}'s input '{field}' takes what step id {id} made, which isn't before it", at + 1)));
                }
                Source::Picked(ids) => notes.push(Note::warning(
                    Code::WorkflowPicked,
                    format!("step {} ({command}): its input '{field}' is the {} object{} as picked, as no rule finds {}", at + 1, ids.len(), if ids.len() == 1 { "" } else { "s" }, if ids.len() == 1 { "it" } else { "them" }),
                )),
                Source::Named(name) if name.trim().is_empty() => return Err(CommandError::Invalid(format!("step {}'s input '{field}' names no object", at + 1))),
                Source::Layer(name) if name.trim().is_empty() => return Err(CommandError::Invalid(format!("step {}'s input '{field}' names no layer", at + 1))),
                _ => {}
            }
        }
    }
    // Cases (decision 0066): each named, once, not Base, changing only the settings of steps that are there.
    let mut names = HashSet::new();
    for case in &workflow.cases {
        let name = case.name.trim();
        if name.is_empty() {
            return Err(CommandError::Invalid("a case has no name".into()));
        }
        if name.eq_ignore_ascii_case(BASE_CASE) {
            return Err(CommandError::Invalid(format!("'{BASE_CASE}' is the steps as they stand; a case needs another name")));
        }
        if !names.insert(name.to_lowercase()) {
            return Err(CommandError::Invalid(format!("two cases are named '{name}'")));
        }
        for (id, changes) in &case.settings {
            let Some(step) = workflow.step(*id) else { return Err(CommandError::Invalid(format!("case '{name}' changes step id {id}, which isn't there"))) };
            let StepAction::Command { inputs, .. } = &step.action else { return Err(CommandError::Invalid(format!("case '{name}' changes a pause"))) };
            if let Some(field) = changes.keys().find(|k| inputs.contains_key(*k)) {
                return Err(CommandError::Invalid(format!("case '{name}' changes '{field}', an input found by rule, not a setting")));
            }
        }
    }
    Ok(notes)
}
