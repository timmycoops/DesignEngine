use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zd_core::{AppliedChange, Body, Change, Colouring, Contours, LayerId, ObjectId, ObjectRef, Project, RunId, same_path};
use zd_io::{FolderLink, ProjectStore, SavedFile, Snapshot, ZdpStore};

use crate::builtin::{ModelLinks, check_solids_made, seam_links};
use crate::command::Inputs;
use crate::{Code, CommandError, Note, Registry};

/// A run that panicked, refused with what the panic said.
fn stopped_on_a_panic(command: &str, panic: &(dyn std::any::Any + Send)) -> CommandError {
    let said = panic.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| panic.downcast_ref::<String>().cloned()).unwrap_or_else(|| "no reason given".into());
    CommandError::Geometry(format!("{command} stopped on an error in the engine and changed nothing ({said}); please send the run log"))
}

/// The serialisable record of one run, successful or not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "SavedRun")]
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
    /// What the run reported, each line a note (decision 0059).
    pub notes: Vec<Note>,
    /// The notes' sentences, in order, as runs reported them before notes.
    pub messages: Vec<String>,
    pub status: RunStatus,
    /// The kind of refusal, when the run was refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<Code>,
}

/// A run as saved, by this build or one before notes, whose runs hold only `messages`: those read as info notes.
#[derive(Deserialize)]
struct SavedRun {
    run: RunId,
    command: String,
    params: Value,
    inputs: Vec<ObjectRef>,
    outputs: Vec<AppliedChange>,
    layers_created: Vec<String>,
    started_unix_ms: u64,
    duration_ms: f64,
    #[serde(default)]
    notes: Option<Vec<Note>>,
    #[serde(default)]
    messages: Vec<String>,
    status: RunStatus,
    #[serde(default)]
    refused: Option<Code>,
}

