//! A face design: what the Face design panel edits and `face.project` runs. Which lines, which profile
//! each one takes, and the steps of each profile, one table going one way from the toe. It's one saved
//! object, so one edit is one revision and undoes in one go. See `docs/decisions/0024-face-design.md`.

use serde::{Deserialize, Serialize};

use crate::{LayerId, ObjectId};

/// Past this a conditioning tolerance stops tidying a line and starts cutting its corners off (m).
pub const MOST_TOLERANCE: f64 = 2.0;

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceDesign {
    /// Everything a run makes goes on this layer.
    pub layer: String,
    pub lines: Vec<FaceLine>,
    pub profiles: Vec<FaceProfile>,
    #[serde(default)]
    pub rules: Vec<WallRule>,
}

/// What a line runs on: one string, or every string on a layer as it stands at each run. Nothing rebuilds
/// by itself, so a string added to the layer comes in when the design is next run. A layer is held by its
/// id, so renaming it doesn't lose it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineOn {
    String(ObjectId),
    Layer(LayerId),
}

/// What to run, and how. A string can be in twice with different profiles, to go both ways from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceLine {
    #[serde(flatten)]
    pub on: LineOn,
    /// A profile's name.
    pub profile: String,
    pub side: WallSide,
    /// How far the toe is extended past its start and end (m).
    #[serde(default)]
    pub run_on: [f64; 2],
    /// Roads cut into this line's walls. On a line that names a layer, each goes on the string nearest its
    /// start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ramps: Vec<Ramp>,
}

/// A road up a wall. It's cut back into the wall as the wall is projected, so running the design again
/// rebuilds it. See `docs/decisions/0028-ramps.md`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ramp {
    /// Where it leaves the wall: the nearest point in plan on the line its first step starts from (E, N).
    pub start: [f64; 2],
    pub heading: RampHeading,
    /// The step it starts from and the step it finishes on, numbered as the table numbers them.
    pub from_step: usize,
    pub to_step: usize,
    /// Square to the road (m).
    pub width: f64,
    /// Rise over plan run on the inside edge, the pit side (%).
    pub grade: f64,
}

/// The way a road runs along its string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RampHeading {
    /// The way the string runs.
    Along,
    /// Back against it.
    Back,
}

/// The side a wall rises on, looking along the string. It doesn't change with direction: going down from
/// the same string on the same side carries on the same plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallSide {
    Left,
    Right,
}

/// The way a profile's steps go from the toe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaceDirection {
    Up,
    Down,
}

/// One table of steps, for one type of string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceProfile {
    pub name: String,
    pub direction: FaceDirection,
    /// The surface or grid a toe registers to.
    pub register: ObjectId,
    /// How far a toe may move in plan when it's conditioned (m); none leaves it as drawn.
    #[serde(default)]
    pub condition: Option<f64>,
    pub steps: Vec<ProfileStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProfileStep {
    /// A wall to a target at a batter (degrees). `condition` is how far the crest may move in plan when
    /// it's cleaned for the next step (m); none leaves it as it landed.
    Project {
        to: StepTarget,
        batter: StepValue,
        #[serde(default)]
        condition: Option<f64>,
    },
    /// A berm of a width (m). Its line registers on `on`, or on what the row above reached if that's left
    /// out. With `register` off it keeps its crest's heights instead. `condition` is how far the line may
    /// move in plan when it's cleaned after moving out (m).
    Berm {
        /// No berm where the wall below it is lower than this (m): where grids sit on each other there's
        /// nothing to stand it on, and a berm for every grid would run out flat for no reason.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_bench: Option<f64>,
        #[serde(default)]
        on: Option<StepTarget>,
        width: StepValue,
        #[serde(default = "yes")]
        register: bool,
        #[serde(default)]
        condition: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StepTarget {
    Surface { id: ObjectId },
    Rl { rl: f64 },
}

/// A step's batter or width: a number typed on the row, or the name of a rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StepValue {
    Number(f64),
    Rule(String),
}

/// A named rule, which any step can use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallRule {
    pub name: String,
    pub value: RuleValue,
}

