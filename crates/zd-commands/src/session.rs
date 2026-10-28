use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zd_core::{AppliedChange, LayerId, ObjectRef, Project, RunId};

use crate::command::Inputs;
use crate::{CommandError, Registry};

/// The serialisable record of one run, successful or not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub run: RunId,
    pub command: String,
    /// The parameters as the command read them (as given, if they could not be read).
    pub params: Value,
    /// Object revisions the command read.
    pub inputs: Vec<ObjectRef>,
    /// Revisions the run added.
    pub outputs: Vec<AppliedChange>,
    pub layers_created: Vec<String>,
    pub started_unix_ms: u64,
    pub duration_ms: f64,
    /// Repair logs, warnings and timings the command reported.
    pub messages: Vec<String>,
    pub status: RunStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    Failed(String),
}

/// The run log: every run, undo and redo, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LogEntry {
    Run(RunRecord),
    Undo { run: RunId, at_unix_ms: u64 },
    Redo { run: RunId, at_unix_ms: u64 },
}

/// A project, the commands that can act on it, and the log of what they did.
pub struct Session {
    project: Project,
    registry: Registry,
    log: Vec<LogEntry>,
}

impl Session {
    pub fn new(registry: Registry) -> Self {
        Session { project: Project::new(), registry, log: Vec::new() }
    }

    /// A new project with the engine's own commands.
    pub fn with_builtins() -> Self {
        Session::new(Registry::with_builtins())
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn log(&self) -> &[LogEntry] {
        &self.log
    }

    /// Display state only: not a command, not undone.
    pub fn set_layer_visible(&mut self, layer: LayerId, visible: bool) -> bool {
        self.project.set_layer_visible(layer, visible)
    }

    /// Runs a command by name with JSON parameters. Its changes apply together or not at all, and
    /// the run is logged either way; a failure comes back as the error and as a failed record.
    pub fn run(&mut self, command: &str, params: Value) -> Result<RunRecord, CommandError> {
        let run = RunId::new();
        let started_unix_ms = now_ms();
        let clock = Instant::now();
        let mut record = RunRecord {
            run,
            command: command.to_string(),
            params: params.clone(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            layers_created: Vec::new(),
            started_unix_ms,
            duration_ms: 0.0,
            messages: Vec::new(),
            status: RunStatus::Ok,
        };
        let result = self.execute(run, command, &params, &mut record);
        record.duration_ms = clock.elapsed().as_secs_f64() * 1e3;
        if let Err(e) = &result {
            record.status = RunStatus::Failed(e.to_string());
        }
        self.log.push(LogEntry::Run(record.clone()));
        result.map(|()| record)
    }

    fn execute(&mut self, run: RunId, command: &str, params: &Value, record: &mut RunRecord) -> Result<(), CommandError> {
        let command = self.registry.get(command).ok_or_else(|| CommandError::UnknownCommand(command.to_string()))?;
        let inputs = Inputs::new(&self.project);
        let outcome = command.run_json(&inputs, params);
        record.inputs = inputs.into_read();
        let (outcome, canonical) = outcome?;
        record.params = canonical;
        record.messages = outcome.messages;
        let applied = self.project.apply(run, outcome.changes)?;
        record.outputs = applied.changes;
        record.layers_created = applied.layers_created.into_iter().map(|l| l.name).collect();
        Ok(())
    }

    /// Takes the last run off the project; returns its id.
    pub fn undo(&mut self) -> Option<RunId> {
        let run = self.project.undo()?;
        self.log.push(LogEntry::Undo { run, at_unix_ms: now_ms() });
        Some(run)
    }

    /// Puts the last undone run back; returns its id.
    pub fn redo(&mut self) -> Option<RunId> {
        let run = self.project.redo()?;
        self.log.push(LogEntry::Redo { run, at_unix_ms: now_ms() });
        Some(run)
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}