impl From<SavedRun> for RunRecord {
    fn from(r: SavedRun) -> Self {
        let notes = r.notes.unwrap_or_else(|| r.messages.iter().map(Note::info).collect());
        let messages = notes.iter().map(|n| n.text.clone()).collect();
        RunRecord {
            run: r.run,
            command: r.command,
            params: r.params,
            inputs: r.inputs,
            outputs: r.outputs,
            layers_created: r.layers_created,
            started_unix_ms: r.started_unix_ms,
            duration_ms: r.duration_ms,
            notes,
            messages,
            status: r.status,
            refused: r.refused,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    Failed(String),
}

/// The run log: every run, undo, redo, save and open, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant, reason = "a log holds runs mostly, and few entries; boxing them buys nothing")]
pub enum LogEntry {
    Run(RunRecord),
    Undo { run: RunId, at_unix_ms: u64 },
    Redo { run: RunId, at_unix_ms: u64 },
    Saved { path: String, at_unix_ms: u64 },
    Opened { path: String, at_unix_ms: u64 },
}

/// A project, the commands that can act on it, and the log of what they did.
pub struct Session {
    project: Project,
    registry: Registry,
    log: Vec<LogEntry>,
    /// The `.zdp` file this session last saved to or opened, shared with a save running off the session.
    store: Option<Arc<Mutex<ZdpStore>>>,
    /// How long the log was at the last save or open: a save with no new entry and nothing else changed writes nothing.
    saved_log_len: usize,
    /// What opening did beyond reading: grid folders found beside the project and relinked, or not found.
    open_notes: Vec<String>,
}

/// A save taken from a session in an instant, to write while the session carries on (decision 0058): the project as
/// it stood, the run log with this save in it, and the file to write.
pub struct SaveJob {
    store: Arc<Mutex<ZdpStore>>,
    snapshot: Snapshot,
    log: String,
    log_changed: bool,
    saved: LogEntry,
}

impl SaveJob {
    /// Encodes and writes the project file. Two saves of one file take turns.
    pub fn run(&self) -> Result<SavedFile, CommandError> {
        let mut store = self.store.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        store.save(&self.snapshot, &self.log, self.log_changed).map_err(|e| CommandError::Store(e.to_string()))
    }
}

/// The path with `.` and `..` worked out by name, as `D:\pits\b\..\geology` is `D:\pits\geology`.
fn lexically_normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if matches!(out.components().next_back(), Some(std::path::Component::Normal(_))) => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Which object each of a run's changes is about, so a note on what the run made can name it once it has an id.
struct ChangedObjects {
    /// For each change: whether it's an object's, and the id it already names (none for one it creates).
    changes: Vec<(bool, Option<ObjectId>)>,
}

fn changed_objects(changes: &[Change]) -> ChangedObjects {
    ChangedObjects {
        changes: changes
            .iter()
            .map(|c| match c {
                Change::Create { .. } => (true, None),
                Change::Revise { id, .. } | Change::Delete { id, .. } => (true, Some(*id)),
                _ => (false, None),
            })
            .collect(),
    }
}

impl ChangedObjects {
    /// Each change's object, taking the ids the project gave, in order, one an object's change.
    fn resolve(&self, applied: &[AppliedChange]) -> Vec<Option<ObjectId>> {
        let mut given = applied.iter();
        self.changes.iter().map(|&(object, id)| if object { given.next().map(|a| a.id).or(id) } else { None }).collect()
    }
}

/// Where a save goes: a `.zdproj` folder saves as a `.zdp` file beside it, of the same name.
fn zdp_path(path: &Path) -> PathBuf {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zdproj")) { path.with_extension("zdp") } else { path.to_path_buf() }
}

impl Session {
    pub fn new(registry: Registry) -> Self {
        Session { project: Project::new(), registry, log: Vec::new(), store: None, saved_log_len: 0, open_notes: Vec::new() }
    }

    /// A new project with the engine's own commands.
    pub fn with_builtins() -> Self {
        Session::new(Registry::with_builtins())
    }

    /// Opens a `.zdp` project file, or a `<name>.zdproj/` folder saved before it, with the engine's own commands. The
    /// run log carries on from where it was saved; the undo history starts empty.
    pub fn open(path: impl AsRef<Path>) -> Result<Session, CommandError> {
        Session::open_with(Registry::with_builtins(), path)
    }

    pub fn open_with(registry: Registry, path: impl AsRef<Path>) -> Result<Session, CommandError> {
        let path = path.as_ref();
        let store_error = |e: zd_io::StoreError| CommandError::Store(e.to_string());
        let (project, run_log, links, store) = if path.is_dir() {
            let opened = ProjectStore::open(path).map_err(store_error)?;
            (opened.project, opened.run_log, Vec::new(), None)
        } else {
            let (store, read) = ZdpStore::open(path).map_err(store_error)?;
            (read.project, read.run_log, read.links, Some(Arc::new(Mutex::new(store))))
        };
        let mut log: Vec<LogEntry> = match run_log.as_deref().filter(|t| !t.is_empty()) {
            Some(text) => serde_json::from_str(text).map_err(|e| CommandError::Store(format!("run log: {e}")))?,
            None => Vec::new(),
        };
        log.push(LogEntry::Opened { path: path.display().to_string(), at_unix_ms: now_ms() });
        // An old folder hasn't been saved as a file yet; a file just read has nothing new to save.
        let saved_log_len = if store.is_some() { log.len() } else { 0 };
        let mut session = Session { project, registry, log, store, saved_log_len, open_notes: Vec::new() };
        session.relink_beside(&links, path);
        Ok(session)
    }

    /// Tries both of each grid folder's locations (decision 0058, stage 4): where a seam model reads from a folder
    /// that isn't there, but is found where it was relative to the project file, the model is relinked to it by a
    /// `seam.relink` run, logged and undone like any other. A folder found neither way is left for the user to relink.
    fn relink_beside(&mut self, links: &[FolderLink], file: &Path) {
        let Some(base) = std::path::absolute(file).ok().and_then(|f| f.parent().map(Path::to_path_buf)) else { return };
        for link in links {
            let was = Path::new(&link.folder);
            let reads_it = |project: &Project| match project.object(link.object).map(|o| &o.body) {
                Some(Body::SeamModel(model)) => model.folders().iter().any(|f| same_path(Path::new(f), was)),
                _ => false,
            };
            if was.is_dir() || !reads_it(&self.project) {
                continue;
            }
            let name = self.project.object(link.object).map(|o| o.name.clone()).unwrap_or_default();
            let Some(found) = link.from_project.as_ref().map(|rel| lexically_normal(&base.join(rel))).filter(|f| f.is_dir() && !same_path(f, was)) else {
                self.open_notes.push(format!("{name}: its grids aren't in {}, nor where they were beside the project", link.folder));
                continue;
            };
            let params = serde_json::json!({ "from": link.folder, "to": found.to_string_lossy(), "models": [link.object] });
            match self.run("seam.relink", params) {
                Ok(_) => self.open_notes.push(format!("{name}: its grids aren't in {}; found beside the project in {} and relinked", link.folder, found.display())),
                Err(e) => self.open_notes.push(format!("{name}: its grids aren't in {}; {} beside the project was tried: {e}", link.folder, found.display())),
            }
        }
    }

    /// What opening did beyond reading: seam models relinked to grid folders found beside the project, and folders
    /// found neither where they were nor beside it.
    pub fn open_notes(&self) -> &[String] {
        &self.open_notes
    }

    /// Every seam model's grid files checked where they are now: missing, or changed since the model was built.
    pub fn links(&self) -> Vec<ModelLinks> {
        seam_links(&self.project)
    }

    /// Saves the project and its run log as a `.zdp` file (a `.zdproj` path saves as a `.zdp` beside it). Saving
    /// again re-encodes only what changed, and writes nothing when nothing has.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<SavedFile, CommandError> {
        let job = self.begin_save(path)?;
        let result = job.run();
        self.end_save(job, result)
    }

    /// Takes a save in an instant: run it with [`SaveJob::run`] while the session carries on, then hand its result
    /// to [`Session::end_save`].
    pub fn begin_save(&mut self, path: impl AsRef<Path>) -> Result<SaveJob, CommandError> {
        let path = zdp_path(path.as_ref());
        let same = self.store.as_ref().is_some_and(|s| s.lock().unwrap_or_else(|p| p.into_inner()).path() == path);
        if !same {
            self.store = Some(Arc::new(Mutex::new(ZdpStore::create(&path))));
        }
        let saved = LogEntry::Saved { path: path.display().to_string(), at_unix_ms: now_ms() };
        let mut log = self.log.clone();
        log.push(saved.clone());
        Ok(SaveJob {
            store: self.store.clone().expect("store set above"),
            snapshot: Snapshot::of(&self.project),
            log: serde_json::to_string(&log).map_err(|e| CommandError::Store(e.to_string()))?,
            log_changed: self.log.len() != self.saved_log_len,
            saved,
        })
    }

    /// Records a save once it has run: the log notes it, unless nothing was written.
    pub fn end_save(&mut self, job: SaveJob, result: Result<SavedFile, CommandError>) -> Result<SavedFile, CommandError> {
        let saved = result?;
        if !saved.unchanged {
            self.log.push(job.saved);
            self.saved_log_len = self.log.len();
        }
        Ok(saved)
    }

    /// The file this session last saved to or opened, if any.
    pub fn file(&self) -> Option<PathBuf> {
        self.store.as_ref().map(|s| s.lock().unwrap_or_else(|p| p.into_inner()).path().to_path_buf())
    }

    /// Who else has this session's file open, when someone does: a save to it is refused.
    pub fn locked_by(&self) -> Option<String> {
        self.store.as_ref()?.lock().unwrap_or_else(|p| p.into_inner()).locked_by().map(|l| l.describe())
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

    /// See [`Project::set_objects_visible`].
    pub fn set_objects_visible(&mut self, objects: &[ObjectId], visible: bool) -> usize {
        self.project.set_objects_visible(objects, visible)
    }

    /// See [`Project::set_looks`]. Display state only: not a command, not undone.
    pub fn set_looks(&mut self, objects: &[ObjectId], colouring: Option<Colouring>, opacity: Option<f64>) -> usize {
        self.project.set_looks(objects, colouring, opacity)
    }

    /// See [`Project::set_layer_opacity`]. Display state only.
    pub fn set_layer_opacity(&mut self, layer: LayerId, opacity: f64) -> bool {
        self.project.set_layer_opacity(layer, opacity)
    }

    /// See [`Project::set_contours`]. Display state only.
    pub fn set_contours(&mut self, contours: Contours) -> bool {
        self.project.set_contours(contours)
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
            notes: Vec::new(),
            messages: Vec::new(),
            status: RunStatus::Ok,
            refused: None,
        };
        let result = self.execute(run, command, &params, &mut record);
        record.duration_ms = clock.elapsed().as_secs_f64() * 1e3;
        if let Err(e) = &result {
            record.status = RunStatus::Failed(e.to_string());
            record.refused = Some(e.code());
        }
        self.log.push(LogEntry::Run(record.clone()));
        result.map(|()| record)
    }

    fn execute(&mut self, run: RunId, command: &str, params: &Value, record: &mut RunRecord) -> Result<(), CommandError> {
        let command = self.registry.get(command).ok_or_else(|| CommandError::UnknownCommand(command.to_string()))?;
        let inputs = Inputs::new(&self.project);
        // A panic anywhere in a run, the geometry included, refuses that run and leaves the project as it was: commands
        // only read the project, and nothing is applied until they're done. Without this a panic would end the app's
        // session with the lock held (the review of 10 November).
        let guarded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (mut outcome, canonical) = command.run_json(&inputs, params)?;
            let checked = check_solids_made(&mut outcome.changes, &mut outcome.messages);
            Ok((outcome, canonical, checked))
        }));
        record.inputs = inputs.into_read();
        let (outcome, canonical, checked) = guarded.unwrap_or_else(|panic| Err(stopped_on_a_panic(record.command.as_str(), panic.as_ref())))?;
        record.params = canonical;
        record.notes = outcome.messages.into_iter().map(Note::info).chain(outcome.notes).collect();
        record.messages = record.notes.iter().map(|n| n.text.clone()).collect();
        checked?;
        let ids = changed_objects(&outcome.changes);
        let applied = self.project.apply(run, outcome.changes)?;
        let made = ids.resolve(&applied.changes);
        for note in &mut record.notes {
            if let Some(index) = note.made.take() {
                note.object = made.get(index).copied().flatten();
            }
        }
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
