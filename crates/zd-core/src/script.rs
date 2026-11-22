//! A script of the project (the Build Plan's "Tables and scripts plan", decision 9, `docs/decisions/0099-tables-and-scripts.md`):
//! Starlark that a dataflow's formula, a calculated column or another script loads by its name,
//! `load("qualities", "by_tonnes")`, so a site's own functions are written once and used everywhere. What loads it keeps
//! its id, so a run names the revision it read.

use serde::{Deserialize, Serialize};

/// Its Starlark, and a line on what it's for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Script {
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// The parameter tables a script looks up by name, in order, each once (the Build Plan's "Schedule quantities plan",
/// decision 1): `lookup("Rates", "bcm_per_hour", ...)` names Rates. A name worked out as the script runs can't be read
/// off its source, and isn't bound.
pub fn lookups_in(source: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut from = 0;
        while let Some(at) = line[from..].find("lookup").map(|a| a + from) {
            from = at + "lookup".len();
            if line[..at].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                continue;
            }
            let Some(rest) = line[from..].trim_start().strip_prefix('(') else { continue };
            let rest = rest.trim_start();
            let quote = match rest.chars().next() {
                Some(q @ ('"' | '\'')) => q,
                _ => continue,
            };
            if let Some(end) = rest[1..].find(quote) {
                let name = rest[1..1 + end].to_string();
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names
}

/// The names a script's `load` lines ask for, in order, each once: `load("qualities", "by_tonnes")` asks for
/// `qualities`. Comments are passed over.
pub fn loads_in(source: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("").trim_start();
        let Some(rest) = line.strip_prefix("load") else { continue };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('(') else { continue };
        let rest = rest.trim_start();
        let quote = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => q,
            _ => continue,
        };
        if let Some(end) = rest[1..].find(quote) {
            let name = rest[1..1 + end].to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_s_loads_are_read_off_its_source() {
        let source = "load(\"qualities\", \"by_tonnes\")\nload( 'limits', 'clamp_ash')\n# load(\"commented\", \"x\")\nload(\"qualities\", \"other\")\ndef formula(unit):\n    pass\n";
        assert_eq!(loads_in(source), ["qualities", "limits"]);
    }
}
