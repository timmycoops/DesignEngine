//! Workflows (decision 0065): how a step's inputs are found by rule, and how runs from the run log are captured as
//! steps, their object ids turned into rules.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Map, Value};
use zd_core::{AttrValue, Body, ChangeKind, InputRule, LastRun, Object, ObjectId, ObjectKind, Project, RunId, Source, StepAction, Workflow, WorkflowStep};

use crate::builtin::SaveWorkflowParams;
use crate::{CommandError, LogEntry, RunRecord, RunStatus, Session, Severity};

/// The objects a rule finds now, for the step at `at` in the workflow's order: live ones only, in the order the rule
/// gives. A rule for one object must find exactly one; a rule for many must find some.
pub fn resolve(project: &Project, workflow: &Workflow, at: usize, rule: &InputRule) -> Result<Vec<ObjectId>, String> {
    let keep = |o: &Object| rule.kind.is_none_or(|k| o.kind() == k) && rule.filter.iter().all(|(key, value)| o.attributes.get(key) == Some(value));
    let found: Vec<ObjectId> = match &rule.from {
        Source::Step(id) => {
            let Some(position) = workflow.position(*id).filter(|&p| p < at) else { return Err(format!("takes what step id {id} made, which isn't before it")) };
            let Some(last) = &workflow.steps[position].last else { return Err(format!("takes what step {} made, which hasn't run", position + 1)) };
            let mut seen = HashSet::new();
            last.outputs().filter(|id| seen.insert(*id)).filter(|&id| project.object(id).is_some_and(|o| keep(o))).collect()
        }
        Source::Layer(name) => match project.layer_named(name) {
            Some(layer) => project.objects().filter(|o| o.layer == layer.id && keep(o)).map(|o| o.id).collect(),
            None => Vec::new(),
        },
        Source::Named(name) => project.objects().filter(|o| &o.name == name && keep(o)).map(|o| o.id).collect(),
        Source::Picked(ids) => {
            let gone = ids.iter().filter(|&&id| project.object(id).is_none()).count();
            if gone > 0 {
                return Err(format!("{gone} of the {} objects picked {} gone", ids.len(), if gone == 1 { "is" } else { "are" }));
            }
            ids.iter().copied().filter(|&id| project.object(id).is_some_and(|o| keep(o))).collect()
        }
    };
    match (rule.many, found.len()) {
        (_, 0) => Err(format!("finds nothing: {}", describe(workflow, rule))),
        (false, n) if n > 1 => Err(format!("finds {n} objects, and takes one: {}", describe(workflow, rule))),
        _ => Ok(found),
    }
}

/// A rule in words, as the panel and a refusal say it.
pub fn describe(workflow: &Workflow, rule: &InputRule) -> String {
    let what = rule.kind.map_or("objects".to_string(), |k| format!("{} objects", kind_name(k)));
    let mut said = match &rule.from {
        Source::Step(id) => match workflow.position(*id) {
            Some(p) => format!("the {what} step {} made", p + 1),
            None => format!("the {what} a step no longer there made"),
        },
        Source::Layer(name) => format!("the {what} on layer '{name}'"),
        Source::Named(name) => format!("the {what} named '{name}'"),
        Source::Picked(ids) => format!("the {} {what} as picked", ids.len()),
    };
    for (key, value) in &rule.filter {
        said.push_str(&format!(" where {key} is {}", attr_text(value)));
    }
    said
}

fn kind_name(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::String => "string",
        ObjectKind::Surface => "surface",
        ObjectKind::Solid => "solid",
        ObjectKind::Grid => "grid",
        ObjectKind::SeamModel => "seam model",
        ObjectKind::FaceDesign => "face design",
        ObjectKind::Workflow => "workflow",
    }
}

fn attr_text(value: &AttrValue) -> String {
    match value {
        AttrValue::Bool(b) => b.to_string(),
        AttrValue::Number(n) => n.to_string(),
        AttrValue::Text(t) => t.clone(),
    }
}

/// A step's parameters for a run: its settings, with each input's objects put in as ids.
pub fn step_params(settings: &Value, inputs: &BTreeMap<String, (bool, Vec<ObjectId>)>) -> Value {
    let mut params = settings.as_object().cloned().unwrap_or_default();
    for (field, (many, ids)) in inputs {
        let value = if *many { Value::Array(ids.iter().map(|id| Value::String(id.to_string())).collect()) } else { Value::String(ids[0].to_string()) };
        params.insert(field.clone(), value);
    }
    Value::Object(params)
}

