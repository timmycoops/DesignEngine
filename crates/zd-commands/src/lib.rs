//! Typed commands for the Zovlyn Pit Design Engine.
//!
//! Every operation is a command with named inputs, typed parameters and outputs, and every run
//! leaves a serialisable record. A command reads the project through [`Inputs`] and returns a
//! changeset; the [`Session`] applies it in one step, logs the run, and does undo and redo.
//! A command reads its inputs and asks for changes: an edit is the object's next revision, in place, or, run with
//! `result` as new, a new object on a layer with the original left as it was (decision 0091); a cut makes new objects.

pub mod builtin;
mod command;
mod control;
mod note;
mod error;
pub mod modules;
mod registry;
pub mod report;
pub mod reserves_view;
pub mod table_calc;
pub mod table_grid;
mod report_html;
mod result_as;
mod session;
mod spec;
pub mod workflow;
mod workflow_run;

pub use command::{Command, Inputs, Outcome};
pub use control::{Progress, RunControl};
pub use error::CommandError;
pub use note::{Code, Measure, Note, Place, Severity};
pub use modules::Module;
pub use registry::Registry;
pub use result_as::ResultAs;
pub use session::{LogEntry, RunDone, RunJob, RunRecord, RunStatus, SaveJob, Session};
pub use spec::{CommandSpec, Field, FieldType};
pub use workflow_run::{StepDone, StepJob, StepOutcome, StepRun, WorkflowRecord, WorkflowRun, WorkflowStatus};
