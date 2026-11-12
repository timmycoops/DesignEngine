//! A run taken off the session, watched and cancelled (decision 0060, workstream 2): the session carries on while it
//! runs, what it made is applied at the end in one step, and a cancelled run, or one whose project changed meanwhile,
//! applies nothing and is logged as refused.

use serde_json::json;
use zd_commands::{Code, CommandError, LogEntry, Session};
use zd_core::ObjectId;

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// `n` 10 m cubes in a row, RL 100 to 110: 1,000 m³ each.
fn cubes(n: usize) -> (Session, Vec<ObjectId>) {
    let mut s = Session::with_builtins();
    let ids = (0..n)
        .map(|k| {
            let x = E + 20.0 * k as f64;
            let v: Vec<[f64; 3]> = (0..8).map(|i| [x + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]).collect();
            let t = [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
            s.run("solid.from_mesh", json!({ "name": format!("block {k}"), "layer": "blocks", "vertices": v, "triangles": t })).unwrap().outputs[0].id
        })
        .collect();
    (s, ids)
}

fn cut_at_105(ids: &[ObjectId]) -> serde_json::Value {
    json!({ "solids": ids, "rl": 105.0, "keep": "below", "layer": "cut" })
}

fn last_refusal(s: &Session) -> Option<Code> {
    match s.log().last() {
        Some(LogEntry::Run(r)) => r.refused,
        _ => None,
    }
}

#[test]
fn a_run_off_the_session_says_how_far_it_got_and_applies_at_the_end() {
    let (mut s, ids) = cubes(12);
    let before = s.project().objects().count();
    let job = s.begin_run("solid.cut_by_surface", cut_at_105(&ids));
    let control = job.control();
    assert_eq!(job.command(), "solid.cut_by_surface");
    let done = job.run();
    // The session's project stands until the run is applied, and can be read meanwhile.
    assert_eq!(s.project().objects().count(), before, "nothing applied while it runs");
    let progress = control.progress();
    assert_eq!((progress.done, progress.total, progress.of), (12, 12, "solids"));
    let record = s.end_run(done).unwrap();
    assert_eq!(record.outputs.len(), 12, "a part below 105 for each block");
    assert_eq!(s.project().objects().count(), before + 12);
    let volume: f64 = record.outputs.iter().map(|o| match &s.project().object(o.id).unwrap().body {
        zd_core::Body::Solid(solid) => solid.volume(),
        _ => 0.0,
    }).sum();
    assert!((volume - 12.0 * 500.0).abs() < 1e-6, "12 halves of 1,000 m³: {volume}");
}

#[test]
fn a_cancelled_run_applies_nothing_and_is_logged_as_cancelled() {
    let (mut s, ids) = cubes(12);
    let before = s.project().objects().count();
    let job = s.begin_run("solid.cut_by_surface", cut_at_105(&ids));
    job.control().cancel();
    let error = s.end_run(job.run()).unwrap_err();
    assert_eq!(error, CommandError::Cancelled);
    assert_eq!(s.project().objects().count(), before);
    assert_eq!(last_refusal(&s), Some(Code::RefusedCancelled));

    // Cancelled after it has made everything, but before it's applied: still nothing.
    let job = s.begin_run("solid.cut_by_surface", cut_at_105(&ids));
    let control = job.control();
    let done = job.run();
    control.cancel();
    assert_eq!(s.end_run(done).unwrap_err(), CommandError::Cancelled);
    assert_eq!(s.project().objects().count(), before);
}

#[test]
fn a_run_whose_project_changed_while_it_ran_applies_nothing() {
    let (mut s, ids) = cubes(3);
    let job = s.begin_run("solid.cut_by_surface", cut_at_105(&ids));
    let done = job.run();
    s.run("object.rename", json!({ "object": ids[0], "name": "renamed" })).unwrap();
    assert_eq!(s.end_run(done).unwrap_err(), CommandError::Changed);
    assert_eq!(last_refusal(&s), Some(Code::RefusedChanged));
    assert_eq!(s.project().objects().count(), 3, "the rename stands, the cut doesn't");
}