/// How a rule gives its value: one value per edge of whatever line a step works from. Blending between
/// values happens in the wall and only ever flattens a slope or widens a berm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleValue {
    Fixed { value: f64 },
    /// One value for each segment of a line's string, in order.
    FixedList { values: Vec<f64> },
    /// One value inside a closed string in plan and another outside.
    InsidePolygon { polygon: ObjectId, inside: f64, outside: f64 },
    /// One value inside a solid and another outside.
    InsideSolid { solid: ObjectId, inside: f64, outside: f64 },
    /// A value by the way the wall faces.
    SegmentAzimuth { sectors: Vec<AzimuthSector>, otherwise: f64 },
    /// A value by the surface a row finishes on: for a berm, the one the wall below it reached.
    ByHorizon { values: Vec<HorizonValue>, otherwise: f64 },
}

/// A sector of azimuths, clockwise from `from` to `to` in degrees from grid north, and its value. One
/// from a number back to the same number holds every azimuth.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AzimuthSector {
    pub from: f64,
    pub to: f64,
    pub value: f64,
}

/// A surface and the value for a row that finishes on it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HorizonValue {
    pub surface: ObjectId,
    pub value: f64,
}

impl RuleValue {
    /// Every number the rule can give, to check against what a batter or a berm can be.
    pub fn numbers(&self) -> Vec<f64> {
        match self {
            RuleValue::Fixed { value } => vec![*value],
            RuleValue::FixedList { values } => values.clone(),
            RuleValue::InsidePolygon { inside, outside, .. } | RuleValue::InsideSolid { inside, outside, .. } => vec![*inside, *outside],
            RuleValue::SegmentAzimuth { sectors, otherwise } => sectors.iter().map(|s| s.value).chain([*otherwise]).collect(),
            RuleValue::ByHorizon { values, otherwise } => values.iter().map(|v| v.value).chain([*otherwise]).collect(),
        }
    }
}

impl FaceDesign {
    pub fn rule(&self, name: &str) -> Option<&WallRule> {
        self.rules.iter().find(|r| r.name == name)
    }

    pub fn profile(&self, name: &str) -> Option<&FaceProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// The number a step's batter or width comes to.
    /// A step's value if it's one number all along: typed on the row, or a fixed rule.
    pub fn value_of(&self, given: &StepValue) -> Option<f64> {
        match given {
            StepValue::Number(value) => Some(*value),
            StepValue::Rule(name) => match self.rule(name)?.value {
                RuleValue::Fixed { value } => Some(value),
                _ => None,
            },
        }
    }

    /// Every number a step's value can come to, or nothing if it names no rule.
    fn numbers_of(&self, given: &StepValue) -> Option<Vec<f64>> {
        match given {
            StepValue::Number(value) => Some(vec![*value]),
            StepValue::Rule(name) => self.rule(name).map(|rule| rule.value.numbers()),
        }
    }