/// Which runs in the log are applied now: logged as done, and not undone since.
pub fn applied_runs(log: &[LogEntry]) -> HashSet<RunId> {
    let (mut applied, mut done, mut undone) = (HashSet::new(), Vec::new(), Vec::new());
    for entry in log {
        match entry {
            LogEntry::Run(r) if r.status == RunStatus::Ok => {
                applied.insert(r.run);
                if !r.outputs.is_empty() || !r.layers_created.is_empty() {
                    done.push(r.run);
                    undone.clear();
                }
            }
            LogEntry::Undo { run, .. } => {
                if let Some(p) = done.iter().rposition(|r| r == run) {
                    done.remove(p);
                }
                applied.remove(run);
                undone.push(*run);
            }
            LogEntry::Redo { run, .. } => {
                if let Some(p) = undone.iter().rposition(|r| r == run) {
                    undone.remove(p);
                }
                applied.insert(*run);
                done.push(*run);
            }
            // The undo history doesn't survive closing a project.
            LogEntry::Opened { .. } => {
                done.clear();
                undone.clear();
            }
            _ => {}
        }
    }
    applied
}

/// The project as it stood at a point in the run log, as far as the log and the revisions still held can tell.
struct Timeline<'a> {
    project: &'a Project,
    /// Each object's changes by applied runs, in log order: where in the log, the revision and what happened.
    events: HashMap<ObjectId, Vec<(usize, u32, ChangeKind)>>,
    /// Every object the log or the project knows, in the order first seen.
    known: Vec<ObjectId>,
    /// Each object's name as the log last gives it: all that's left of one deleted before the project was opened.
    names: HashMap<ObjectId, String>,
}

impl<'a> Timeline<'a> {
    fn new(project: &'a Project, log: &[LogEntry], applied: &HashSet<RunId>) -> Self {
        let mut events: HashMap<ObjectId, Vec<(usize, u32, ChangeKind)>> = HashMap::new();
        let mut known = Vec::new();
        let mut seen = HashSet::new();
        let mut names = HashMap::new();
        for (at, entry) in log.iter().enumerate() {
            let LogEntry::Run(r) = entry else { continue };
            if !applied.contains(&r.run) {
                continue;
            }
            for change in &r.outputs {
                events.entry(change.id).or_default().push((at, change.revision, change.change));
                names.insert(change.id, change.name.clone());
                if seen.insert(change.id) {
                    known.push(change.id);
                }
            }
        }
        for object in project.objects() {
            if seen.insert(object.id) {
                known.push(object.id);
            }
        }
        Timeline { project, events, known, names }
    }

    /// The object as it stood just before the log entry at `at`, if it was there then and can still be read: the
    /// revision it was at if the project still holds it, else as it is now.
    fn before(&self, id: ObjectId, at: usize) -> Option<&'a Object> {
        let events = self.events.get(&id).map(Vec::as_slice).unwrap_or_default();
        let last = events.iter().rev().find(|e| e.0 < at);
        let there = match last {
            Some((_, _, ChangeKind::Deleted)) => false,
            Some(_) => true,
            // Not changed by then: there if it was made before the log began.
            None => events.first().is_none_or(|e| e.2 != ChangeKind::Created),
        };
        if !there {
            return None;
        }
        let at_revision = last.and_then(|&(_, revision, _)| self.project.revision(id, revision));
        at_revision.or_else(|| self.project.object(id)).map(|o| o.as_ref())
    }

    /// Everything there just before the log entry at `at`, in the order first seen.
    fn there_before(&self, at: usize) -> Vec<&'a Object> {
        self.known.iter().filter_map(|&id| self.before(id, at)).collect()
    }
}

/// A captured step's outputs, still there at a later point, in order.
struct Captured {
    id: u32,
    outputs: Vec<ObjectId>,
}

