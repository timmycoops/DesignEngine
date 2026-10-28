//! Typed commands for the Zovlyn Pit Design Engine.
//!
//! Every operation is a command with named inputs, typed parameters and outputs, and every run
//! leaves a serialisable record. A command reads the project through [`Inputs`] and returns a
//! changeset; the [`Session`] applies it in one step, logs the run, and does undo and redo.
//! Commands never change their inputs: an edit becomes a new revision, a cut becomes new objects.

pub mod builtin;
mod command;
mod error;
mod registry;
mod session;
mod spec;

pub use command::{Command, Inputs, Outcome};
pub use error::CommandError;
pub use registry::Registry;
pub use session::{LogEntry, RunRecord, RunStatus, Session};
pub use spec::{CommandSpec, Field, FieldType};
