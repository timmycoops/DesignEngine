use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use zd_core::{Body, Change, Layer, LayerId, Object, ObjectId, ObjectKind, ObjectRef, Polyline, Project, Solid, Surface};

use crate::control::RunControl;
use crate::{Code, CommandError, CommandSpec, Note};

/// One operation. Implementations are stateless: everything a run needs comes in `params` and
/// through `inputs`, and everything it makes goes out as the changeset in its [`Outcome`].
pub trait Command: Send + Sync + 'static {
    /// Input object ids and parameter values, as JSON.
    type Params: Serialize + DeserializeOwned;
    /// The registry name, e.g. `string.move_vertex`.
    const NAME: &'static str;

    fn spec(&self) -> CommandSpec;

    fn run(&self, inputs: &Inputs<'_>, params: &Self::Params) -> Result<Outcome, CommandError>;
}

/// What a run asks for: the changes to apply together, and what it reports, a note a line in the order said (decision
/// 0059).
#[derive(Debug, Default)]
pub struct Outcome {
    pub changes: Vec<Change>,
    pub notes: Vec<Note>,
}

impl Outcome {
    pub fn new(changes: Vec<Change>) -> Self {
        Outcome { changes, notes: Vec::new() }
    }

    /// A plain sentence: what the run did, as an info note.
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.notes.push(Note::info(message));
        self
    }

    /// Plain sentences, each an info note, in order.
    pub fn said(&mut self, lines: impl IntoIterator<Item = impl Into<String>>) {
        self.notes.extend(lines.into_iter().map(Note::info));
    }

    /// Sentences that each warn of `code`, in order.
    pub fn warned(&mut self, code: Code, lines: impl IntoIterator<Item = impl Into<String>>) {
        self.notes.extend(lines.into_iter().map(|line| Note::warning(code, line)));
    }
}

/// Read-only access to the project for a running command. It records every object revision the
/// command reads, so the run record lists its real inputs. A long command also says through it how far it has got,
/// and checks at each solid, strip or file whether it has been cancelled (decision 0060).
pub struct Inputs<'a> {
    project: &'a Project,
    /// What it read, in order, and the same as a set: a run over 43,000 solids reads 43,000 of them.
    read: Mutex<(Vec<ObjectRef>, HashSet<ObjectRef>)>,
    control: Arc<RunControl>,
}

impl<'a> Inputs<'a> {
    pub(crate) fn new(project: &'a Project, control: Arc<RunControl>) -> Self {
        Inputs { project, read: Mutex::new((Vec::new(), HashSet::new())), control }
    }

    /// Inputs for a run no one can cancel or watch, as in a test of a command's own.
    pub fn of(project: &'a Project) -> Self {
        Inputs::new(project, Arc::new(RunControl::new()))
    }

    /// How far the run has got: `done` of `total` `of` ("solids", "strips", "files").
    pub fn progress(&self, done: usize, total: usize, of: &'static str) {
        self.control.set(done, total, of);
    }

    /// One more done of the total last given, for work spread over threads.
    pub fn step(&self) {
        self.control.step();
    }

    /// Refuses the run if it has been cancelled: call it at each solid, strip or file, and pass the refusal on.
    pub fn stop_if_cancelled(&self) -> Result<(), CommandError> {
        if self.control.is_cancelled() { Err(CommandError::Cancelled) } else { Ok(()) }
    }

    pub(crate) fn into_read(self) -> Vec<ObjectRef> {
        self.read.into_inner().expect("reads aren't left half done").0
    }

    /// A layer by name, which must exist.
    pub fn layer(&self, name: &str) -> Result<&'a Layer, CommandError> {
        self.project.layer_named(name).ok_or_else(|| CommandError::Invalid(format!("no layer '{name}'")))
    }

    /// Every live object on a layer.
    pub fn objects_on(&self, layer: LayerId) -> Result<Vec<&'a Arc<Object>>, CommandError> {
        let ids: Vec<ObjectId> = self.project.objects().filter(|o| o.layer == layer).map(|o| o.id).collect();
        ids.into_iter().map(|id| self.object(id)).collect()
    }

    /// The current revision of a live object.
    pub fn object(&self, id: ObjectId) -> Result<&'a Arc<Object>, CommandError> {
        let object = self.project.object(id).ok_or(CommandError::MissingObject(id))?;
        let reference = ObjectRef { id, revision: object.revision };
        let mut read = self.read.lock().expect("reads aren't left half done");
        if read.1.insert(reference) {
            read.0.push(reference);
        }
        Ok(object)
    }

    pub fn string(&self, id: ObjectId) -> Result<(&'a Arc<Object>, &'a Polyline), CommandError> {
        let object = self.object(id)?;
        match &object.body {
            Body::String(line) => Ok((object, line)),
            other => Err(wrong_kind(id, ObjectKind::String, other)),
        }
    }

    pub fn surface(&self, id: ObjectId) -> Result<(&'a Arc<Object>, &'a Surface), CommandError> {
        let object = self.object(id)?;
        match &object.body {
            Body::Surface(surface) => Ok((object, surface)),
            other => Err(wrong_kind(id, ObjectKind::Surface, other)),
        }
    }

    pub fn solid(&self, id: ObjectId) -> Result<(&'a Arc<Object>, &'a Solid), CommandError> {
        let object = self.object(id)?;
        match &object.body {
            Body::Solid(solid) => Ok((object, solid)),
            other => Err(wrong_kind(id, ObjectKind::Solid, other)),
        }
    }

    /// Every live object, to find what an earlier run made. Listing them is not reading them as inputs.
    pub fn objects(&self) -> impl Iterator<Item = &'a Arc<Object>> {
        self.project.objects()
    }

    /// The name of an object's layer (display state, so reading it is not an input).
    pub fn layer_name(&self, object: &Object) -> String {
        self.project.layer(object.layer).map(|l| l.name.clone()).unwrap_or_default()
    }
}

fn wrong_kind(id: ObjectId, expected: ObjectKind, found: &Body) -> CommandError {
    CommandError::WrongKind { id, expected, found: found.kind() }
}

/// A command behind JSON parameters, so the registry can hold every command in one map and the
/// app bridge and Python can call any of them by name.
pub(crate) trait DynCommand: Send + Sync {
    fn spec(&self) -> CommandSpec;

    /// Runs with JSON parameters; returns the outcome and the parameters as the command read them.
    fn run_json(&self, inputs: &Inputs<'_>, params: &Value) -> Result<(Outcome, Value), CommandError>;
}

impl<C: Command> DynCommand for C {
    fn spec(&self) -> CommandSpec {
        Command::spec(self)
    }

    fn run_json(&self, inputs: &Inputs<'_>, params: &Value) -> Result<(Outcome, Value), CommandError> {
        let typed: C::Params =
            serde_json::from_value(params.clone()).map_err(|e| CommandError::BadParams(e.to_string()))?;
        let canonical = serde_json::to_value(&typed).map_err(|e| CommandError::BadParams(e.to_string()))?;
        Ok((self.run(inputs, &typed)?, canonical))
    }
}
