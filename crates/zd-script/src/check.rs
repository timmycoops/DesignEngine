//! The checks a formula passes before it's saved (decision 0077): every `unit.name` a field or label of the dataflow,
//! every field it sets declared (labels are read only), every `p.name` a parameter column, every add-up a number field.
//! Read off the source with comments and strings blanked, so a word in a comment or a quote is never taken for a name.

use crate::{Declared, Kind, Problem};

/// " (did you mean `x`?)" for the nearest name within two edits, or nothing.
pub(crate) fn did_you_mean<'a>(name: &str, names: impl Iterator<Item = &'a str>) -> String {
    let near = names.map(|n| (edits(&name.to_lowercase(), &n.to_lowercase()), n)).filter(|(d, _)| *d <= 2).min_by_key(|(d, _)| *d);
    near.map(|(_, n)| format!(" (did you mean `{n}`?)")).unwrap_or_default()
}

fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(here + 1);
            prev = here;
        }
    }
    row[b.len()]
}

/// The source with each comment and string's characters turned to spaces, lines and columns kept.
pub(crate) fn blank_comments_and_strings(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            let triple = i + 2 < chars.len() && chars[i + 1] == c && chars[i + 2] == c;
            let len = if triple { 3 } else { 1 };
            out.extend(std::iter::repeat_n(' ', len));
            i += len;
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push_str("  ");
                    i += 2;
                    continue;
                }
                let ends = if triple { i + 2 < chars.len() && chars[i] == c && chars[i + 1] == c && chars[i + 2] == c } else { chars[i] == c || chars[i] == '\n' };
                if ends {
                    let n = if triple { 3 } else if chars[i] == '\n' { 0 } else { 1 };
                    out.extend(std::iter::repeat_n(' ', n));
                    i += n;
                    break;
                }
                out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// A dotted name in the source: its parts, its line (from 1), and whether it's set (`a.b = …`, `a.b += …`).
#[derive(Debug)]
pub(crate) struct Chain {
    pub parts: Vec<String>,
    pub line: usize,
    pub set: bool,
    /// Set by `+=` and the like, which read it first.
    pub augmented: bool,
}

pub(crate) fn chains(source: &str) -> Vec<Chain> {
    let text = blank_comments_and_strings(source);
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut line = 1;
    let mut i = 0;
    let ident_start = |c: char| c.is_ascii_alphabetic() || c == '_';
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        let after_dot = i > 0 && chars[i - 1] == '.';
        let after_ident = i > 0 && ident(chars[i - 1]);
        if ident_start(c) && !after_dot && !after_ident {
            let mut parts = Vec::new();
            loop {
                let start = i;
                while i < chars.len() && ident(chars[i]) {
                    i += 1;
                }
                parts.push(chars[start..i].iter().collect::<String>());
                if i + 1 < chars.len() && chars[i] == '.' && ident_start(chars[i + 1]) {
                    i += 1;
                } else {
                    break;
                }
            }
            if parts.len() > 1 {
                let mut j = i;
                while j < chars.len() && chars[j] == ' ' {
                    j += 1;
                }
                let next: String = chars[j..(j + 2).min(chars.len())].iter().collect();
                let augmented = ["+=", "-=", "*=", "/="].contains(&next.as_str());
                let set = next.starts_with('=') && next != "==" || augmented;
                out.push(Chain { parts, line, set, augmented });
            }
            continue;
        }
        i += 1;
    }
    out
}

/// The names a formula's function takes: `def formula(unit, p):` gives ("unit", Some("p")).
pub(crate) fn signature(source: &str) -> Option<(usize, String, Option<String>)> {
    signature_of(source, "formula")
}

