//! `solid.compare`: blocks set beside another package's numbers (decision 0061, workstream 4). Each row of a match file
//! names a field of theirs, as it came in on the blocks from Spry or Deswik, and the value of ours it's set beside: an
//! attribute the interrogation wrote, or `volume`, the block's own. Each says how many blocks were compared, how many are
//! within tolerance, the worst and the mean difference. Nothing in the project changes: it's a report, kept in the run log.

use std::path::Path;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Object, ObjectId, Solid};
use zd_io::{Tolerance, read_value_match};

use crate::{Code, Command, CommandError, CommandSpec, Field, FieldType, Inputs, Note, Outcome, Place};

pub struct CompareSolids;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompareSolidsParams {
    /// The blocks.
    pub solids: Vec<ObjectId>,
    /// The match file: `source,ours,tolerance`.
    pub matches: String,
    /// What the numbers set beside ours are called in the run's notes: "Spry", "Deswik".
    #[serde(default = "source_default")]
    pub source: String,
    /// Our density's code, whose tolerance is 0.005 unless the match file says otherwise.
    #[serde(default = "density_default")]
    pub density: String,
}

fn source_default() -> String {
    "theirs".into()
}

fn density_default() -> String {
    "rd".into()
}

/// What `ours` names for the block's own volume.
const VOLUME: &str = "volume";

/// A value's tolerance when the match file leaves it blank (decision 0061, decision 6): volume 0.1 %, density 0.005 and
/// any other 0.05 in its own unit.
fn usual_tolerance(ours: &str, density: &str) -> Tolerance {
    if ours.eq_ignore_ascii_case(VOLUME) {
        Tolerance::Share(0.001)
    } else if ours.eq_ignore_ascii_case(density) {
        Tolerance::Within(0.005)
    } else {
        Tolerance::Within(0.05)
    }
}

fn number(object: &Object, name: &str) -> Option<f64> {
    match object.attributes.get(name)? {
        AttrValue::Number(n) => Some(*n),
        AttrValue::Text(t) => t.trim().parse().ok(),
        AttrValue::Bool(_) => None,
    }
    .filter(|v: &f64| v.is_finite())
}

/// The furthest of a row's blocks: by how much, which block, where, and the two values.
struct Worst<'a> {
    off: f64,
    name: &'a str,
    id: ObjectId,
    at: [f64; 3],
    theirs: f64,
    ours: f64,
}

impl Command for CompareSolids {
    type Params = CompareSolidsParams;
    const NAME: &'static str = "solid.compare";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Set blocks beside another package's numbers, by a match file, and say how near they are",
            inputs: vec![Field::new("solids", FieldType::Objects, "The blocks")],
            params: vec![
                Field::new("matches", FieldType::Path, "The match file: source,ours,tolerance"),
                Field::new("source", FieldType::Text, "What their numbers are called in the notes: Spry, Deswik"),
                Field::new("density", FieldType::Text, "Our density's code, 0.005 unless the file says (rd)"),
            ],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        if p.solids.is_empty() {
            return Err(CommandError::Invalid("give the blocks to compare".into()));
        }
        let rows = read_value_match(Path::new(&p.matches)).map_err(|e| CommandError::Io(e.to_string()))?;
        if rows.is_empty() {
            return Err(CommandError::Invalid(format!("{} matches nothing: give a row a value", p.matches)));
        }
        let blocks: Vec<(&Object, &Solid)> = p.solids.iter().map(|&id| inputs.solid(id).map(|(o, s)| (o.as_ref(), s))).collect::<Result<_, _>>()?;
        inputs.progress(0, rows.len(), "values");
        let mut outcome = Outcome::default();
        for (k, row) in rows.iter().enumerate() {
            inputs.stop_if_cancelled()?;
            let tolerance = row.tolerance.unwrap_or_else(|| usual_tolerance(&row.ours, &p.density));
            let (mut compared, mut within, mut no_theirs, mut no_ours) = (0usize, 0usize, 0usize, 0usize);
            let (mut sum_theirs, mut sum_ours, mut sum_off) = (0.0, 0.0, 0.0);
            let mut worst: Option<Worst<'_>> = None;
            for (object, solid) in &blocks {
                let theirs = number(object, &row.source);
                let ours = if row.ours.eq_ignore_ascii_case(VOLUME) { Some(solid.volume()) } else { number(object, &row.ours) };
                let (Some(theirs), Some(ours)) = (theirs, ours) else {
                    no_theirs += usize::from(theirs.is_none());
                    no_ours += usize::from(ours.is_none());
                    continue;
                };
                compared += 1;
                sum_theirs += theirs;
                sum_ours += ours;
                let off = ours - theirs;
                sum_off += off;
                let allowed = match tolerance {
                    Tolerance::Share(share) => share * theirs.abs(),
                    Tolerance::Within(within) => within,
                };
                let over = off.abs() - allowed;
                if over <= 1e-12 * theirs.abs().max(1.0) {
                    within += 1;
                } else if worst.as_ref().is_none_or(|w| off.abs() > w.off.abs()) {
                    let at = solid.mesh().bounds().map_or([0.0; 3], |b| b.centre());
                    worst = Some(Worst { off, name: object.name.as_str(), id: object.id, at, theirs, ours });
                }
            }
            let said = match tolerance {
                Tolerance::Share(share) => format!("{} %", share * 100.0),
                Tolerance::Within(within) => format!("{within}"),
            };
            let mut line = format!("{} {} against {}: {compared} of {} blocks compared, {within} within {said}", p.source, row.source, row.ours, blocks.len());
            if compared > 0 {
                line.push_str(&format!("; {} {sum_theirs:.3} against ours {sum_ours:.3} summed, mean difference {:+.4}", p.source, sum_off / compared as f64));
            }
            if no_theirs > 0 {
                line.push_str(&format!("; {no_theirs} with no {}", row.source));
            }
            if no_ours > 0 {
                line.push_str(&format!("; {no_ours} with no {}", row.ours));
            }
            outcome.notes.push(Note::info(line));
            if let Some(w) = worst {
                let beyond = compared - within;
                outcome.notes.push(
                    Note::warning(Code::CompareBeyond, format!(
                        "{} {} against {}: {beyond} block{} beyond {said}; the furthest '{}', {} {:.4} against ours {:.4} ({:+.4})",
                        p.source, row.source, row.ours, if beyond == 1 { "" } else { "s" }, w.name, p.source, w.theirs, w.ours, w.off
                    ))
                    .measured(beyond as f64, "blocks")
                    .at(Place::Point { at: w.at })
                    .on(w.id),
                );
            }
            inputs.progress(k + 1, rows.len(), "values");
        }
        Ok(outcome)
    }
}