/// The simplest rule that finds exactly `targets` among `there` (what was there when the run read them). A rule by
/// layer or name must find them in `now` too, as they are now: a run again sees the project as it stands then, with what
/// later steps made the time before, as a layer a step's input was alone on that later steps added to, or a toe that
/// numbering has renamed since. `gone_name` is the name the log gives an object that can't be read any more.
fn rule_for(targets: &[ObjectId], many: bool, there: &[&Object], now: &[&Object], earlier: &[Captured], layer_name: impl Fn(&Object) -> Option<String>, gone_name: impl Fn(ObjectId) -> Option<String>) -> InputRule {
    let wanted: HashSet<ObjectId> = targets.iter().copied().collect();
    let by_id: HashMap<ObjectId, &Object> = there.iter().map(|o| (o.id, *o)).collect();
    // What was there then, and what's there now that wasn't.
    let then_and_now: Vec<&Object> = there.iter().copied().chain(now.iter().copied().filter(|o| !by_id.contains_key(&o.id))).collect();
    let picked: Option<Vec<&Object>> = targets.iter().map(|id| by_id.get(id).copied()).collect();
    let rule = |from: Source, kind: Option<ObjectKind>, filter: BTreeMap<String, AttrValue>| InputRule { from, kind, filter, many };
    let Some(picked) = picked else {
        // One deleted since and not held any more: by the name the log gives it, if nothing there then had that name.
        // A step that made it makes it again under that name, as a strip sliced and then deleted is.
        if let [one] = targets
            && let Some(name) = gone_name(*one)
            && !then_and_now.iter().any(|o| o.name == name)
        {
            return rule(Source::Named(name), None, BTreeMap::new());
        }
        return rule(Source::Picked(targets.to_vec()), None, BTreeMap::new());
    };
    let kind = picked.first().map(|o| o.kind()).filter(|k| picked.iter().all(|o| o.kind() == *k));
    let shared: Vec<(String, AttrValue)> = match picked.first() {
        Some(first) => first.attributes.iter().filter(|(key, value)| picked.iter().all(|o| o.attributes.get(key) == Some(value))).map(|(k, v)| (k.to_string(), v.clone())).collect(),
        None => Vec::new(),
    };
    // The filters to try in a pool, simplest first: none, the kind, an attribute they share, then both. An attribute
    // everything in the pool carries comes before one only some carry: it's more likely what made them set it, where an
    // attribute read as it is now may have been added since, as an interrogation adds its own.
    let filters = |pool: &[&Object]| {
        let mut keys: Vec<&(String, AttrValue)> = shared.iter().collect();
        keys.sort_by_key(|(key, _)| !pool.iter().all(|o| o.attributes.contains_key(key)));
        let mut filters: Vec<(Option<ObjectKind>, BTreeMap<String, AttrValue>)> = vec![(None, BTreeMap::new())];
        filters.extend(kind.map(|k| (Some(k), BTreeMap::new())));
        filters.extend(keys.iter().map(|(k, v)| (None, BTreeMap::from([(k.clone(), v.clone())]))));
        if kind.is_some() {
            filters.extend(keys.iter().map(|(k, v)| (kind, BTreeMap::from([(k.clone(), v.clone())]))));
        }
        filters
    };
    let found = |pool: &[&Object], kind: Option<ObjectKind>, filter: &BTreeMap<String, AttrValue>| -> HashSet<ObjectId> {
        pool.iter().filter(|o| kind.is_none_or(|k| o.kind() == k) && filter.iter().all(|(key, value)| o.attributes.get(key) == Some(value))).map(|o| o.id).collect()
    };
    let finds = |pool: &[&Object], kind: Option<ObjectKind>, filter: &BTreeMap<String, AttrValue>| found(pool, kind, filter) == wanted;
    // The targets still there now, which the same rule must find among what's there now, as it is now.
    let alive: HashSet<ObjectId> = now.iter().map(|o| o.id).filter(|id| wanted.contains(id)).collect();
    let finds_now = |pool: &[&Object], kind: Option<ObjectKind>, filter: &BTreeMap<String, AttrValue>| alive.is_empty() || found(pool, kind, filter) == alive;
    // What an earlier step made, the latest first.
    for step in earlier.iter().rev() {
        if !wanted.iter().all(|id| step.outputs.contains(id)) {
            continue;
        }
        let pool: Vec<&Object> = step.outputs.iter().filter_map(|id| by_id.get(id).copied()).collect();
        for (kind, filter) in filters(&pool) {
            if finds(&pool, kind, &filter) {
                return rule(Source::Step(step.id), kind, filter);
            }
        }
    }
    // A layer they all sit on.
    if let Some(layer) = picked.first().map(|o| o.layer).filter(|l| picked.iter().all(|o| o.layer == *l))
        && let Some(name) = layer_name(picked[0])
    {
        let pool: Vec<&Object> = then_and_now.iter().copied().filter(|o| o.layer == layer).collect();
        let pool_now: Vec<&Object> = now.iter().copied().filter(|o| layer_name(o).as_deref() == Some(name.as_str())).collect();
        for (kind, filter) in filters(&pool) {
            if finds(&pool, kind, &filter) && finds_now(&pool_now, kind, &filter) {
                return rule(Source::Layer(name), kind, filter);
            }
        }
    }
    // One object by its name, the only one of its kind so named.
    if let [one] = picked.as_slice()
        && then_and_now.iter().filter(|o| o.name == one.name && o.kind() == one.kind()).count() == 1
        && finds_now(&now.iter().copied().filter(|o| o.name == one.name).collect::<Vec<_>>(), Some(one.kind()), &BTreeMap::new())
    {
        return rule(Source::Named(one.name.clone()), Some(one.kind()), BTreeMap::new());
    }
    rule(Source::Picked(targets.to_vec()), None, BTreeMap::new())
}

