//! Typed commands for the Zovlyn Pit Design Engine.
//!
//! Every operation is a command with named inputs, typed parameters and outputs, and every run
//! leaves a serialisable record. A command reads the project through [`Inputs`] and returns a
//! changeset; the [`Session`] applies it in one step, logs the run, and does undo and redo.
//! Commands never change their inputs: an edit becomes a new revision, a cut becomes new objects.

pub mod builtin;
mod command;
mod control;
mod note;
mod error;
mod registry;
mod session;
mod spec;
pub mod workflow;
mod workflow_run;

pub use command::{Command, Inputs, Outcome};
pub use control::{Progress, RunControl};
pub use error::CommandError;
pub use note::{Code, Measure, Note, Place, Severity};
pub use registry::Registry;
pub use session::{LogEntry, RunDone, RunJob, RunRecord, RunStatus, SaveJob, Session};
pub use spec::{CommandSpec, Field, FieldType};
pub use workflow_run::{StepDone, StepJob, StepOutcome, StepRun, WorkflowRecord, WorkflowRun, WorkflowStatus};