/// A function's line and its first two parameters' names, as [`signature`] reads `formula`'s: a table script's `run`.
pub(crate) fn signature_of(source: &str, function: &str) -> Option<(usize, String, Option<String>)> {
    let text = blank_comments_and_strings(source);
    let head = format!("def {function}");
    for (n, l) in text.lines().enumerate() {
        let t = l.trim_start();
        if let Some(rest) = t.strip_prefix(head.as_str()).filter(|r| r.trim_start().starts_with('(')) {
            let rest = rest.trim_start();
            let inside = rest.strip_prefix('(')?.split(')').next()?;
            let names: Vec<String> = inside.split(',').map(|s| s.trim().split('=').next().unwrap_or("").trim().to_string()).filter(|s| !s.is_empty()).collect();
            return match names.as_slice() {
                [u] => Some((n + 1, u.clone(), None)),
                [u, p] => Some((n + 1, u.clone(), Some(p.clone()))),
                _ => Some((n + 1, String::new(), None)),
            };
        }
    }
    None
}

/// The fields of the dataflow a formula reads (decision 0084), each once in the order first read: `unit.name`, and a field
/// read through `unit.above`, `unit.section`, `unit.column` or `unit.rom_coal`. A plain `unit.name = …` sets it without
/// reading it; `+=` reads it first. Labels, block attributes and parameters aren't fields; a name worked out as it runs
/// (`unit.above[m]`) can't be read off the source.
pub(crate) fn fields_read(source: &str, declared: &Declared, unit: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in chains(source) {
        let [head, rest @ ..] = c.parts.as_slice() else { continue };
        if head != unit {
            continue;
        }
        let name = match rest {
            [part, name, ..] if matches!(part.as_str(), "section" | "column" | "rom_coal" | "above") => name,
            [name] if c.set && !c.augmented => continue,
            [name, ..] => name,
            [] => continue,
        };
        if declared.is_field(name) && !out.contains(name) {
            out.push(name.clone());
        }
    }
    out
}

/// Every name a formula uses that the dataflow doesn't have, said with its line.
pub(crate) fn names(source: &str, declared: &Declared, unit: &str, p: Option<&str>) -> Vec<Problem> {
    let mut problems = Vec::new();
    let fields = || declared.fields.iter().map(|f| f.name.as_str()).chain(declared.labels.iter().map(String::as_str));
    let numbers = || declared.fields.iter().filter(|f| f.kind == Kind::Number).map(|f| f.name.as_str());
    let mut say = |line: usize, message: String| problems.push(Problem { message, line: Some(line), unit: None });
    for c in chains(source) {
        let [head, rest @ ..] = c.parts.as_slice() else { continue };
        if head == unit {
            match rest {
                [part, name, ..] if part == "section" || part == "column" || part == "rom_coal" => {
                    if !declared.fields.iter().any(|f| &f.name == name && f.kind == Kind::Number) {
                        say(c.line, format!("`{unit}.{part}.{name}`: a {part} adds up number fields, and `{name}` isn't one{}", did_you_mean(name, numbers())));
                    }
                }
                [part, name, ..] if part == "above" => {
                    if !declared.is_field(name) && !declared.is_label(name) {
                        say(c.line, format!("`{unit}.above.{name}` isn't a field of this dataflow{}", did_you_mean(name, fields())));
                    }
                }
                [part, ..] if part == "block" || part == "section" || part == "column" || part == "above" || part == "rom_coal" => {}
                [name, ..] => {
                    if c.set && declared.is_label(name) {
                        say(c.line, format!("`{unit}.{name}` is a label of the unit: a formula reads it but can't set it"));
                    } else if !declared.is_field(name) && !declared.is_label(name) {
                        say(c.line, format!("`{unit}.{name}` isn't a field of this dataflow{}", did_you_mean(name, fields())));
                    }
                }
                [] => {}
            }
        } else if Some(head.as_str()) == p
            && let [name, ..] = rest
        {
            if c.set {
                say(c.line, format!("`{head}.{name}`: parameters are read, never set"));
            } else if !declared.parameters.iter().any(|q| &q.name == name) {
                say(c.line, format!("`{head}.{name}` isn't a parameter column of this dataflow's tables{}", did_you_mean(name, declared.parameters.iter().map(|q| q.name.as_str()))));
            }
        }
    }
    problems
}