/// The ids a parameter value names, if it names objects: one id, or a list of them, each 32 hex digits and known.
fn ids_in(value: &Value, known: &impl Fn(ObjectId) -> bool) -> Option<(bool, Vec<ObjectId>)> {
    let one = |v: &Value| v.as_str().and_then(ObjectId::parse).filter(|&id| known(id));
    match value {
        Value::String(_) => one(value).map(|id| (false, vec![id])),
        Value::Array(items) if !items.is_empty() => items.iter().map(one).collect::<Option<Vec<_>>>().map(|ids| (true, ids)),
        _ => None,
    }
}

impl Session {
    /// Captures runs from the run log as a new workflow (decision 0065), saved by a `workflow.save` run. The runs must
    /// still be applied: not refused, not undone. Each becomes a step in log order, with the parameters it ran with as
    /// its settings, and each object id it was given turned into the simplest rule that finds the same objects at
    /// that point: what an earlier captured step made (whole, or of a kind, or where an attribute has a value), else
    /// what's on a layer, else an object by its name, else the objects as picked. Each step keeps its run as its last,
    /// so a run from a later step reads what the earlier ones made.
    pub fn capture_workflow(&mut self, name: &str, runs: &[RunId]) -> Result<RunRecord, CommandError> {
        let content = self.captured(runs)?;
        let params = serde_json::to_value(SaveWorkflowParams { workflow: None, name: name.to_string(), content }).map_err(|e| CommandError::BadParams(e.to_string()))?;
        self.run("workflow.save", params)
    }

    /// The workflow that capturing these runs would save.
    pub fn captured(&self, runs: &[RunId]) -> Result<Workflow, CommandError> {
        if runs.is_empty() {
            return Err(CommandError::Invalid("pick at least one run to capture".into()));
        }
        let log = self.log();
        let applied = applied_runs(log);
        let wanted: HashSet<RunId> = runs.iter().copied().collect();
        if wanted.len() != runs.len() {
            return Err(CommandError::Invalid("a run is picked twice".into()));
        }
        let picked: Vec<(usize, &RunRecord)> = log
            .iter()
            .enumerate()
            .filter_map(|(at, e)| match e {
                LogEntry::Run(r) if wanted.contains(&r.run) => Some((at, r)),
                _ => None,
            })
            .collect();
        for run in runs {
            match picked.iter().find(|(_, r)| r.run == *run) {
                None => return Err(CommandError::Invalid(format!("run {run} isn't in the run log"))),
                Some((_, r)) if !applied.contains(run) => {
                    let why = if matches!(r.status, RunStatus::Failed(_)) { "was refused" } else { "has been undone" };
                    return Err(CommandError::Invalid(format!("the {} run {run} {why}, so it can't be captured", r.command)));
                }
                _ => {}
            }
        }
        let project = self.project();
        let timeline = Timeline::new(project, log, &applied);
        let known = |id: ObjectId| timeline.events.contains_key(&id) || project.object(id).is_some();
        let layer_name = |o: &Object| project.layer(o.layer).map(|l| l.name.clone());
        let now: Vec<&Object> = project.objects().map(|o| o.as_ref()).collect();
        let mut steps: Vec<WorkflowStep> = Vec::new();
        let mut earlier: Vec<Captured> = Vec::new();
        for (index, (at, record)) in picked.iter().enumerate() {
            let there = timeline.there_before(*at);
            // What earlier steps made, as it stood when this run read it.
            let alive: Vec<Captured> = earlier.iter().map(|c| Captured { id: c.id, outputs: c.outputs.iter().copied().filter(|&id| timeline.before(id, *at).is_some()).collect() }).collect();
            let gone_name = |id: ObjectId| timeline.names.get(&id).cloned();
            let (settings, inputs) = split_params(&record.params, &known, |ids, many| rule_for(ids, many, &there, &now, &alive, layer_name, gone_name));
            let id = index as u32 + 1;
            let made: Vec<ObjectId> = record.outputs.iter().filter(|c| c.change == ChangeKind::Created).map(|c| c.id).collect();
            let revised: Vec<ObjectId> = record.outputs.iter().filter(|c| c.change == ChangeKind::Revised).map(|c| c.id).collect();
            let deleted = record.outputs.iter().filter(|c| c.change == ChangeKind::Deleted).count();
            earlier.push(Captured { id, outputs: made.iter().chain(&revised).copied().collect() });
            steps.push(WorkflowStep {
                id,
                note: String::new(),
                on: true,
                action: StepAction::Command { command: record.command.clone(), settings, inputs },
                last: Some(LastRun {
                    run: record.run,
                    workflow_run: None,
                    at_unix_ms: record.started_unix_ms,
                    ms: record.duration_ms,
                    made,
                    revised,
                    deleted,
                    warnings: record.notes.iter().filter(|n| n.severity >= Severity::Warning).count(),
                    refused: false,
                }),
            });
        }
        Ok(Workflow { steps })
    }

