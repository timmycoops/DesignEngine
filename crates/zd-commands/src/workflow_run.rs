//! How a workflow runs (decision 0065): step by step, each step's inputs found by its rules, the check before a cut run
//! first, stopping at a pause or the first refusal; what each step made last time made again as the next revisions of
//! the same objects; and the whole run one undo step.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zd_core::{AttrValue, BASE_CASE, Body, Change, ChangeKind, LastRun, Object, ObjectId, ObjectKind, Project, RunId, StepAction, Workflow};

use crate::builtin::{check_cut_by_polygons, check_cut_by_seams, check_slice};
use crate::workflow::{CASE_ATTRIBUTE, CaseScope, resolve_in, step_params};
use crate::{CommandError, LogEntry, Note, RunControl, RunDone, RunJob, Session, Severity};

/// What happened at one step of a workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcome {
    /// Its command ran, and what it made was applied.
    Ran,
    /// Switched off, or the pause the run started from, so passed over.
    Off,
    /// A pause: the run stopped here.
    Paused,
    /// Refused: an input its rule couldn't find, the check before a cut, or the command itself. The run stopped here.
    Refused,
}

/// One step of a workflow run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepRun {
    /// The step's id.
    pub step: u32,
    /// Where it is in the order, from 1.
    pub position: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub outcome: StepOutcome,
    /// Its command's run in the log, when the command ran, applied or refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunId>,
    /// Why it was refused, when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
    /// What the check before a cut found, when one ran.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checked: Vec<Note>,
    pub ms: f64,
}

/// How a workflow run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkflowStatus {
    /// Every step from where it started ran or was passed over.
    Finished,
    /// Stopped at the pause at this position (from 1); Continue runs on from the next.
    Paused { at: usize },
    /// Stopped at the step at this position (from 1), which was refused.
    Refused { at: usize },
}

/// The record of a workflow run, in the run log after its steps' own runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRecord {
    pub run: RunId,
    pub workflow: ObjectId,
    pub name: String,
    /// The case it ran (decision 0066): Base, or one of the workflow's cases.
    #[serde(default = "base_case")]
    pub case: String,
    /// The position it ran from, from 1.
    pub from: usize,
    pub steps: Vec<StepRun>,
    #[serde(flatten)]
    pub status: WorkflowStatus,
    pub started_unix_ms: u64,
    pub duration_ms: f64,
}

fn base_case() -> String {
    BASE_CASE.to_string()
}

/// A workflow run under way. It goes step by step through [`Session::next_step`] and [`Session::end_step`], each
/// step's command run off the session in between as any run is (decision 0060), and ends with
/// [`Session::end_workflow`].
pub struct WorkflowRun {
    /// The workflow as its case runs it: the case's settings, and its own last runs.
    workflow: Workflow,
    /// The workflow as it's saved, to keep the case's last runs in at the end.
    saved: Workflow,
    /// What the case sees, and the suffix of its layers.
    scope: CaseScope,
    next: usize,
    record: WorkflowRecord,
    since: usize,
    clock: Instant,
    over: bool,
    /// The step whose command is out, and when it went.
    out: Option<(usize, Instant)>,
}

impl WorkflowRun {
    /// The record so far.
    pub fn record(&self) -> &WorkflowRecord {
        &self.record
    }

    /// The workflow as it stands in the run, each step's last run brought up to date as it runs.
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    /// The position (from 1) of the step whose command is out, if one is.
    pub fn running(&self) -> Option<usize> {
        self.out.map(|(at, _)| at + 1)
    }

    fn stop(&mut self, status: WorkflowStatus) {
        self.record.status = status;
        self.over = true;
    }
}

/// The checks a cut has, run before it: what it would refuse, found with nothing cut (0049, 0062, 0063).
fn check_before(project: &Project, command: &str, params: &Value) -> Option<Result<Vec<Note>, CommandError>> {
    fn read<P: serde::de::DeserializeOwned>(params: &Value) -> Result<P, CommandError> {
        serde_json::from_value(params.clone()).map_err(|e| CommandError::BadParams(e.to_string()))
    }
    Some(match command {
        "solid.slice_by_faces" => read(params).and_then(|p| check_slice(project, &p)),
        "solid.cut_by_polygons" => read(params).and_then(|p| check_cut_by_polygons(project, &p)),
        "solid.cut_by_seams" => read(params).and_then(|p| check_cut_by_seams(project, &p)),
        _ => return None,
    })
}

/// A step's command to run off the session: the check before it first, if it's a cut, then the command if nothing
/// stops it.
pub struct StepJob {
    job: RunJob,
}

