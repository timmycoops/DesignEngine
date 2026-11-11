//! What a run reports, as notes (decision 0059, workstream 1): messages read as info notes with their text unchanged,
//! a note on what a run made names it once it has an id, a refusal carries its kind as a code, and run logs saved
//! before notes read back whole.

use serde::{Deserialize, Serialize};
use serde_json::json;
use zd_commands::{Code, Command, CommandError, CommandSpec, Inputs, LogEntry, Note, Outcome, Place, Registry, RunStatus, Session, Severity};
use zd_core::{Attributes, Body, Change, ObjectId, Polyline};

/// Makes a string and offsets it by revising an input, reporting both ways a command can: a plain message, and
/// notes on what it read, what it made, a place and a value.
struct Reports;

#[derive(Serialize, Deserialize)]
struct ReportsParams {
    object: ObjectId,
}

impl Command for Reports {
    type Params = ReportsParams;
    const NAME: &'static str = "test.reports";

    fn spec(&self) -> CommandSpec {
        CommandSpec { name: Self::NAME, summary: "reports", inputs: Vec::new(), params: Vec::new(), outputs: Vec::new() }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &ReportsParams) -> Result<Outcome, CommandError> {
        let read = inputs.object(p.object)?;
        let line = Polyline { vertices: vec![[650_000.0, 7_450_000.0, 96.5], [650_100.0, 7_450_000.0, 97.25]], closed: false };
        let mut outcome = Outcome::new(vec![
            Change::CreateLayer { name: "made".into(), colour: None },
            Change::Revise { id: read.id, base: read.revision, name: Some("toe, read".into()), layer: None, attributes: None, body: None },
            Change::Create { name: "crest".into(), layer: "made".into(), attributes: Attributes::new(), body: Body::String(line) },
        ])
        .message("2 strings in 1.2 ms");
        outcome.notes.push(Note::info("read the toe").on(read.id));
        outcome.notes.push(
            Note::warning(Code::RefusedGeometry, "the crest has no height for 42 m")
                .on_made(2)
                .at(Place::Stretch { from: [650_000.0, 7_450_000.0, 96.5], to: [650_042.0, 7_450_000.0, 96.8] })
                .measured(42.0, "m"),
        );
        outcome.notes.push(Note::info("revised the toe").on_made(1));
        Ok(outcome)
    }
}

fn session() -> (Session, ObjectId) {
    let mut registry = Registry::with_builtins();
    registry.register(Reports);
    let mut s = Session::new(registry);
    let made = s.run("string.create", json!({ "name": "toe", "layer": "toes", "points": [[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]] })).unwrap();
    let toe = made.outputs[0].id;
    (s, toe)
}

#[test]
fn messages_are_info_notes_and_a_note_names_what_the_run_made() {
    let (mut s, toe) = session();
    let record = s.run("test.reports", json!({ "object": toe })).unwrap();
    let texts: Vec<&str> = record.notes.iter().map(|n| n.text.as_str()).collect();
    assert_eq!(texts, ["2 strings in 1.2 ms", "read the toe", "the crest has no height for 42 m", "revised the toe"], "messages first, then notes");
    assert_eq!(record.messages, texts, "the sentences as before");
    assert_eq!((record.notes[0].severity, record.notes[0].code), (Severity::Info, None));

    let crest = s.project().objects().find(|o| o.name == "crest").unwrap().id;
    let warning = &record.notes[2];
    assert_eq!((warning.severity, warning.object, warning.made), (Severity::Warning, Some(crest), None), "named once it has an id");
    assert_eq!(warning.value.as_ref().map(|v| (v.value, v.unit.as_str())), Some((42.0, "m")));
    assert_eq!(record.notes[1].object, Some(toe));
    assert_eq!(record.notes[3].object, Some(toe), "a revised input by its own id");
    assert_eq!(record.refused, None);

    // The log keeps the notes, places exactly.
    let saved = serde_json::to_string(s.log()).unwrap();
    let back: Vec<LogEntry> = serde_json::from_str(&saved).unwrap();
    assert_eq!(back, s.log());
}

#[test]
fn a_refused_run_carries_its_kind() {
    let (mut s, _) = session();
    let missing = ObjectId::new();
    let refused = s.run("test.reports", json!({ "object": missing })).unwrap_err();
    assert_eq!(refused.code(), Code::RefusedMissingObject);
    let Some(LogEntry::Run(record)) = s.log().last() else { panic!("the run is logged") };
    assert!(matches!(record.status, RunStatus::Failed(_)));
    assert_eq!(record.refused, Some(Code::RefusedMissingObject));
    assert!(record.notes.is_empty());
    assert_eq!(s.run("no.such", json!({})).unwrap_err().code(), Code::RefusedUnknownCommand);
    assert_eq!(s.run("string.create", json!({ "nme": "x" })).unwrap_err().code(), Code::RefusedBadParams);
}

/// A run saved before notes holds only its messages: they read back as info notes, nothing lost.
#[test]
fn a_run_log_saved_before_notes_reads_back_whole() {
    let old = json!([{
        "event": "run",
        "run": zd_core::RunId::new(),
        "command": "face.project_step",
        "params": { "berm": 10.0 },
        "inputs": [],
        "outputs": [],
        "layers_created": [],
        "started_unix_ms": 1_794_387_180_000u64,
        "duration_ms": 146.5,
        "messages": ["30 faces", "Strip 7: 42 m of crest has no height"],
        "status": "ok"
    }]);
    let log: Vec<LogEntry> = serde_json::from_value(old).unwrap();
    let LogEntry::Run(record) = &log[0] else { panic!("a run") };
    assert_eq!(record.messages, ["30 faces", "Strip 7: 42 m of crest has no height"]);
    assert_eq!(record.notes, [Note::info("30 faces"), Note::info("Strip 7: 42 m of crest has no height")]);
    assert_eq!(record.refused, None);
}
