//! The workflow report (decision 0068): what a workflow made, case by case, worked out from the workflow, each case's
//! latest run and the blocks' own attributes, never typed up by hand. These are the figures; `report_html` writes them as
//! one self-contained page.
//!
//! A block is a solid a step of the case last made or changed that carries a `block` attribute, still in the project.
//! Coal is material `coal`; the rest is waste, with what lies below the last floor kept apart from the strip ratio.
//! Tonnes are volume × the density the interrogation wrote, on the density grid's own basis; RD is composited by
//! volume and every other value by tonnes, as the interrogation averages them. The pit is reconciled: the solid the
//! case started from against its blocks plus what was left out (thin layers, flaps, solids deleted by hand), the
//! difference shown whatever it is (engine rule 9).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use zd_core::{AttrValue, BASE_CASE, Body, ChangeKind, InputRule, Object, ObjectId, ObjectKind, RunId, SeamModel, Source, StepAction, Workflow};

use crate::builtin::horizon_order;
use crate::{Code, CommandError, LogEntry, RunRecord, Session, Severity};

/// A difference between the pit and what accounts for it within this share of the pit is taken as none.
pub const RECONCILE_TOLERANCE: f64 = 1e-6;

/// The commands that cut a solid into pieces: the first of them in a workflow reads the pit, when no step makes one.
const CUTS: [&str; 4] = ["solid.slice_by_faces", "solid.cut_by_surface", "solid.cut_by_seams", "solid.cut_by_polygons"];

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub workflow: String,
    /// The project file's name, if it has been saved.
    pub project: Option<String>,
    pub made_unix_ms: u64,
    /// Local time's offset from UTC where the report was made, for its dates.
    pub utc_offset_minutes: i32,
    /// Base first, then each case.
    pub cases: Vec<CaseFigures>,
    pub steps: Vec<StepLine>,
    pub inputs: Vec<InputLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseFigures {
    pub name: String,
    /// When its steps last ran; none if no step has.
    pub ran: Option<Ran>,
    /// The settings it changes from Base.
    pub changes: Vec<SettingChange>,
    pub blocks: Vec<BlockRow>,
    /// A row a horizon and material, top down as the seam cut names them.
    pub horizons: Vec<GroupRow>,
    pub coal: GroupRow,
    /// Waste above the last floor: what the strip ratio counts.
    pub waste: GroupRow,
    /// Waste below the last floor.
    pub below: GroupRow,
    /// Waste bcm a coal tonne.
    pub strip_ratio: Option<f64>,
    /// The attribute the interrogation wrote density to, when a step interrogated the blocks.
    pub density: Option<String>,
    /// The other values it wrote, in the model's order.
    pub qualities: Vec<String>,
    /// Coal blocks with no density, so no tonnes.
    pub no_density: usize,
    pub left_out: Vec<LeftOut>,
    pub pit: Option<Pit>,
    pub warnings: Vec<WarningLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ran {
    /// When its last step ran.
    pub at_unix_ms: u64,
    /// Its steps' run times added up.
    pub ms: f64,
    pub steps_run: usize,
    pub steps: usize,
    /// The step (from 1) that was refused, if one was.
    pub refused_at: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingChange {
    pub step: usize,
    pub field: String,
    pub base: Value,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockRow {
    pub name: String,
    pub strip: Option<String>,
    pub block: Option<String>,
    pub horizon: Option<String>,
    pub seam: Option<String>,
    pub material: Option<String>,
    pub volume: f64,
    pub rd: Option<f64>,
    pub tonnes: Option<f64>,
    /// Each interrogated value but density, by attribute.
    pub values: BTreeMap<String, f64>,
    /// Its 2.5D column volume, where the interrogation wrote one.
    pub column: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GroupRow {
    pub label: String,
    pub material: String,
    pub volume: f64,
    /// Coal only: volume × RD over the blocks that have one.
    pub tonnes: Option<f64>,
    pub rd: Option<f64>,
    pub qualities: BTreeMap<String, f64>,
    pub blocks: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LeftOut {
    pub what: String,
    /// The step (from 1) that left it out; none for solids deleted by hand.
    pub step: Option<usize>,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pit {
    pub names: Vec<String>,
    pub volume: f64,
    /// Blocks plus what was left out.
    pub accounted: f64,
    /// The pit less what accounts for it.
    pub difference: f64,
    pub within: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WarningLine {
    pub step: Option<usize>,
    pub command: String,
    pub severity: String,
    pub code: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepLine {
    pub position: usize,
    pub command: Option<String>,
    pub note: String,
    pub on: bool,
    /// Each input and the rule finding it, in words.
    pub inputs: Vec<(String, String)>,
    /// Base's settings, each in a few words.
    pub settings: Vec<(String, String)>,
    /// What its last run did, a line a case.
    pub last: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputLine {
    pub rule: String,
    /// The steps (from 1) that read it.
    pub steps: Vec<usize>,
    /// What it finds now.
    pub found: String,
    pub files: Vec<FileLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileLine {
    pub file: String,
    pub bytes: u64,
    pub checksum: String,
    pub modified_unix_ms: u64,
}

impl Session {
    /// The report's figures for a workflow: every case from its latest run (decision 0068).
    pub fn workflow_report(&self, workflow: ObjectId, utc_offset_minutes: i32) -> Result<Report, CommandError> {
        let w = self.workflow(workflow)?.clone();
        let name = self.project().object(workflow).map(|o| o.name.clone()).unwrap_or_default();
        let log = LogView::of(self.log());
        let names: Vec<String> = std::iter::once(BASE_CASE.to_string()).chain(w.cases.iter().map(|c| c.name.clone())).collect();
        let cases = names.iter().filter_map(|case| w.as_case(case).map(|as_case| self.case_figures(&w, case, &as_case, &log))).collect();
        Ok(Report {
            workflow: name,
            project: self.file().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())),
            made_unix_ms: now_ms(),
            utc_offset_minutes,
            cases,
            steps: self.step_lines(&w, &names),
            inputs: self.input_lines(&w),
        })
    }

    /// Writes the report as one HTML page into `folder`, or the project's reports folder beside its file (decision 0068):
    /// `<project> reports/<workflow> <date> <time>.html`. Returns where it went.
    pub fn write_workflow_report(&self, workflow: ObjectId, utc_offset_minutes: i32, folder: Option<&Path>) -> Result<PathBuf, CommandError> {
        let report = self.workflow_report(workflow, utc_offset_minutes)?;
        let folder = match folder {
            Some(f) => f.to_path_buf(),
            None => reports_folder(&self.file().ok_or_else(|| CommandError::Invalid("save the project first: a report goes in a folder beside it".into()))?),
        };
        std::fs::create_dir_all(&folder).map_err(|e| CommandError::Invalid(format!("can't make the reports folder {}: {e}", folder.display())))?;
        let (date, time) = civil(report.made_unix_ms, utc_offset_minutes);
        let stem = format!("{} {} {}", file_safe(&report.workflow), date, time.replace(':', ""));
        let mut path = folder.join(format!("{stem}.html"));
        let mut n = 2;
        while path.exists() {
            path = folder.join(format!("{stem} {n}.html"));
            n += 1;
        }
        std::fs::write(&path, crate::report_html::page(&report)).map_err(|e| CommandError::Invalid(format!("can't write the report {}: {e}", path.display())))?;
        Ok(path)
    }

    fn case_figures(&self, w: &Workflow, case: &str, as_case: &Workflow, log: &LogView<'_>) -> CaseFigures {
        let project = self.project();
        let position = |id: u32| w.position(id).map(|p| p + 1);
        // Everything the case's steps last made or changed, once each, in step order.
        let mut outputs: Vec<ObjectId> = Vec::new();
        let mut seen = HashSet::new();
        for step in &as_case.steps {
            for id in step.last.iter().flat_map(|l| l.outputs()) {
                if seen.insert(id) {
                    outputs.push(id);
                }
            }
        }
        let record_of = |step: &zd_core::WorkflowStep| step.last.as_ref().and_then(|l| log.records.get(&l.run).copied());
        let step_of = |command: &str| as_case.steps.iter().find(|s| matches!(&s.action, StepAction::Command { command: c, .. } if c == command));

        // The interrogation's model and prefix name the density and the other values.
        let interrogation = step_of("solid.interrogate").and_then(record_of);
        let prefix = interrogation.and_then(|r| r.params.get("prefix")).and_then(Value::as_str).unwrap_or("").to_string();
        let model_of = |record: Option<&RunRecord>| -> Option<&SeamModel> {
            let id: ObjectId = serde_json::from_value(record?.params.get("model")?.clone()).ok()?;
            match &project.object(id)?.body {
                Body::SeamModel(m) => Some(m),
                _ => None,
            }
        };
        let model = model_of(interrogation).or_else(|| model_of(step_of("solid.cut_by_seams").and_then(record_of)));
        let density = interrogation.zip(model).map(|(_, m)| format!("{prefix}{}", m.density));
        let column_key = format!("{prefix}{}", crate::builtin::COLUMN_VOLUME);
        let mut quality_codes: Vec<String> = Vec::new();
        if let (Some(_), Some(m)) = (interrogation, model) {
            for h in m.working_section() {
                for code in h.grids.keys() {
                    let key = format!("{prefix}{code}");
                    if Some(&key) != density.as_ref() && !quality_codes.contains(&key) {
                        quality_codes.push(key);
                    }
                }
            }
        }

        let blocks: Vec<BlockRow> = outputs
            .iter()
            .filter_map(|&id| project.object(id))
            .filter(|o| matches!(o.body, Body::Solid(_)) && o.attributes.contains_key("block"))
            .map(|o| block_row(o, density.as_deref(), &quality_codes, &column_key))
            .collect();
        let qualities: Vec<String> = quality_codes.into_iter().filter(|q| blocks.iter().any(|b| b.values.contains_key(q))).collect();

        // Rows top down as the seam cut names its horizons, then any it didn't name, as found.
        let mut order: Vec<(String, String)> = model.map(horizon_order).unwrap_or_default();
        for b in &blocks {
            let key = (b.horizon.clone().unwrap_or_default(), b.material.clone().unwrap_or_default());
            if !order.contains(&key) {
                order.push(key);
            }
        }
        let horizons: Vec<GroupRow> = order
            .iter()
            .filter_map(|(h, m)| {
                let of: Vec<&BlockRow> = blocks.iter().filter(|b| b.horizon.as_deref().unwrap_or("") == h && b.material.as_deref().unwrap_or("") == m).collect();
                (!of.is_empty()).then(|| composite(h, m, &of, &qualities))
            })
            .collect();
        let is_coal = |b: &&BlockRow| b.material.as_deref() == Some("coal");
        let is_below = |b: &&BlockRow| b.horizon.as_deref().is_some_and(|h| h.starts_with("below "));
        let coal = composite("Coal", "coal", &blocks.iter().filter(is_coal).collect::<Vec<_>>(), &qualities);
        let waste = composite("Waste", "waste", &blocks.iter().filter(|b| !is_coal(b) && !is_below(b)).collect::<Vec<_>>(), &qualities);
        let below = composite("Below the last floor", "waste", &blocks.iter().filter(|b| !is_coal(b) && is_below(b)).collect::<Vec<_>>(), &qualities);
        let strip_ratio = coal.tonnes.filter(|t| *t > 0.0).map(|t| waste.volume / t);
        let no_density = blocks.iter().filter(is_coal).filter(|b| b.rd.is_none()).count();

        // What was left out: the cut's measured notes, and solids deleted by hand that a step of this case had made.
        let mut left_out = Vec::new();
        let mut warnings = Vec::new();
        for step in &as_case.steps {
            let Some(record) = record_of(step) else { continue };
            let at = position(step.id);
            for note in &record.notes {
                let what = match note.code {
                    Some(Code::SeamThinLeftOut) => Some("Layers thinner than the minimum"),
                    Some(Code::SeamFlapsLeftOut) => Some("Flaps under the flap thickness"),
                    _ => None,
                };
                if let (Some(what), Some(m)) = (what, &note.value) {
                    left_out.push(LeftOut { what: what.into(), step: at, volume: m.value });
                }
                if note.severity >= Severity::Warning {
                    let severity = if note.severity == Severity::Blocking { "blocking" } else { "warning" };
                    warnings.push(WarningLine { step: at, command: record.command.clone(), severity: severity.into(), code: note.code.map(|c| c.name().to_string()), text: note.text.clone() });
                }
            }
            if step.last.as_ref().is_some_and(|l| l.refused) {
                warnings.push(WarningLine { step: at, command: record.command.clone(), severity: "refused".into(), code: record.refused.map(|c| c.name().to_string()), text: record.messages.join(" ") });
            }
        }
        let made: HashSet<ObjectId> = as_case.steps.iter().flat_map(|s| s.last.iter().flat_map(|l| l.outputs())).collect();
        // A delete that is a step of the workflow is that step's; any other was done by hand, as at a pause.
        let step_runs: HashMap<RunId, usize> = as_case.steps.iter().filter_map(|s| Some((s.last.as_ref()?.run, position(s.id)?))).collect();
        for record in log.deletes() {
            let deleted: Vec<ObjectId> = record.outputs.iter().filter(|o| o.change == ChangeKind::Deleted).map(|o| o.id).collect();
            let ours = deleted.iter().filter(|id| made.contains(id)).count();
            if ours == 0 {
                continue;
            }
            let measured = record.notes.iter().find(|n| n.code == Some(Code::ObjectSolidsDeleted)).and_then(|n| n.value.as_ref());
            match measured {
                Some(m) if ours == deleted.len() => {
                    let step = step_runs.get(&record.run).copied();
                    let by = if step.is_some() { "Deleted" } else { "Deleted by hand" };
                    left_out.push(LeftOut { what: format!("{by} ({ours} solid{})", if ours == 1 { "" } else { "s" }), step, volume: m.value })
                }
                _ => warnings.push(WarningLine {
                    step: None,
                    command: record.command.clone(),
                    severity: "warning".into(),
                    code: None,
                    text: format!("{ours} solids this case made were deleted by hand in a run whose volume can't be told apart, so the reconciliation can't account for them"),
                }),
            }
        }

        // The pit: what the first solid-making step made, or what the first cut read.
        let pit_ids: Vec<ObjectId> = match step_of("solid.between_surfaces").and_then(|s| s.last.as_ref()) {
            Some(last) => last.made.clone(),
            None => as_case
                .steps
                .iter()
                .find(|s| matches!(&s.action, StepAction::Command { command, .. } if CUTS.contains(&command.as_str())))
                .and_then(record_of)
                .and_then(|r| r.params.get("solids").cloned())
                .and_then(|v| serde_json::from_value::<Vec<ObjectId>>(v).ok())
                .unwrap_or_default(),
        };
        let pit_solids: Vec<&Object> = pit_ids.iter().filter_map(|&id| project.object(id)).map(|o| o.as_ref()).filter(|o| matches!(o.body, Body::Solid(_))).collect();
        let pit = (!pit_solids.is_empty() && !blocks.is_empty()).then(|| {
            let volume: f64 = pit_solids.iter().map(|o| solid_volume(o)).sum();
            let accounted = blocks.iter().map(|b| b.volume).sum::<f64>() + left_out.iter().map(|l| l.volume).sum::<f64>();
            let difference = volume - accounted;
            Pit { names: pit_solids.iter().map(|o| o.name.clone()).collect(), volume, accounted, difference, within: difference.abs() <= RECONCILE_TOLERANCE * volume.abs().max(1.0) }
        });

        let lasts: Vec<&zd_core::LastRun> = as_case.steps.iter().filter_map(|s| s.last.as_ref()).collect();
        let ran = (!lasts.is_empty()).then(|| Ran {
            at_unix_ms: lasts.iter().map(|l| l.at_unix_ms).max().unwrap_or(0),
            ms: lasts.iter().map(|l| l.ms).sum(),
            steps_run: lasts.len(),
            steps: as_case.steps.len(),
            refused_at: as_case.steps.iter().find(|s| s.last.as_ref().is_some_and(|l| l.refused)).and_then(|s| position(s.id)),
        });
        let changes = w
            .case(case)
            .map(|c| {
                c.settings
                    .iter()
                    .flat_map(|(&id, fields)| {
                        let base = w.step(id).and_then(|s| match &s.action {
                            StepAction::Command { settings, .. } => Some(settings.clone()),
                            StepAction::Pause => None,
                        });
                        fields.iter().map(move |(field, value)| SettingChange {
                            step: position(id).unwrap_or(0),
                            field: field.clone(),
                            base: base.as_ref().and_then(|b| b.get(field)).cloned().unwrap_or(Value::Null),
                            value: value.clone(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        CaseFigures { name: case.to_string(), ran, changes, blocks, horizons, coal, waste, below, strip_ratio, density, qualities, no_density, left_out, pit, warnings }
    }

    fn step_lines(&self, w: &Workflow, cases: &[String]) -> Vec<StepLine> {
        let as_cases: Vec<(String, Workflow)> = cases.iter().filter_map(|c| w.as_case(c).map(|w| (c.clone(), w))).collect();
        w.steps
            .iter()
            .enumerate()
            .map(|(k, step)| {
                let (command, inputs, settings) = match &step.action {
                    StepAction::Command { command, settings, inputs } => (
                        Some(command.clone()),
                        inputs.iter().map(|(name, rule)| (name.clone(), self.rule_words(w, rule))).collect(),
                        settings.as_object().map(|m| m.iter().map(|(f, v)| (f.clone(), value_words(v))).collect()).unwrap_or_default(),
                    ),
                    StepAction::Pause => (None, Vec::new(), Vec::new()),
                };
                let last = as_cases
                    .iter()
                    .map(|(case, cw)| {
                        let words = match cw.steps.get(k).and_then(|s| s.last.as_ref()) {
                            None => "not run".to_string(),
                            Some(l) if l.refused => "refused".to_string(),
                            Some(l) => {
                                let mut parts = vec![format!("made {}", l.made.len())];
                                if !l.revised.is_empty() {
                                    parts.push(format!("changed {}", l.revised.len()));
                                }
                                if l.warnings > 0 {
                                    parts.push(format!("{} warning{}", l.warnings, if l.warnings == 1 { "" } else { "s" }));
                                }
                                parts.push(format!("{:.1} s", l.ms / 1000.0));
                                parts.join(", ")
                            }
                        };
                        (case.clone(), words)
                    })
                    .collect();
                StepLine { position: k + 1, command, note: step.note.clone(), on: step.on, inputs, settings, last }
            })
            .collect()
    }

    /// A rule in words: what step 3 made, the objects on layer 'strips', the seam model named 'geology'.
    fn rule_words(&self, w: &Workflow, rule: &InputRule) -> String {
        let kind = rule.kind.map(kind_words);
        let mut words = match &rule.from {
            Source::Step(id) => format!("what step {} made", w.position(*id).map_or(0, |p| p + 1)),
            Source::Layer(name) => format!("the {} on layer '{name}'", kind.map_or("objects".to_string(), |k| format!("{k}s"))),
            Source::Named(name) => format!("the {} named '{name}'", kind.unwrap_or("object")),
            Source::Picked(ids) => format!("{} object{} as picked", ids.len(), if ids.len() == 1 { "" } else { "s" }),
        };
        if matches!(rule.from, Source::Step(_)) {
            if let Some(k) = kind {
                words.push_str(&format!(", {k}s"));
            }
        }
        if !rule.filter.is_empty() {
            let filter: Vec<String> = rule.filter.iter().map(|(k, v)| format!("{k} = {}", attr_words(v))).collect();
            words.push_str(&format!(", where {}", filter.join(" and ")));
        }
        words
    }

    /// What steps take from outside the workflow, each rule once, with what it finds now and any grid files behind it.
    fn input_lines(&self, w: &Workflow) -> Vec<InputLine> {
        let project = self.project();
        let mut lines: Vec<InputLine> = Vec::new();
        for (k, step) in w.steps.iter().enumerate() {
            let StepAction::Command { inputs, .. } = &step.action else { continue };
            for rule in inputs.values() {
                if matches!(rule.from, Source::Step(_)) {
                    continue;
                }
                let words = self.rule_words(w, rule);
                if let Some(line) = lines.iter_mut().find(|l| l.rule == words) {
                    line.steps.push(k + 1);
                    continue;
                }
                let found: Vec<&Object> = match &rule.from {
                    Source::Layer(name) => project.layer_named(name).map(|l| project.objects().filter(|o| o.layer == l.id).map(|o| o.as_ref()).collect()).unwrap_or_default(),
                    Source::Named(name) => project.objects().filter(|o| &o.name == name).map(|o| o.as_ref()).collect(),
                    Source::Picked(ids) => ids.iter().filter_map(|&id| project.object(id)).map(|o| o.as_ref()).collect(),
                    Source::Step(_) => Vec::new(),
                };
                let found: Vec<&Object> = found.into_iter().filter(|o| rule.kind.is_none_or(|k| o.kind() == k)).filter(|o| rule.filter.iter().all(|(a, v)| o.attributes.get(a) == Some(v))).collect();
                let described = match found.len() {
                    0 => "nothing, now".to_string(),
                    1 => format!("'{}', revision {}", found[0].name, found[0].revision),
                    n => format!("{n} objects"),
                };
                let files = found
                    .iter()
                    .filter_map(|o| if let Body::SeamModel(m) = &o.body { Some(m) } else { None })
                    .flat_map(|m| m.horizons.iter().flat_map(|h| std::iter::once(&h.roof).chain(std::iter::once(&h.floor)).chain(h.grids.values())))
                    .map(|g| FileLine { file: g.file.clone(), bytes: g.fingerprint.bytes, checksum: format!("{:016x}", g.fingerprint.checksum), modified_unix_ms: g.fingerprint.modified_unix_ms })
                    .collect();
                lines.push(InputLine { rule: words, steps: vec![k + 1], found: described, files });
            }
        }
        lines
    }
}

/// The run log read once: each run by id, and which were undone and not redone.
struct LogView<'a> {
    records: HashMap<RunId, &'a RunRecord>,
    runs: Vec<&'a RunRecord>,
    undone: HashSet<RunId>,
}

impl<'a> LogView<'a> {
    fn of(log: &'a [LogEntry]) -> Self {
        let mut records = HashMap::new();
        let mut runs = Vec::new();
        let mut undone = HashSet::new();
        for entry in log {
            match entry {
                LogEntry::Run(r) => {
                    records.insert(r.run, r);
                    runs.push(r);
                }
                LogEntry::Undo { run, .. } => {
                    undone.insert(*run);
                }
                LogEntry::Redo { run, .. } => {
                    undone.remove(run);
                }
                _ => {}
            }
        }
        LogView { records, runs, undone }
    }

    /// Runs that deleted objects and stand.
    fn deletes(&self) -> impl Iterator<Item = &'a RunRecord> + '_ {
        self.runs.iter().copied().filter(|r| (r.command == "object.delete" || r.command == "object.delete_many") && !self.undone.contains(&r.run) && r.outputs.iter().any(|o| o.change == ChangeKind::Deleted))
    }
}

fn solid_volume(o: &Object) -> f64 {
    match &o.body {
        Body::Solid(s) => s.volume(),
        _ => 0.0,
    }
}

fn number(o: &Object, key: &str) -> Option<f64> {
    match o.attributes.get(key)? {
        AttrValue::Number(n) if n.is_finite() => Some(*n),
        _ => None,
    }
}

fn text(o: &Object, key: &str) -> Option<String> {
    o.attributes.get(key).map(attr_words)
}

fn attr_words(v: &AttrValue) -> String {
    match v {
        AttrValue::Text(t) => t.clone(),
        AttrValue::Number(n) => format!("{n}"),
        AttrValue::Bool(b) => format!("{b}"),
    }
}

fn block_row(o: &Object, density: Option<&str>, qualities: &[String], column: &str) -> BlockRow {
    let volume = solid_volume(o);
    let material = text(o, "material");
    let rd = if material.as_deref() == Some("coal") { density.and_then(|d| number(o, d)) } else { None };
    BlockRow {
        name: o.name.clone(),
        strip: text(o, "strip"),
        block: text(o, "block"),
        horizon: text(o, "horizon"),
        seam: text(o, "seam"),
        material,
        volume,
        rd,
        tonnes: rd.map(|rd| volume * rd),
        values: qualities.iter().filter_map(|q| number(o, q).map(|v| (q.clone(), v))).collect(),
        column: number(o, column),
    }
}

/// Blocks added up: volume; for coal, tonnes over those with a density, RD by volume and every other value by tonnes.
fn composite(label: &str, material: &str, blocks: &[&BlockRow], qualities: &[String]) -> GroupRow {
    let volume = blocks.iter().map(|b| b.volume).sum();
    let mut row = GroupRow { label: label.into(), material: material.into(), volume, blocks: blocks.len(), ..GroupRow::default() };
    let weighed: Vec<(&BlockRow, f64)> = blocks.iter().filter_map(|b| b.tonnes.map(|t| (*b, t))).collect();
    if weighed.is_empty() {
        return row;
    }
    let tonnes: f64 = weighed.iter().map(|(_, t)| t).sum();
    let by_volume: f64 = weighed.iter().map(|(b, _)| b.volume).sum();
    row.tonnes = Some(tonnes);
    row.rd = (by_volume > 0.0).then(|| tonnes / by_volume);
    for q in qualities {
        let (sum, weight) = weighed.iter().filter_map(|(b, t)| b.values.get(q).map(|v| (v * t, *t))).fold((0.0, 0.0), |(s, w), (a, b)| (s + a, w + b));
        if weight > 0.0 {
            row.qualities.insert(q.clone(), sum / weight);
        }
    }
    row
}

fn kind_words(kind: ObjectKind) -> &'static str {
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

/// A setting in a few words: a list or a table is set in its tool, so only how many it holds.
fn value_words(v: &Value) -> String {
    match v {
        Value::Null => "none".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(a) => format!("{} item{}", a.len(), if a.len() == 1 { "" } else { "s" }),
        Value::Object(o) => format!("{} field{}", o.len(), if o.len() == 1 { "" } else { "s" }),
    }
}

/// `<project> reports` beside the project's file.
pub fn reports_folder(project_file: &Path) -> PathBuf {
    let stem = project_file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into());
    project_file.parent().unwrap_or(Path::new(".")).join(format!("{stem} reports"))
}

fn file_safe(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { '_' } else { c }).collect();
    let trimmed = cleaned.trim().trim_end_matches('.').to_string();
    if trimmed.is_empty() { "workflow".into() } else { trimmed }
}

/// A moment as a date (`2026-11-14`) and a time (`21:10`), at an offset from UTC in minutes.
pub fn civil(unix_ms: u64, offset_minutes: i32) -> (String, String) {
    let seconds = (unix_ms / 1000) as i64 + i64::from(offset_minutes) * 60;
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (format!("{year:04}-{month:02}-{day:02}"), format!("{:02}:{:02}", of_day / 3600, of_day % 3600 / 60))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_in_local_time() {
        // 2026-11-14 10:20 UTC is 20:20 in Brisbane (UTC+10).
        assert_eq!(civil(1_794_651_600_000, 600), ("2026-11-14".into(), "20:20".into()));
        assert_eq!(civil(0, 0), ("1970-01-01".into(), "00:00".into()));
        assert_eq!(civil(951_782_400_000, 0).0, "2000-02-29");
    }

    #[test]
    fn coal_composites_rd_by_volume_and_qualities_by_tonnes() {
        let block = |v: f64, rd: f64, ash: f64| BlockRow {
            name: String::new(),
            strip: None,
            block: None,
            horizon: None,
            seam: None,
            material: Some("coal".into()),
            volume: v,
            rd: Some(rd),
            tonnes: Some(v * rd),
            values: [("ash".to_string(), ash)].into(),
            column: None,
        };
        let (a, b) = (block(72_000.0, 1.4, 10.0), block(72_000.0, 1.5, 20.0));
        let row = composite("Coal", "coal", &[&a, &b], &["ash".into()]);
        // 100,800 t and 108,000 t: 208,800 t over 144,000 m³ is RD 1.45; ash (1,008,000 + 2,160,000) / 208,800.
        assert!((row.tonnes.unwrap() - 208_800.0).abs() < 1e-9);
        assert!((row.rd.unwrap() - 1.45).abs() < 1e-12);
        assert!((row.qualities["ash"] - 3_168_000.0 / 208_800.0).abs() < 1e-12);
        assert_eq!(row.blocks, 2);
    }

    #[test]
    fn a_report_file_is_named_safely_beside_the_project() {
        assert_eq!(reports_folder(Path::new("D:/pits/Demo.zdp")), PathBuf::from("D:/pits/Demo reports"));
        assert_eq!(file_safe("Toe/to: blocks?"), "Toe_to_ blocks_");
        assert_eq!(file_safe("  "), "workflow");
    }
}