    /// The step a tool panel's command and settings make, to go into a workflow at `at` (from 0; the end if left out),
    /// the steps before it its earlier steps (decision 0065, Add to workflow). Each object id in the settings becomes
    /// the simplest rule that finds the same objects now, as in a capture: what an earlier step last made first. Nothing
    /// is saved; returns where it goes and the step.
    pub fn step_from(&self, workflow: ObjectId, at: Option<usize>, command: &str, params: &Value, note: &str) -> Result<(usize, WorkflowStep), CommandError> {
        if self.registry().get(command).is_none() {
            return Err(CommandError::UnknownCommand(command.to_string()));
        }
        let content = self.workflow(workflow)?;
        let at = at.unwrap_or(content.steps.len()).min(content.steps.len());
        let project = self.project();
        let there: Vec<&Object> = project.objects().map(|o| o.as_ref()).collect();
        let earlier: Vec<Captured> = content.steps[..at]
            .iter()
            .filter_map(|s| s.last.as_ref().map(|l| Captured { id: s.id, outputs: l.outputs().filter(|&id| project.object(id).is_some()).collect() }))
            .collect();
        let known = |id: ObjectId| project.object(id).is_some();
        let layer_name = |o: &Object| project.layer(o.layer).map(|l| l.name.clone());
        let (settings, inputs) = split_params(params, &known, |ids, many| rule_for(ids, many, &there, &there, &earlier, layer_name, |_| None));
        let step = WorkflowStep { id: content.next_id(), note: note.to_string(), on: true, action: StepAction::Command { command: command.to_string(), settings, inputs }, last: None };
        Ok((at, step))
    }

    /// [`Session::step_from`], the step put in the workflow and saved by a `workflow.save` run.
    pub fn add_workflow_step(&mut self, workflow: ObjectId, at: Option<usize>, command: &str, params: &Value, note: &str) -> Result<RunRecord, CommandError> {
        let (at, step) = self.step_from(workflow, at, command, params, note)?;
        let mut content = self.workflow(workflow)?.clone();
        content.steps.insert(at, step);
        let name = self.project().object(workflow).map(|o| o.name.clone()).unwrap_or_default();
        let params = serde_json::to_value(SaveWorkflowParams { workflow: Some(workflow), name, content }).map_err(|e| CommandError::BadParams(e.to_string()))?;
        self.run("workflow.save", params)
    }

    /// A workflow's content, by its id.
    pub fn workflow(&self, id: ObjectId) -> Result<&Workflow, CommandError> {
        let object = self.project().object(id).ok_or(CommandError::MissingObject(id))?;
        match &object.body {
            Body::Workflow(w) => Ok(w),
            _ => Err(CommandError::WrongKind { id, expected: ObjectKind::Workflow, found: object.kind() }),
        }
    }
}

/// A run's or a panel's parameters as a step's settings and inputs: each one holding an object's id, or a list of them,
/// an input by the rule `rule` gives it; every other one a setting.
fn split_params(params: &Value, known: &impl Fn(ObjectId) -> bool, mut rule: impl FnMut(&[ObjectId], bool) -> InputRule) -> (Value, BTreeMap<String, InputRule>) {
    let mut settings = Map::new();
    let mut inputs = BTreeMap::new();
    for (field, value) in params.as_object().into_iter().flatten() {
        match ids_in(value, known) {
            Some((many, ids)) => {
                inputs.insert(field.clone(), rule(&ids, many));
            }
            None => {
                settings.insert(field.clone(), value.clone());
            }
        }
    }
    (Value::Object(settings), inputs)
}
