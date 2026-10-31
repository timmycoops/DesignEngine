use std::cell::RefCell;
use std::sync::Arc;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use zd_core::{Body, Change, Object, ObjectId, ObjectKind, ObjectRef, Polyline, Project, Solid, Surface};

use crate::{CommandError, CommandSpec};

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

/// What a run asks for: the changes to apply together, and messages for the run record
/// (repair logs, warnings, timings).
#[derive(Debug, Default)]
pub struct Outcome {
    pub changes: Vec<Change>,
    pub messages: Vec<String>,
}

impl Outcome {
    pub fn new(changes: Vec<Change>) -> Self {
        Outcome { changes, messages: Vec::new() }
    }

    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.messages.push(message.into());
        self
    }
}

/// Read-only access to the project for a running command. It records every object revision the
/// command reads, so the run record lists its real inputs.
pub struct Inputs<'a> {
    project: &'a Project,
    read: RefCell<Vec<ObjectRef>>,
}

impl<'a> Inputs<'a> {
    pub(crate) fn new(project: &'a Project) -> Self {
        Inputs { project, read: RefCell::new(Vec::new()) }
    }

    pub(crate) fn into_read(self) -> Vec<ObjectRef> {
        self.read.into_inner()
    }

    /// The current revision of a live object.
    pub fn object(&self, id: ObjectId) -> Result<&'a Arc<Object>, CommandError> {
        let object = self.project.object(id).ok_or(CommandError::MissingObject(id))?;
        let reference = ObjectRef { id, revision: object.revision };
        let mut read = self.read.borrow_mut();
        if !read.contains(&reference) {
            read.push(reference);
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
