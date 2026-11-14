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
    RefusedCancelled = "refused.cancelled", "Cancelled by whoever started it, with nothing applied";
    RefusedChanged = "refused.changed", "The project changed while the run read it, so nothing was applied";

    WallNoHeight = "wall.no_height", "A wall or berm whose target is on or past its line, so it has no height there";
    WallNoTarget = "wall.no_target", "A wall with no target under it, so the gap was bridged";
    BermNoSurface = "berm.no_surface", "Berm vertices with nothing under them, holding their crest's height";
    BermPinched = "berm.pinched", "A berm pinched into pieces, and the small ones left out";
    BermUnderMinBench = "berm.under_min_bench", "No berm where the bench below is under the minimum, as the rule says";
    ToeNoTarget = "toe.no_target", "Next-toe vertices with no target under them, keeping their elevations";
    ToeOffGrid = "toe.off_grid", "Toe vertices off their grid, carried on at their grade";
    FaceCrossing = "face.crossing", "A face or wall whose triangles cross each other";
    FaceNotOneSheet = "face.not_one_sheet", "A face with edges besides its own: a hole, a crack or a piece apart";
    FaceNotBuilt = "face.not_built", "A strip the design couldn't build, so nothing is built on it";
    FaceGapFilled = "face.gap_filled", "A gap the faces of a strip closed all round, filled";
    RampRoad = "ramp.road", "How far a road got: its steps, length and where it starts";
    RampStopped = "ramp.stopped", "A road that stopped before its last wall, and why";
    RampOffHold = "ramp.off_hold", "A road held at its end that comes out away from where it's held";
    RampSteepFlat = "ramp.steep_flat", "A flat before a turn that follows a floor steeper than the road";
    RampNoRoad = "ramp.no_road", "A ramp that made no road";
    RampNoWall = "ramp.no_wall", "A ramp no wall row climbs yet, so it made no road";
    RampNothingToCarryOn = "ramp.nothing_to_carry_on", "A ramp set to carry on from the line, with no road on the line";
    RampStringGone = "ramp.string_gone", "A ramp whose string has been deleted";
    RampNoCrossing = "ramp.no_crossing", "A ramp whose string doesn't cross a line, so it has no road there";
    RampCrossesAgain = "ramp.crosses_again", "A ramp string crossing a line more than once: the road goes where it first does";
    RampTopOffLine = "ramp.top_off_line", "A road's top away from the line it carries on from";
    RampStartsAfresh = "ramp.starts_afresh", "A road carried on that isn't on the first wall, starting afresh";
    RampEnds = "ramp.ends", "A road ended on a line, as asked";
    JoinOnePad = "join.one_pad", "Switchbacks whose boxes are made one pad";
    JoinNothing = "join.nothing", "Join set on a berm with nothing to join";
    LineEdited = "line.edited", "A line redrawn by hand, and what stands on it";
    LineBoxMissing = "line.box_missing", "A line redrawn before a ramp turned on it, so it has no box for the turn";
    BuildCrestGone = "build.crest_gone", "The crest a step was built from has been deleted";
    BuildEditedRebuilt = "build.edited_rebuilt", "A line edited by hand, built again: undo brings the edit back";
    BuildLineMoved = "build.line_moved", "A berm's line not where the berm puts it now, so the wall stands on it as it is";
    SliceEmpty = "slice.empty", "A strip with nothing of the solid in it";
    SliceToeAboveFloor = "slice.toe_above_floor", "A face whose toe doesn't reach below the solid's floor, found before a slice";
    SliceTopBelowTop = "slice.top_below_top", "A face whose top doesn't stand above the solid's top, found before a slice";
    SliceEndShort = "slice.end_short", "A face that stops short of the solid at an end, found before a slice";
    SliceFacesCross = "slice.faces_cross", "A face crossing faces before it inside the solid";
    SolidOutlineOpen = "solid.outline_open", "An open outline taken as closed";
    SolidNotCovered = "solid.not_covered", "A surface that doesn't cover the outline in plan, with no solid there";
    SolidInPieces = "solid.in_pieces", "Solids made in several pieces, each kept as one solid";
    SolidCavities = "solid.cavities", "Solids made holding cavities, their volume taken out";
    SolidNotASolid = "solid.not_a_solid", "Meshes that could not be solids, kept as surfaces flagged not_a_solid";
    MeshCrossing = "mesh.crossing", "A surface whose triangles cross each other, kept and flagged: cuts refuse it";
    ImportRefused = "import.refused", "Files or meshes refused, nothing made of them";
    ImportVolumeDiffers = "import.volume_differs", "Solids whose volume differs from the source's own by more than a millionth";
    ImportSkipped = "import.skipped", "Grids in a seam left out of the import";
    CutNothingOnSide = "cut.nothing_on_side", "A solid lying wholly on one side of what cuts it";
    CutNotCovered = "cut.not_covered", "Solids the cutting surface doesn't cover in plan";
    CutUncut = "cut.uncut", "Solids left uncut, nothing saved for them";
    ExportLeftOut = "export.left_out", "Solids left out of an export, as their files don't read back the same";
    SeamLeftOut = "seam.left_out", "A seam left out of a suggested definition";
    SeamFloorAboveRoof = "seam.floor_above_roof", "A horizon whose floor lies above its roof";
    SeamCrossing = "seam.crossing", "A working horizon whose roof rises above the floor of the one above";
    SeamOutsideParent = "seam.outside_parent", "A daughter seam reaching outside its parent";
    SeamUnpaired = "seam.unpaired", "A horizon whose roof and floor aren't live at the same nodes";
    SeamHoles = "seam.holes", "A horizon whose roof or floor holds no height at some nodes: holes to fix in the geology model, never filled";
    SeamNoDensity = "seam.no_density", "A working horizon with no density grid: tonnes will need one";
    LinkChanged = "link.changed", "Grid files whose content differs from when the seam model was built";
    InterrogateNoSeam = "interrogate.no_seam", "Blocks with no seam attribute, left as they were";
    InterrogateUnknownSeam = "interrogate.unknown_seam", "Blocks whose seam attribute isn't a horizon of the model, left as they were";
    InterrogateNoValue = "interrogate.no_value", "A variable with no value over a block, so none written";
    InterrogateOutsideSeam = "interrogate.outside_seam", "A block lying less than the share asked between its seam's roof and floor: its seam may be wrong";
    InterrogateColumnDiffers = "interrogate.column_differs", "A block whose own volume differs from its 2.5D column volume by more than the tolerance";
    InterrogateColumnWholePlan = "interrogate.column_whole_plan", "Blocks whose plan at mid-seam couldn't be worked out, so their column check takes their whole plan";
    CompareBeyond = "compare.beyond", "Blocks whose value differs from another package's by more than its tolerance";
    StringOffSurface = "string.off_surface", "String vertices with no surface under them, keeping their elevations";
    StringNotCrossed = "string.not_crossed", "Strings a numbering line doesn't cross, left as they were";
    StringCrossesAgain = "string.crosses_again", "A string a numbering line crosses more than once, numbered where it first does";
    StringEndJoined = "string.end_joined", "An end of a line joined to another line across a gap within the tolerance";
    StringEndShort = "string.end_short", "A line ending inside a polygon it was meant to split, short of the line it should meet";
    StringClosesNothing = "string.closes_nothing", "A line that closes no area, left out of the polygons";
    StringIsland = "string.island", "Lines inside a polygon that don't touch its sides, so they don't split it";
    SolidOutsidePolygons = "solid.outside_polygons", "Part of a solid outside every block polygon, found before a cut by polygons, which it holds";
    BlockNameRepeated = "block.name_repeated", "Blocks given the same name, which should be unique";
    BlockVolumeDiffers = "block.volume_differs", "A solid whose blocks and remainder don't add back to its volume";
    SolidOffGrids = "solid.off_grids", "Part of a solid past the edge of the seam model's grids, found before a cut by seams, which it holds";
    SolidOverGridHoles = "solid.over_grid_holes", "Part of a solid over a hole in the seam model's grids, found before a cut by seams, which it holds";
    SeamThinLeftOut = "seam.thin_left_out", "Volume left out of a seam cut where a layer is thinner than the minimum";
    SeamFlapsLeftOut = "seam.flaps_left_out", "Volume taken off a seam cut's pieces as flaps thinner than the flap thickness";
    SeamCutDiffers = "seam.cut_differs", "A solid whose pieces from the seam cut don't add back to its volume";
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
        let placed = Note::warning(Code::WallNoHeight, "no height")
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