    /// What's wrong with the design, in the words the panel shows. Empty when it can run, as far as can
    /// be told without the strings and surfaces it names.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.layer.trim().is_empty() {
            problems.push("Name a layer for what a run makes".to_string());
        }
        if self.lines.is_empty() {
            problems.push("Add at least one line".to_string());
        }
        let twice = |names: Vec<&str>, what: &str, problems: &mut Vec<String>| {
            for (k, name) in names.iter().enumerate() {
                if name.trim().is_empty() {
                    problems.push(format!("A {what} has no name"));
                } else if names[..k].contains(name) {
                    problems.push(format!("Two {what}s are called {name}"));
                }
            }
        };
        twice(self.profiles.iter().map(|p| p.name.as_str()).collect(), "profile", &mut problems);
        twice(self.rules.iter().map(|r| r.name.as_str()).collect(), "rule", &mut problems);
        for rule in &self.rules {
            if matches!(&rule.value, RuleValue::FixedList { values } if values.is_empty()) {
                problems.push(format!("{} has no values", rule.name));
            }
        }
        for (k, line) in self.lines.iter().enumerate() {
            if self.profile(&line.profile).is_none() {
                problems.push(format!("A line uses a profile that isn't there: {}", line.profile));
            }
            if !line.run_on.iter().all(|d| d.is_finite() && *d >= 0.0) {
                problems.push("A run-on must be zero or more".to_string());
            }
            if self.lines[..k].iter().any(|l| l.on == line.on && l.profile == line.profile) {
                let what = if matches!(line.on, LineOn::Layer(_)) { "layer" } else { "string" };
                problems.push(format!("A {what} is in twice with the profile {}", line.profile));
            }
            for (r, ramp) in line.ramps.iter().enumerate() {
                self.check_ramp(&format!("Ramp {} on {}", r + 1, line.profile), ramp, line, &mut problems);
            }
        }
        for profile in &self.profiles {
            if profile.steps.is_empty() {
                problems.push(format!("{} has no steps", profile.name));
            }
            check_tolerance(&format!("{}'s toe", profile.name), profile.condition, &mut problems);
            for (k, step) in profile.steps.iter().enumerate() {
                self.check_step(&format!("{} step {}", profile.name, k + 1), step, &mut problems);
            }
        }
        let steps = || self.profiles.iter().flat_map(|p| &p.steps);
        for rule in &self.rules {
            let named = |value: &StepValue| matches!(value, StepValue::Rule(name) if *name == rule.name);
            let as_batter = steps().any(|s| matches!(s, ProfileStep::Project { batter, .. } if named(batter)));
            let as_berm = steps().any(|s| matches!(s, ProfileStep::Berm { width, .. } if named(width)));
            if as_batter && as_berm {
                problems.push(format!("{} is used as both a batter and a berm: a batter is in degrees and a berm in metres", rule.name));
            }
        }
        problems.dedup();
        problems
    }

    fn check_ramp(&self, row: &str, ramp: &Ramp, line: &FaceLine, problems: &mut Vec<String>) {
        if !ramp.start.iter().all(|c| c.is_finite()) {
            problems.push(format!("{row} needs a start point"));
        }
        if !(ramp.width.is_finite() && ramp.width > 0.0) {
            problems.push(format!("{row}: a road's width must be more than zero"));
        }
        if !(ramp.grade.is_finite() && ramp.grade > 0.0 && ramp.grade < 100.0) {
            problems.push(format!("{row}: a road's grade must be between 0 and 100 %"));
        }
        let Some(profile) = self.profile(&line.profile) else { return };
        if profile.direction == FaceDirection::Down {
            problems.push(format!("{row}: a ramp only goes on a profile going up for now"));
        }
        for number in [ramp.from_step, ramp.to_step] {
            if !matches!(number.checked_sub(1).and_then(|k| profile.steps.get(k)), Some(ProfileStep::Project { .. })) {
                problems.push(format!("{row}: step {number} isn't a wall of {}", profile.name));
            }
        }
        if ramp.from_step != ramp.to_step {
            problems.push(format!("{row}: a ramp climbs one wall for now, so it finishes on the step it starts from"));
        }
    }

    fn check_step(&self, row: &str, step: &ProfileStep, problems: &mut Vec<String>) {
        let (given, condition, is_batter) = match step {
            ProfileStep::Project { batter, condition, .. } => (batter, condition, true),
            ProfileStep::Berm { width, condition, .. } => (width, condition, false),
        };
        check_tolerance(row, *condition, problems);
        if let ProfileStep::Berm { min_bench: Some(height), .. } = step
            && !(height.is_finite() && *height >= 0.0)
        {
            problems.push(format!("{row}: a minimum bench must be zero or more"));
        }
        let Some(numbers) = self.numbers_of(given) else {
            match given {
                StepValue::Rule(name) if !name.trim().is_empty() => problems.push(format!("{row} uses a rule that isn't there: {name}")),
                _ => problems.push(format!("{row} needs {}", if is_batter { "a batter" } else { "a width" })),
            }
            return;
        };
        if is_batter && !numbers.iter().all(|value| *value > 0.0 && *value < 90.0) {
            problems.push(format!("{row}: a batter must be between 0 and 90 degrees"));
        }
        if !is_batter && !numbers.iter().all(|value| value.is_finite() && *value >= 0.0) {
            problems.push(format!("{row}: a berm must be zero or more"));
        }
    }
}

