//! A panic in a run refuses that run and leaves the project as it was (the review of 10 November: a panic in a face
//! command would end the app's session with its lock held).

use serde::{Deserialize, Serialize};
use serde_json::json;
use zd_commands::{Command, CommandError, CommandSpec, Inputs, Outcome, Registry, RunStatus, Session};

/// A command that panics part way through, as a fault deep in the geometry would.
struct Panics;

#[derive(Serialize, Deserialize)]
struct NoParams {}

impl Command for Panics {
    type Params = NoParams;
    const NAME: &'static str = "test.panics";

    fn spec(&self) -> CommandSpec {
        CommandSpec { name: Self::NAME, summary: "panics", inputs: Vec::new(), params: Vec::new(), outputs: Vec::new() }
    }

    fn run(&self, _: &Inputs<'_>, _: &NoParams) -> Result<Outcome, CommandError> {
        panic!("a hip with nowhere to go");
    }
}

#[test]
fn a_panic_refuses_the_run_and_changes_nothing() {
    let mut registry = Registry::with_builtins();
    registry.register(Panics);
    let mut s = Session::new(registry);
    s.run("string.create", json!({ "name": "toe", "layer": "toes", "points": [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]] })).unwrap();
    let before: Vec<_> = s.project().objects().map(|o| (o.id, o.revision)).collect();

    let refused = s.run("test.panics", json!({})).unwrap_err();
    let said = refused.to_string();
    assert!(said.contains("test.panics stopped on an error in the engine and changed nothing (a hip with nowhere to go)"), "{said}");
    let after: Vec<_> = s.project().objects().map(|o| (o.id, o.revision)).collect();
    assert_eq!(before, after, "nothing changed");
    let Some(zd_commands::LogEntry::Run(record)) = s.log().last() else { panic!("the run is logged") };
    assert!(matches!(&record.status, RunStatus::Failed(why) if why.contains("a hip with nowhere to go")));

    // The session carries on.
    s.run("string.create", json!({ "name": "next", "layer": "toes", "points": [[0.0, 10.0, 0.0], [100.0, 10.0, 0.0]] })).unwrap();
    assert_eq!(s.project().objects().count(), 2);
}
