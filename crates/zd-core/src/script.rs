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
/// decisions 1 and 2): `lookup("Rates", "bcm_per_hour", ...)` names Rates, `interpolate("Yield curve", ...)` names Yield
/// curve. A name worked out as the script runs can't be read off its source, and isn't bound.
pub fn lookups_in(source: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (name, _) in calls_in(source, "lookup").into_iter().chain(calls_in(source, "interpolate")) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The curves a script reads, each its table and `x` column as written: `interpolate("Yield curve", unit.ash, x="ash",
/// ...)` gives ("Yield curve", "ash"), so the table can be checked as a curve when either is saved. One read with a key
/// nearest or between (Waste allocation plan, decision 6) reads a grid, many rows at each `x`, and isn't checked so.
pub fn curves_in(source: &str) -> Vec<(String, String)> {
    calls_in(source, "interpolate")
        .into_iter()
        .filter(|(_, rest)| !rest.contains("nearest(") && !rest.contains("between("))
        .filter_map(|(name, rest)| quoted_after(&rest, "x").map(|x| (name, x)))
        .collect()
}

/// A named argument's quoted value in what follows a call's first argument: `x="ash"` gives ash.
fn quoted_after(rest: &str, key: &str) -> Option<String> {
    let mut from = 0;
    while let Some(at) = rest[from..].find(key).map(|a| a + from) {
        from = at + key.len();
        if rest[..at].chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let Some(after) = rest[from..].trim_start().strip_prefix('=') else { continue };
        let after = after.trim_start();
        let quote = match after.chars().next() {
            Some(q @ ('"' | '\'')) => q,
            _ => continue,
        };
        return after[1..].find(quote).map(|end| after[1..1 + end].to_string());
    }
    None
}

/// Each call of `function` whose first argument is a quoted name: the name, and the rest of its line after it.
fn calls_in(source: &str, function: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut from = 0;
        while let Some(at) = line[from..].find(function).map(|a| a + from) {
            from = at + function.len();
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
                out.push((rest[1..1 + end].to_string(), rest[2 + end..].to_string()));
            }
        }
    }
    out
}

/// The parameter columns a script reads through `p` (Table setup plan, decision 4): `p.min_burden`, `u.p.dip`,
/// `getattr(p, "rd_default", None)`, each once in order. A name built as the script runs isn't seen.
pub fn parameters_read_in(source: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut add = |n: &str| {
        if !n.is_empty() && n != "at" && !names.iter().any(|m| m == n) {
            names.push(n.to_string());
        }
    };
    for line in source.lines() {
        let line = line.split('#').next().unwrap_or("");
        let bytes = line.as_bytes();
        let mut from = 0;
        while let Some(at) = line[from..].find("p.").map(|a| a + from) {
            from = at + 2;
            if at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_') {
                continue;
            }
            let name: String = line[at + 2..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            add(&name);
        }
        let mut from = 0;
        while let Some(at) = line[from..].find("getattr(").map(|a| a + from) {
            from = at + 8;
            let rest = line[at + 8..].trim_start();
            let Some(rest) = rest.strip_prefix('p').filter(|r| r.trim_start().starts_with(',')) else { continue };
            let rest = rest.trim_start()[1..].trim_start();
            let Some(q @ ('"' | '\'')) = rest.chars().next() else { continue };
            if let Some(end) = rest[1..].find(q) {
                add(&rest[1..1 + end]);
            }
        }
    }
    names
}

/// The columns a script reads of the parameter table named `table` by name: `lookup("Rates", "bcm_per_hour", ...)`
/// gives bcm_per_hour; `interpolate("Yield curve", x, x="ash", y="yield")` gives ash and yield; a column named as the
/// script runs, `lookup("Rates", name)`, gives `*`, every column.
pub fn columns_looked_up_in(source: &str, table: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (name, rest) in calls_in(source, "lookup") {
        if name != table {
            continue;
        }
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix(',') else { continue };
        let rest = rest.trim_start();
        // A column named as the script runs (`lookup("T", name)`) can't be read off it: every column, `*`.
        let Some(q @ ('"' | '\'')) = rest.chars().next() else {
            if !names.iter().any(|n| n == "*") {
                names.push("*".to_string());
            }
            continue;
        };
        if let Some(end) = rest[1..].find(q) {
            let column = rest[1..1 + end].to_string();
            if !names.contains(&column) {
                names.push(column);
            }
        }
    }
    for (name, rest) in calls_in(source, "interpolate") {
        if name != table {
            continue;
        }
        for key in ["x", "y"] {
            if let Some(column) = quoted_after(&rest, key).filter(|c| !names.contains(c)) {
                names.push(column);
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

    #[test]
    fn curves_and_lookups_are_read_off_the_source() {
        let source = "def formula(unit):
    y = interpolate(\"Yield curve\", unit.ash, x=\"ash\", y=\"yield\", seam=unit.seam)
    r = lookup('Rates', 'bcm_per_hour')  # lookup(\"not this\")
    z = my_lookup(\"nor this\")
";
        assert_eq!(lookups_in(source), ["Rates", "Yield curve"]);
        assert_eq!(curves_in(source), [("Yield curve".to_string(), "ash".to_string())]);
        // A grid read with a graded key isn't a curve to check: many rows at each x.
        assert!(curves_in("f = interpolate(\"Factors\", t, x=\"zone_thickness\", y=\"dragline\", dip=nearest(12))").is_empty());
    }

    #[test]
    fn the_parameters_a_script_reads_are_read_off_it() {
        let source = "def allocate(column, p):\n    t = p.min_burden + u.p.dip  # p.not_this\n    d = getattr(p, \"rd_default\", None)\n    r = lookup(\"Rates\", \"bcm\", key=1)\n    y = interpolate(\"Yield\", a, x=\"ash\", y=\"yield\")\n    top.p = 1\n";
        assert_eq!(parameters_read_in(source), ["min_burden", "dip", "rd_default"]);
        assert_eq!(columns_looked_up_in(source, "Rates"), ["bcm"]);
        assert_eq!(columns_looked_up_in(source, "Yield"), ["ash", "yield"]);
        assert!(columns_looked_up_in(source, "Other").is_empty());
        assert_eq!(columns_looked_up_in("f = lambda c: lookup(\"Rates\", c)\n", "Rates"), ["*"]);
    }
}