fn check_tolerance(row: &str, condition: Option<f64>, problems: &mut Vec<String>) {
    if let Some(tolerance) = condition
        && !(tolerance.is_finite() && (0.0..=MOST_TOLERANCE).contains(&tolerance))
    {
        problems.push(format!("{row}: a conditioning tolerance must be from 0 to {MOST_TOLERANCE} m"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design() -> FaceDesign {
        let id = ObjectId::new();
        FaceDesign {
            layer: "faces".into(),
            lines: vec![FaceLine { on: LineOn::String(id), profile: "Highwall".into(), side: WallSide::Right, run_on: [100.0, 100.0], ramps: Vec::new() }],
            profiles: vec![FaceProfile {
                name: "Highwall".into(),
                direction: FaceDirection::Up,
                register: id,
                condition: None,
                steps: vec![
                    ProfileStep::Project { to: StepTarget::Surface { id }, batter: StepValue::Rule("HW batter".into()), condition: Some(0.1) },
                    ProfileStep::Berm { on: None, width: StepValue::Number(10.0), register: true, condition: Some(0.1), min_bench: None },
                    ProfileStep::Project { to: StepTarget::Rl { rl: 500.0 }, batter: StepValue::Rule("HW batter".into()), condition: None },
                ],
            }],
            rules: vec![WallRule { name: "HW batter".into(), value: RuleValue::Fixed { value: 70.0 } }],
        }
    }

    #[test]
    fn a_design_reads_back_from_json_as_it_was() {
        let d = design();
        assert_eq!(serde_json::from_str::<FaceDesign>(&serde_json::to_string(&d).unwrap()).unwrap(), d);
        assert!(d.problems().is_empty());
    }

    #[test]
    fn a_line_names_a_string_or_a_layer_and_a_string_saved_before_layers_still_reads() {
        let mut d = design();
        let text = serde_json::to_string(&d.lines[0]).unwrap();
        assert!(text.starts_with(r#"{"string":"#), "the field a saved design has: {text}");
        d.lines.push(FaceLine { on: LineOn::Layer(LayerId::new()), ..d.lines[0].clone() });
        assert_eq!(serde_json::from_str::<FaceDesign>(&serde_json::to_string(&d).unwrap()).unwrap(), d);
        assert!(d.problems().is_empty(), "{:?}", d.problems());
        d.lines.push(d.lines[1].clone());
        assert!(d.problems().iter().any(|p| p.contains("A layer is in twice with the profile Highwall")), "{:?}", d.problems());
    }

    /// Exactly what the Face design panel sent, nulls and whole numbers included.
    #[test]
    fn a_design_as_the_panel_saves_it_reads() {
        let saved = r#"{
            "layer": "faces",
            "lines": [
                { "string": "55555555555555555555555555555555", "profile": "Highwall", "side": "right", "run_on": [100, 100] },
                { "string": "55555555555555555555555555555555", "profile": "Highwall copy", "side": "right", "run_on": [100, 100] }
            ],
            "profiles": [{
                "name": "Highwall", "direction": "up", "register": "11111111111111111111111111111111", "condition": null,
                "steps": [
                    { "step": "project", "to": { "kind": "surface", "id": "22222222222222222222222222222222" }, "batter": "Highwall batter", "condition": 0.1 },
                    { "step": "berm", "on": null, "width": 10, "register": false, "condition": 0.1 },
                    { "step": "berm", "on": { "kind": "rl", "rl": 300 }, "width": 10, "register": true, "condition": null },
                    { "step": "project", "to": { "kind": "rl", "rl": 500 }, "batter": 45.5, "condition": 0.1 }
                ]
            }, {
                "name": "Highwall copy", "direction": "down", "register": "11111111111111111111111111111111", "condition": 0.1,
                "steps": [{ "step": "project", "to": { "kind": "rl", "rl": -500 }, "batter": "Highwall batter", "condition": null }]
            }],
            "rules": [{ "name": "Highwall batter", "value": { "type": "fixed", "value": 72 } }]
        }"#;
        let design: FaceDesign = serde_json::from_str(saved).unwrap();
        assert!(design.problems().is_empty(), "{:?}", design.problems());
        assert_eq!(design.profiles[1].direction, FaceDirection::Down);
        assert_eq!(design.profiles[0].steps[1], ProfileStep::Berm { on: None, width: StepValue::Number(10.0), register: false, condition: Some(0.1), min_bench: None });
        assert_eq!(design.value_of(&StepValue::Rule("Highwall batter".into())), Some(72.0));
    }

    #[test]
    fn a_step_takes_a_number_or_a_rule() {
        let d = design();
        assert_eq!(d.value_of(&StepValue::Number(45.0)), Some(45.0));
        assert_eq!(d.value_of(&StepValue::Rule("HW batter".into())), Some(70.0));
        assert_eq!(d.value_of(&StepValue::Rule("Endwall batter".into())), None);
        let row: ProfileStep = serde_json::from_str(r#"{ "step": "berm", "width": 30 }"#).unwrap();
        assert_eq!(row, ProfileStep::Berm { on: None, width: StepValue::Number(30.0), register: true, condition: None, min_bench: None });
    }

    #[test]
    fn what_is_wrong_with_a_design_is_said_in_plain_words() {
        let mut d = design();
        d.lines[0].profile = "Endwall".into();
        assert!(d.problems().iter().any(|p| p.contains("profile that isn't there: Endwall")));
        let mut d = design();
        d.rules[0].value = RuleValue::Fixed { value: 95.0 };
        assert!(d.problems().iter().any(|p| p.contains("Highwall step 1: a batter must be")));
        let mut d = design();
        d.rules.remove(0);
        assert!(d.problems().iter().any(|p| p.contains("Highwall step 1 uses a rule that isn't there: HW batter")));
        let mut d = design();
        d.profiles[0].steps[1] = ProfileStep::Berm { on: None, width: StepValue::Rule(String::new()), register: true, condition: None, min_bench: None };
        assert!(d.problems().iter().any(|p| p.contains("Highwall step 2 needs a width")));
        let mut d = design();
        d.profiles[0].steps[1] = ProfileStep::Berm { on: None, width: StepValue::Rule("HW batter".into()), register: true, condition: None, min_bench: None };
        assert!(d.problems().iter().any(|p| p.contains("HW batter is used as both")));
        let mut d = design();
        d.profiles.push(d.profiles[0].clone());
        assert!(d.problems().iter().any(|p| p.contains("Two profiles are called Highwall")));
        let mut d = design();
        d.profiles[0].steps[0] = ProfileStep::Project { to: StepTarget::Rl { rl: 0.0 }, batter: StepValue::Number(70.0), condition: Some(30.0) };
        assert!(d.problems().iter().any(|p| p.contains("Highwall step 1: a conditioning tolerance must be")));
        let mut d = design();
        d.profiles[0].condition = Some(5.0);
        assert!(d.problems().iter().any(|p| p.contains("Highwall's toe: a conditioning tolerance must be")));
    }

    fn ramp() -> Ramp {
        Ramp { start: [650_000.0, 7_450_000.0], heading: RampHeading::Along, from_step: 1, to_step: 1, width: 30.0, grade: 10.0 }
    }

    #[test]
    fn a_ramp_saves_with_its_line_and_a_design_without_one_saves_as_it_did() {
        let mut d = design();
        assert!(!serde_json::to_string(&d).unwrap().contains("ramps"));
        d.lines[0].ramps.push(ramp());
        assert_eq!(serde_json::from_str::<FaceDesign>(&serde_json::to_string(&d).unwrap()).unwrap(), d);
        assert!(d.problems().is_empty(), "{:?}", d.problems());
    }

    #[test]
    fn what_is_wrong_with_a_ramp_is_said_in_plain_words() {
        let with = |change: fn(&mut Ramp)| {
            let mut d = design();
            let mut r = ramp();
            change(&mut r);
            d.lines[0].ramps.push(r);
            d.problems()
        };
        assert!(with(|r| r.width = 0.0).iter().any(|p| p.contains("Ramp 1 on Highwall: a road's width")));
        assert!(with(|r| r.grade = 0.0).iter().any(|p| p.contains("a road's grade")));
        assert!(with(|r| r.start = [f64::NAN, 0.0]).iter().any(|p| p.contains("needs a start point")));
        assert!(with(|r| r.from_step = 2).iter().any(|p| p.contains("step 2 isn't a wall")), "a berm row is no wall to climb");
        assert!(with(|r| r.to_step = 9).iter().any(|p| p.contains("step 9 isn't a wall")));
        assert!(with(|r| r.from_step = 0).iter().any(|p| p.contains("step 0 isn't a wall")));
        assert!(with(|r| r.to_step = 3).iter().any(|p| p.contains("climbs one wall for now")));
        let mut d = design();
        d.profiles[0].direction = FaceDirection::Down;
        d.lines[0].ramps.push(ramp());
        assert!(d.problems().iter().any(|p| p.contains("only goes on a profile going up")));
    }

    #[test]
    fn a_string_goes_both_ways_with_a_profile_for_each_but_not_twice_with_one() {
        let mut d = design();
        let mut down = d.profiles[0].clone();
        down.name = "Highwall down".into();
        down.direction = FaceDirection::Down;
        d.profiles.push(down);
        d.lines.push(FaceLine { profile: "Highwall down".into(), ..d.lines[0].clone() });
        assert!(d.problems().is_empty(), "{:?}", d.problems());
        d.lines.push(d.lines[0].clone());
        assert!(d.problems().iter().any(|p| p.contains("A string is in twice with the profile Highwall")));
    }
}
