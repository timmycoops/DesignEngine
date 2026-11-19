//! In place or as new on a layer: one rule for every tool (Surface tools plan, decision 6; decision 0091). A tool that
//! changes an object does it in place by default, as the object's next revision, one undo putting it back. Run with
//! `"result": {"as_new": true, "layer": "…"}` among its parameters, the same run instead makes each changed object anew:
//! a copy with its original's name and attributes as they are (Tim, 20 November: nothing added about what changed; its
//! own id tells it apart), on the layer given (each on its original's layer if none is), and leaves the original as it
//! was, deleting nothing. It's done here, once, for every command: a command
//! never sees `result`, and a new one gets the choice without code of its own.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zd_core::{Change, ObjectId, Project};

use crate::{CommandError, Note};

/// The parameter every command may take, beside its own.
pub(crate) const RESULT: &str = "result";

/// How a run's changes land: in place, or as new objects on a layer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultAs {
    /// Each changed object made anew, the original left as it was.
    #[serde(default)]
    pub as_new: bool,
    /// The layer the new objects go on; each on its original's layer if left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
}

/// Takes `result` out of a run's parameters, so the command reads only its own.
pub(crate) fn take_result(params: &Value) -> Result<(Value, Option<ResultAs>), CommandError> {
    let Some(object) = params.as_object() else { return Ok((params.clone(), None)) };
    let Some(given) = object.get(RESULT) else { return Ok((params.clone(), None)) };
    let result: ResultAs = serde_json::from_value(given.clone()).map_err(|e| CommandError::BadParams(format!("`result`: {e}")))?;
    if result.layer.as_deref().is_some_and(|l| l.trim().is_empty()) {
        return Err(CommandError::BadParams("`result`: the layer for the new objects is blank; leave it out to keep each on its original's layer".into()));
    }
    let mut own = object.clone();
    own.remove(RESULT);
    Ok((Value::Object(own), Some(result)))
}

/// A run's changes made anew: each revision becomes a new object with its original's name and attributes unless the run
/// sets them, on the layer asked for (or its original's); each deletion is dropped, so the originals stay as they were; and, with a layer asked for, what the
/// run makes besides goes on it too. Notes that name a change by its place follow it, and one naming a dropped deletion
/// lets it go. A note says what was done; a run that changed nothing in place says so, made as it would have been.
pub(crate) fn make_anew(project: &Project, result: &ResultAs, changes: Vec<Change>, notes: &mut Vec<Note>) -> Result<Vec<Change>, CommandError> {
    let mut moved: HashMap<usize, usize> = HashMap::new();
    let (mut made, mut copies, mut left): (Vec<Change>, usize, Vec<ObjectId>) = (Vec::new(), 0, Vec::new());
    for (k, change) in changes.into_iter().enumerate() {
        let change = match change {
            Change::Revise { id, name, layer, attributes, body, .. } => {
                let original = project.object(id).ok_or_else(|| CommandError::Invalid(format!("object {id} isn't in the project")))?;
                let original_layer = project.layer(original.layer).map(|l| l.name.clone()).unwrap_or_default();
                copies += 1;
                Change::Create {
                    // Its original's name, unless the run names it (a rename).
                    name: name.unwrap_or_else(|| original.name.clone()),
                    layer: result.layer.clone().or(layer).unwrap_or(original_layer),
                    attributes: attributes.unwrap_or_else(|| original.attributes.clone()),
                    body: body.unwrap_or_else(|| original.body.clone()),
                }
            }
            Change::Delete { id, .. } => {
                left.push(id);
                continue;
            }
            Change::Create { name, layer, attributes, body } => Change::Create { name, layer: result.layer.clone().unwrap_or(layer), attributes, body },
            other => other,
        };
        moved.insert(k, made.len());
        made.push(change);
    }
    for note in notes.iter_mut() {
        if let Some(k) = note.made {
            note.made = moved.get(&k).copied();
        }
    }
    let on = result.layer.as_deref().map_or("each on its original's layer".to_string(), |l| format!("on '{l}'"));
    notes.push(Note::info(match (copies, left.len()) {
        (0, _) => "nothing was changed in place, so as new made no difference".to_string(),
        (n, 0) => format!("made as new, {on}: {n} {}, the original{} left as {}", if n == 1 { "copy" } else { "copies" }, if n == 1 { "" } else { "s" }, if n == 1 { "it was" } else { "they were" }),
        (n, d) => format!("made as new, {on}: {n} {}, the originals left as they were and {d} kept that a change in place would have deleted", if n == 1 { "copy" } else { "copies" }),
    }));
    Ok(made)
}
