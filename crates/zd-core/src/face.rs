//! A face design: what the Face design panel edits and `face.project` runs. Which lines, which profile
//! each one takes, the steps of each profile, and the named rules those steps use. It's one saved object,
//! so one edit is one revision and undoes in one go. See `docs/decisions/0024-face-design.md`.

use serde::{Deserialize, Serialize};

use crate::ObjectId;

fn default_tolerance() -> f64 {
    0.1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceDesign {
    /// Everything a run makes goes on this layer.
    pub layer: String,
    /// How far a next toe's plan shape may leave its crest (m).
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    pub lines: Vec<FaceLine>,
    pub profiles: Vec<FaceProfile>,
    pub rules: Vec<WallRule>,
}

/// One string to run, and how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceLine {
    pub string: ObjectId,
    /// A profile's name.
    pub profile: String,
    pub side: WallSide,
    /// How far the toe is extended past its start and end (m).
    #[serde(default)]
    pub run_on: [f64; 2],
}

/// The side a wall rises on, looking along the string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceProfile {
    pub name: String,
    /// The surface or grid a toe registers to.
    pub register: ObjectId,
    pub up: Vec<ProfileStep>,
    #[serde(default)]
    pub down: Vec<ProfileStep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProfileStep {
    /// A wall to a target at the batter a rule gives (degrees).
    Project { to: StepTarget, rule: String },
    /// A berm of the width a rule gives (m).
    Berm { rule: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StepTarget {
    Surface { id: ObjectId },
    Rl { rl: f64 },
}

/// A named rule, which any step can use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WallRule {
    pub name: String,
    pub value: RuleValue,
}

/// How a rule gives its value. The other kinds (azimuth, inside polygon, by horizon) come next, and each
/// gives one value per edge of a string.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleValue {
    Fixed { value: f64 },
}

impl FaceDesign {
    pub fn rule(&self, name: &str) -> Option<&WallRule> {
        self.rules.iter().find(|r| r.name == name)
    }

    pub fn profile(&self, name: &str) -> Option<&FaceProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// What's wrong with the design, in the words the panel shows. Empty when it can run, as far as can
    /// be told without the strings and surfaces it names.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.layer.trim().is_empty() {
            problems.push("Name a layer for what a run makes".to_string());
        }
        if !(self.tolerance.is_finite() && self.tolerance >= 0.0) {
            problems.push("The tolerance must be zero or more".to_string());
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
        for line in &self.lines {
            if self.profile(&line.profile).is_none() {
                problems.push(format!("A line uses a profile that isn't there: {}", line.profile));
            }
            if !line.run_on.iter().all(|d| d.is_finite() && *d >= 0.0) {
                problems.push("A run-on must be zero or more".to_string());
            }
        }
        for profile in &self.profiles {
            if profile.up.is_empty() && profile.down.is_empty() {
                problems.push(format!("{} has no steps", profile.name));
            }
            for step in profile.up.iter().chain(&profile.down) {
                self.check_step(profile, step, &mut problems);
            }
        }
        let steps = || self.profiles.iter().flat_map(|p| p.up.iter().chain(&p.down));
        for rule in &self.rules {
            let as_batter = steps().any(|s| matches!(s, ProfileStep::Project { rule: r, .. } if *r == rule.name));
            let as_berm = steps().any(|s| matches!(s, ProfileStep::Berm { rule: r } if *r == rule.name));
            if as_batter && as_berm {
                problems.push(format!("{} is used as both a batter and a berm: a batter is in degrees and a berm in metres", rule.name));
            }
        }
        problems.dedup();
        problems
    }

    fn check_step(&self, profile: &FaceProfile, step: &ProfileStep, problems: &mut Vec<String>) {
        let (name, batter) = match step {
            ProfileStep::Project { rule, .. } => (rule, true),
            ProfileStep::Berm { rule } => (rule, false),
        };
        let Some(rule) = self.rule(name) else {
            problems.push(format!("{} uses a rule that isn't there: {name}", profile.name));
            return;
        };
        let RuleValue::Fixed { value } = rule.value;
        if batter && !(value > 0.0 && value < 90.0) {
            problems.push(format!("{name} is used as a batter, which must be between 0 and 90 degrees"));
        }
        if !batter && !(value.is_finite() && value >= 0.0) {
            problems.push(format!("{name} is used as a berm, which must be zero or more"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design() -> FaceDesign {
        let id = ObjectId::new();
        FaceDesign {
            layer: "faces".into(),
            tolerance: 0.1,
            lines: vec![FaceLine { string: id, profile: "Highwall".into(), side: WallSide::Right, run_on: [100.0, 100.0] }],
            profiles: vec![FaceProfile {
                name: "Highwall".into(),
                register: id,
                up: vec![ProfileStep::Project { to: StepTarget::Surface { id }, rule: "HW batter".into() }, ProfileStep::Berm { rule: "HW berm".into() }, ProfileStep::Project { to: StepTarget::Rl { rl: 500.0 }, rule: "HW batter".into() }],
                down: vec![ProfileStep::Project { to: StepTarget::Rl { rl: -500.0 }, rule: "HW batter".into() }],
            }],
            rules: vec![WallRule { name: "HW batter".into(), value: RuleValue::Fixed { value: 70.0 } }, WallRule { name: "HW berm".into(), value: RuleValue::Fixed { value: 10.0 } }],
        }
    }

    #[test]
    fn a_design_reads_back_from_json_as_it_was() {
        let d = design();
        assert_eq!(serde_json::from_str::<FaceDesign>(&serde_json::to_string(&d).unwrap()).unwrap(), d);
        assert!(d.problems().is_empty());
    }

    #[test]
    fn what_is_wrong_with_a_design_is_said_in_plain_words() {
        let mut d = design();
        d.lines[0].profile = "Endwall".into();
        assert!(d.problems().iter().any(|p| p.contains("profile that isn't there: Endwall")));
        let mut d = design();
        d.rules[0].value = RuleValue::Fixed { value: 95.0 };
        assert!(d.problems().iter().any(|p| p.contains("HW batter is used as a batter")));
        let mut d = design();
        d.rules.remove(1);
        assert!(d.problems().iter().any(|p| p.contains("rule that isn't there: HW berm")));
        let mut d = design();
        d.profiles[0].up[1] = ProfileStep::Berm { rule: "HW batter".into() };
        assert!(d.problems().iter().any(|p| p.contains("HW batter is used as both")));
        let mut d = design();
        d.profiles.push(d.profiles[0].clone());
        assert!(d.problems().iter().any(|p| p.contains("Two profiles are called Highwall")));
    }
}