/// What a step's job did, to hand to [`Session::end_step`].
pub struct StepDone {
    checked: Option<Result<Vec<Note>, CommandError>>,
    done: Option<RunDone>,
}

impl StepJob {
    pub fn control(&self) -> Arc<RunControl> {
        self.job.control()
    }

    pub fn command(&self) -> &str {
        self.job.command()
    }

    /// Runs the check before a cut, then the command if the check found nothing to fix.
    pub fn run(self) -> StepDone {
        let checked = check_before(self.job.project(), self.job.command(), self.job.params());
        let passed = match &checked {
            None => true,
            Some(Ok(notes)) => notes.iter().all(|n| n.severity < Severity::Warning),
            Some(Err(_)) => false,
        };
        StepDone { checked, done: passed.then(|| self.job.run()) }
    }
}

/// Makes what a step makes again as the next revisions of what it made last time (decision 0065). Each object it
/// creates is matched, by layer, name and kind, in order, to one it made last time that's still there, and becomes that
/// object's next revision; what's left is matched by layer and kind alone, in order, as when an input renamed renames
/// what's made from it. Where the command deletes one of last time's and makes it again, the delete goes and the object
/// carries on; what it made last time and doesn't make again is deleted. Returns the ids made again.
pub(crate) fn make_again(project: &Project, changes: &mut Vec<Change>, notes: &mut Vec<Note>, last_made: &[ObjectId]) -> HashSet<ObjectId> {
    let revising: HashSet<ObjectId> = changes
        .iter()
        .filter_map(|c| match c {
            Change::Revise { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    let deleting: HashSet<ObjectId> = changes
        .iter()
        .filter_map(|c| match c {
            Change::Delete { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    let layer_name = |o: &Object| project.layer(o.layer).map(|l| l.name.clone()).unwrap_or_default();
    let mut pool: HashMap<(String, String, ObjectKind), VecDeque<(ObjectId, u32)>> = HashMap::new();
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    for &id in last_made {
        if !seen.insert(id) || revising.contains(&id) {
            continue;
        }
        if let Some(o) = project.object(id) {
            pool.entry((layer_name(o), o.name.clone(), o.kind())).or_default().push_back((id, o.revision));
            order.push((id, o.revision));
        }
    }
    let mut again = HashSet::new();
    let mut matched: Vec<Option<(ObjectId, u32)>> = changes
        .iter()
        .map(|change| match change {
            Change::Create { name, layer, body, .. } => pool.get_mut(&(layer.clone(), name.clone(), body.kind())).and_then(VecDeque::pop_front),
            _ => None,
        })
        .collect();
    // What's left, by layer and kind alone.
    let mut left: HashMap<(String, ObjectKind), VecDeque<(ObjectId, u32)>> = HashMap::new();
    for ((layer, _, kind), queue) in &pool {
        left.entry((layer.clone(), *kind)).or_default().extend(queue.iter().copied());
    }
    let place: HashMap<ObjectId, usize> = order.iter().enumerate().map(|(k, (id, _))| (*id, k)).collect();
    for queue in left.values_mut() {
        queue.make_contiguous().sort_by_key(|(id, _)| place[id]);
    }
    for (change, matched) in changes.iter().zip(matched.iter_mut()) {
        if let (Change::Create { layer, body, .. }, None) = (change, &matched) {
            *matched = left.get_mut(&(layer.clone(), body.kind())).and_then(VecDeque::pop_front);
        }
    }
    for (change, matched) in changes.iter_mut().zip(matched) {
        let (Change::Create { name, attributes, body, .. }, Some((id, base))) = (&mut *change, matched) else { continue };
        again.insert(id);
        *change = Change::Revise { id, base, name: Some(std::mem::take(name)), layer: None, attributes: Some(std::mem::take(attributes)), body: Some(body.clone()) };
    }
    // The command's own deletes of what it makes again go; a note on a change is moved to where the change now is.
    let mut moved = Vec::with_capacity(changes.len());
    let mut kept = Vec::with_capacity(changes.len());
    for change in changes.drain(..) {
        let dropped = matches!(&change, Change::Delete { id, .. } if again.contains(id));
        moved.push((!dropped).then_some(kept.len()));
        if !dropped {
            kept.push(change);
        }
    }
    *changes = kept;
    for note in notes.iter_mut() {
        if let Some(index) = note.made {
            note.made = moved.get(index).copied().flatten();
        }
    }
    let mut gone = 0;
    for (id, base) in order {
        if !again.contains(&id) && !deleting.contains(&id) {
            changes.push(Change::Delete { id, base });
            gone += 1;
        }
    }
    let new = changes.iter().filter(|c| matches!(c, Change::Create { .. })).count();
    if !again.is_empty() || gone > 0 {
        notes.push(Note::info(format!(
            "made again as the next revisions of {} object{} it made last time; {new} new; {gone} it made last time and not this time deleted",
            again.len(),
            if again.len() == 1 { "" } else { "s" }
        )));
    }
    again
}

/// A step's last run marked refused. What it made before stays its last, so the run after the fix makes it again as
/// revisions.
fn refused_last(run: &mut WorkflowRun, at: usize, command_run: Option<RunId>) {
    let workflow_run = run.record.run;
    let last = run.workflow.steps[at].last.get_or_insert_with(|| LastRun { run: RunId::new(), workflow_run: None, at_unix_ms: 0, ms: 0.0, made: Vec::new(), revised: Vec::new(), deleted: 0, warnings: 0, refused: true });
    last.workflow_run = Some(workflow_run);
    last.at_unix_ms = now_ms();
    if let Some(r) = command_run {
        last.run = r;
    }
    last.refused = true;
}

/// Every layer a step's parameters write to, by name, taken to the case's own (decision 0066): a `layer` holding a
/// name, at any depth (a face design's own `layer` too), and never one holding a layer's id.
fn case_layers(value: &mut Value, suffix: &str) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields.iter_mut() {
                match field {
                    Value::String(name) if key == "layer" => {
                        if !name.is_empty() && ObjectId::parse(name).is_none() {
                            name.push_str(suffix);
                        }
                    }
                    other => case_layers(other, suffix),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| case_layers(item, suffix)),
        _ => {}
    }
}

/// What a step makes, tagged with the case that made it (decision 0066): what it creates and what it makes again,
/// never the inputs it changes.
fn tag_case(changes: &mut [Change], again: &HashSet<ObjectId>, case: &str) {
    let tag = || AttrValue::Text(case.to_string());
    for change in changes.iter_mut() {
        match change {
            Change::Create { attributes, .. } => {
                attributes.insert(CASE_ATTRIBUTE.to_string(), tag());
            }
            Change::Revise { id, attributes: Some(attributes), .. } if again.contains(id) => {
                attributes.insert(CASE_ATTRIBUTE.to_string(), tag());
            }
            _ => {}
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Session {
    /// Starts a run of a workflow from the step at `from` (from 1). Steps before it aren't run: a rule taking what one
    /// of them made reads its last run.
    pub fn begin_workflow(&self, workflow: ObjectId, from: usize) -> Result<WorkflowRun, CommandError> {
        self.begin_case(workflow, from, BASE_CASE)
    }

    /// [`Session::begin_workflow`] for one of its cases (decision 0066): the steps with the case's settings, its layers
    /// named `<layer> <case>`, what it makes tagged `case`, and its rules seeing what it made and what no step made,
    /// never another case's.
    pub fn begin_case(&self, workflow: ObjectId, from: usize, case: &str) -> Result<WorkflowRun, CommandError> {
        let object = self.project().object(workflow).ok_or(CommandError::MissingObject(workflow))?;
        let Body::Workflow(content) = &object.body else {
            return Err(CommandError::WrongKind { id: workflow, expected: ObjectKind::Workflow, found: object.kind() });
        };
        if from == 0 || from > content.steps.len() {
            return Err(CommandError::Invalid(format!("'{}' has {} steps, so it can't run from step {from}", object.name, content.steps.len())));
        }
        let view = content.as_case(case).ok_or_else(|| CommandError::Invalid(format!("'{}' has no case named '{case}'", object.name)))?;
        // What the other cases made, Base's included, is out of this case's sight.
        let mut others: HashSet<ObjectId> = HashSet::new();
        if case != BASE_CASE {
            others.extend(content.steps.iter().filter_map(|s| s.last.as_ref()).flat_map(|l| l.made.iter().copied()));
        }
        for other in content.cases.iter().filter(|c| c.name != case) {
            others.extend(other.last.values().flat_map(|l| l.made.iter().copied()));
        }
        let suffix = if case == BASE_CASE { String::new() } else { format!(" {case}") };
        Ok(WorkflowRun {
            workflow: view,
            saved: (**content).clone(),
            scope: CaseScope { case: Some(case.to_string()), others, suffix },
            next: from - 1,
            record: WorkflowRecord { run: RunId::new(), workflow, name: object.name.clone(), case: case.to_string(), from, steps: Vec::new(), status: WorkflowStatus::Finished, started_unix_ms: now_ms(), duration_ms: 0.0 },
            since: self.undo_steps(),
            clock: Instant::now(),
            over: false,
            out: None,
        })
    }

    /// The next step's command, its inputs found by their rules, to run off the session; `None` once the run has
    /// finished, paused or been refused. A pause stops the run unless the run starts from it; a step switched off is
    /// passed over.
    pub fn next_step(&self, run: &mut WorkflowRun) -> Option<StepJob> {
        while !run.over && run.next < run.workflow.steps.len() {
            let at = run.next;
            run.next += 1;
            let step = &run.workflow.steps[at];
            let started = Instant::now();
            let said = |outcome, command: Option<&String>| StepRun { step: step.id, position: at + 1, command: command.cloned(), outcome, run: None, refused: None, checked: Vec::new(), ms: 0.0 };
            match &step.action {
                _ if !step.on => run.record.steps.push(said(StepOutcome::Off, None)),
                StepAction::Pause if at + 1 == run.record.from => run.record.steps.push(said(StepOutcome::Off, None)),
                StepAction::Pause => {
                    let paused = said(StepOutcome::Paused, None);
                    run.record.steps.push(paused);
                    run.stop(WorkflowStatus::Paused { at: at + 1 });
                }
                StepAction::Command { command, settings, inputs } => {
                    let mut found = BTreeMap::new();
                    let mut refused = None;
                    for (field, rule) in inputs {
                        match resolve_in(self.project(), &run.workflow, at, rule, &run.scope) {
                            Ok(ids) => {
                                found.insert(field.clone(), (rule.many, ids));
                            }
                            Err(why) => {
                                refused = Some(format!("its input '{field}' {why}"));
                                break;
                            }
                        }
                    }
                    if let Some(why) = refused {
                        let mut stopped = said(StepOutcome::Refused, Some(command));
                        stopped.refused = Some(why);
                        stopped.ms = started.elapsed().as_secs_f64() * 1e3;
                        run.record.steps.push(stopped);
                        refused_last(run, at, None);
                        run.stop(WorkflowStatus::Refused { at: at + 1 });
                        return None;
                    }
                    let mut params = step_params(settings, &found);
                    if !run.scope.suffix.is_empty() {
                        case_layers(&mut params, &run.scope.suffix);
                    }
                    let job = StepJob { job: self.begin_run(command, params) };
                    run.out = Some((at, started));
                    return Some(job);
                }
            }
        }
        run.over = true;
        None
    }

    /// Applies what a step did, making again as revisions what it made last time, and brings its last run up to date.
    /// A refusal, by its check or its command, stops the run there.
    pub fn end_step(&mut self, run: &mut WorkflowRun, done: StepDone) {
        let Some((at, started)) = run.out.take() else { return };
        let StepAction::Command { command, .. } = &run.workflow.steps[at].action else { return };
        let mut said = StepRun { step: run.workflow.steps[at].id, position: at + 1, command: Some(command.clone()), outcome: StepOutcome::Ran, run: None, refused: None, checked: Vec::new(), ms: 0.0 };
        let last_made: Vec<ObjectId> = run.workflow.steps[at].last.as_ref().map(|l| l.made.clone()).unwrap_or_default();
        let StepDone { checked, done } = done;
        match checked {
            Some(Ok(notes)) => said.checked = notes,
            Some(Err(e)) => said.refused = Some(format!("the check before the cut refused it: {e}")),
            None => {}
        }
        let ended = done.map(|done| {
            let mut again = HashSet::new();
            let case = run.record.case.clone();
            let ended = self.end_run_with(done, |project, changes, notes| {
                again = make_again(project, changes, notes, &last_made);
                tag_case(changes, &again, &case);
            });
            (ended, again)
        });
        match ended {
            Some((Ok(record), again)) => {
                said.run = Some(record.run);
                let (mut made, mut revised, mut deleted) = (Vec::new(), Vec::new(), 0);
                for change in &record.outputs {
                    match change.change {
                        ChangeKind::Created => made.push(change.id),
                        ChangeKind::Revised if again.contains(&change.id) => made.push(change.id),
                        ChangeKind::Revised => revised.push(change.id),
                        ChangeKind::Deleted => deleted += 1,
                    }
                }
                run.workflow.steps[at].last = Some(LastRun {
                    run: record.run,
                    workflow_run: Some(run.record.run),
                    at_unix_ms: record.started_unix_ms,
                    ms: record.duration_ms,
                    made,
                    revised,
                    deleted,
                    warnings: record.notes.iter().filter(|n| n.severity >= Severity::Warning).count(),
                    refused: false,
                });
            }
            refusal => {
                match refusal {
                    Some((Err(e), _)) => {
                        said.refused = Some(e.to_string());
                        said.run = self.log().iter().rev().find_map(|e| match e {
                            LogEntry::Run(r) => Some(r.run),
                            _ => None,
                        });
                    }
                    _ if said.refused.is_none() => {
                        let found = said.checked.iter().filter(|n| n.severity >= Severity::Warning).count();
                        said.refused = Some(format!("the check before the cut found {found} thing{} to fix first", if found == 1 { "" } else { "s" }));
                    }
                    _ => {}
                }
                said.outcome = StepOutcome::Refused;
                refused_last(run, at, said.run);
                run.stop(WorkflowStatus::Refused { at: at + 1 });
            }
        }
        said.ms = started.elapsed().as_secs_f64() * 1e3;
        run.record.steps.push(said);
    }

    /// Ends a workflow run: saves each step's last run in the workflow (a `workflow.save` run), makes everything the
    /// run applied one undo step, and logs the run after its steps' own runs.
    pub fn end_workflow(&mut self, mut run: WorkflowRun) -> WorkflowRecord {
        run.over = true;
        let id = run.record.workflow;
        if let Some(name) = self.project().object(id).filter(|o| matches!(o.body, Body::Workflow(_))).map(|o| o.name.clone()) {
            run.saved.keep_case_runs(&run.record.case, &run.workflow);
            let params = serde_json::json!({ "workflow": id, "name": name, "content": run.saved });
            // It saved as it reads, so it saves again; were it refused, the steps' work would stay applied all the same.
            let _ = self.run("workflow.save", params);
        }
        self.join_undo(run.since, run.record.run);
        run.record.duration_ms = run.clock.elapsed().as_secs_f64() * 1e3;
        self.log_workflow(run.record.clone());
        run.record
    }

    /// Runs a workflow from the step at `from` (from 1) to its end, a pause or the first refusal, as one undo step.
    pub fn run_workflow(&mut self, workflow: ObjectId, from: usize) -> Result<WorkflowRecord, CommandError> {
        self.run_case(workflow, from, BASE_CASE)
    }

    /// [`Session::run_workflow`] for one of its cases (decision 0066).
    pub fn run_case(&mut self, workflow: ObjectId, from: usize, case: &str) -> Result<WorkflowRecord, CommandError> {
        let mut run = self.begin_case(workflow, from, case)?;
        while let Some(job) = self.next_step(&mut run) {
            let done = job.run();
            self.end_step(&mut run, done);
        }
        Ok(self.end_workflow(run))
    }

    /// The workflow's cases in the order Run all cases runs them: Base, then each case as listed.
    pub fn case_names(&self, workflow: ObjectId) -> Result<Vec<String>, CommandError> {
        let content = self.workflow(workflow)?;
        Ok(std::iter::once(BASE_CASE.to_string()).chain(content.cases.iter().map(|c| c.name.clone())).collect())
    }

    /// Runs every case from its first step, Base first, as one undo step; a pause or a refusal stops it at that case.
    pub fn run_all_cases(&mut self, workflow: ObjectId) -> Result<Vec<WorkflowRecord>, CommandError> {
        let names = self.case_names(workflow)?;
        let since = self.undo_steps();
        let mut records = Vec::new();
        for name in names {
            let record = self.run_case(workflow, 1, &name)?;
            let stop = record.status != WorkflowStatus::Finished;
            records.push(record);
            if stop {
                break;
            }
        }
        if let Some(first) = records.first() {
            self.join_undo(since, first.run);
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_case_writes_to_its_own_layers_by_name_never_to_another_layer_by_id() {
        let toes = ObjectId::new().to_string();
        let mut params = serde_json::json!({
            "layer": "8 blocks",
            "name": "B#",
            "content": { "layer": "2 faces", "lines": [{ "layer": toes, "profile": "Highwall" }] },
            "empty": { "layer": "" },
        });
        case_layers(&mut params, " 50m");
        assert_eq!(params["layer"], "8 blocks 50m");
        assert_eq!(params["name"], "B#");
        assert_eq!(params["content"]["layer"], "2 faces 50m");
        assert_eq!(params["content"]["lines"][0]["layer"], serde_json::json!(toes), "a layer held by its id is left as it is");
        assert_eq!(params["empty"]["layer"], "");
    }
}
