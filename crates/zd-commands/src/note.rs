//! What a run reports, one note a line (decision 0059): how much it matters, a fixed code for what happened, the
//! sentence a planner reads, and where it can be said, the object it's about, a place and a value.
//!
//! The codes are one list, here. A command names a code from it, so the app, a test or a script can rely on the
//! name, never on the words of the sentence.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zd_core::ObjectId;

/// How much a note matters. A refused run is the run's own state, not a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// What the run did: counts, timings, what was cleaned and why.
    Info,
    /// Look at this: a gap bridged, a stretch with no height, a file changed since.
    Warning,
    /// It stops later use until it's fixed at the source, as a seam crossing stops solids and reserves.
    Blocking,
}

/// Defines [`Code`] and its list in one place: each code's name as saved and its meaning.
macro_rules! codes {
    ($( $(#[$doc:meta])* $variant:ident = $name:literal, $meaning:literal; )*) => {
        /// What happened, by a fixed name: a family and what happened in it, joined by a dot.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Code {
            $( $(#[$doc])* $variant, )*
        }

        impl Code {
            /// Every code, in the order they're listed.
            pub const ALL: &'static [Code] = &[$(Code::$variant),*];

            /// The name a code is saved and sent as, such as `refused.invalid`.
            pub fn name(self) -> &'static str {
                match self { $(Code::$variant => $name,)* }
            }

            /// What it means, in a line.
            pub fn meaning(self) -> &'static str {
                match self { $(Code::$variant => $meaning,)* }
            }

            /// The code of a name, if this build knows it.
            pub fn from_name(name: &str) -> Option<Code> {
                match name { $($name => Some(Code::$variant),)* _ => None }
            }
        }
    };
}

codes! {
    RefusedUnknownCommand = "refused.unknown_command", "No command has that name";
    RefusedBadParams = "refused.bad_params", "The parameters don't match the command's";
    RefusedMissingObject = "refused.missing_object", "An object the run names isn't in the project";
    RefusedWrongKind = "refused.wrong_kind", "An object the run names is of another kind";
    RefusedInvalid = "refused.invalid", "Parameters that read but make no sense";
    RefusedNotWatertight = "refused.not_watertight", "A mesh that has to be closed isn't";
    RefusedGeometry = "refused.geometry", "The geometry couldn't be made, or the engine stopped on an error";
    RefusedIo = "refused.io", "A file couldn't be read or written";
    RefusedProject = "refused.project", "The project refused the changes, as for a stale input or a layer that exists";
    RefusedStore = "refused.store", "The project file couldn't be opened or saved";
}

impl Serialize for Code {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Code::from_name(&name).ok_or_else(|| serde::de::Error::custom(format!("no code named '{name}'")))
    }
}

/// A code read leniently: a run log from a later build may name a code this one doesn't know, and the note's text
/// still says what happened, so the code is dropped rather than the log refused.
fn lenient_code<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Code>, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.and_then(|name| Code::from_name(&name)))
}

/// Where a note is, in mine grid coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Place {
    /// A point: easting, northing, RL.
    Point { at: [f64; 3] },
    /// A point in plan only, where there's no one RL (a seam model's node).
    Plan { at: [f64; 2] },
    /// A stretch along a string or crest, from one point to another.
    Stretch { from: [f64; 3], to: [f64; 3] },
}

/// A number with its unit, such as 42 m or 12 %.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub value: f64,
    pub unit: String,
}

/// One line of what a run reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub severity: Severity,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "lenient_code")]
    pub code: Option<Code>,
    /// The sentence a planner reads.
    pub text: String,
    /// The object it's about: one the run read, or, once the run is applied, one it made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<ObjectId>,
    /// Something the run makes, by its place in the run's changes, until the run is applied and it has an id: the
    /// session then puts the id in `object`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub made: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Measure>,
}

impl Note {
    pub fn new(severity: Severity, code: Option<Code>, text: impl Into<String>) -> Note {
        Note { severity, code, text: text.into(), object: None, made: None, place: None, value: None }
    }

    /// What the run did, with no code: how every message read before codes.
    pub fn info(text: impl Into<String>) -> Note {
        Note::new(Severity::Info, None, text)
    }

    pub fn warning(code: Code, text: impl Into<String>) -> Note {
        Note::new(Severity::Warning, Some(code), text)
    }

    pub fn blocking(code: Code, text: impl Into<String>) -> Note {
        Note::new(Severity::Blocking, Some(code), text)
    }

    /// About an object the run read.
    pub fn on(mut self, object: ObjectId) -> Note {
        self.object = Some(object);
        self
    }

    /// About what the run's `index`th change makes or revises.
    pub fn on_made(mut self, index: usize) -> Note {
        self.made = Some(index);
        self
    }

    pub fn at(mut self, place: Place) -> Note {
        self.place = Some(place);
        self
    }

    pub fn measured(mut self, value: f64, unit: &str) -> Note {
        self.value = Some(Measure { value, unit: unit.into() });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_one_name_and_reads_back() {
        let mut names = std::collections::HashSet::new();
        for &code in Code::ALL {
            let name = code.name();
            assert!(names.insert(name), "{name} twice");
            assert!(name.split_once('.').is_some_and(|(family, what)| !family.is_empty() && !what.is_empty()), "{name}: a family and what happened");
            assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c == '.'), "{name}");
            assert_eq!(Code::from_name(name), Some(code));
            assert!(!code.meaning().is_empty());
            assert_eq!(serde_json::to_value(code).unwrap(), serde_json::json!(name));
        }
    }

    #[test]
    fn a_note_saves_only_what_it_has_and_reads_back() {
        let plain = Note::info("12 strips joined");
        assert_eq!(serde_json::to_string(&plain).unwrap(), r#"{"severity":"info","text":"12 strips joined"}"#);
        let placed = Note::warning(Code::RefusedGeometry, "a hole")
            .at(Place::Stretch { from: [650_000.0, 7_450_000.25, 96.5], to: [650_010.0, 7_450_000.25, 97.0] })
            .measured(10.0, "m")
            .on_made(2);
        let back: Note = serde_json::from_str(&serde_json::to_string(&placed).unwrap()).unwrap();
        assert_eq!(back, placed, "coordinates exactly, f64");
    }

    #[test]
    fn a_code_a_later_build_added_is_dropped_not_refused() {
        let note: Note = serde_json::from_str(r#"{"severity":"warning","code":"wall.something_new","text":"what it said"}"#).unwrap();
        assert_eq!((note.code, note.text.as_str()), (None, "what it said"));
    }
}
