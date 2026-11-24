//! Checking and trying a script without writing anything (the AI script brief, decision 0104, point 4): so an agent can
//! write a script, check it against the project, try it on real rows or blocks, and only then run it. Every kind is
//! checked as its editor checks it; a calculated column, an object script and a table script are tried on the project as
//! it stands and say what they would give, nothing kept.

use std::sync::Arc;

use serde::Serialize;
use zd_core::{AttrValue, Body, CalcAddUp, CalculatedColumn, Project};

use super::brief::{BriefFor, ScriptKind};
use super::formulas::FormulaProblem;
use crate::{Command, CommandError, Inputs};

/// What trying a script found: what it would give, in lines a planner (or an agent) reads, and the problems where it
/// stopped. Nothing is written.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Tried {
    pub said: Vec<String>,
    pub problems: Vec<FormulaProblem>,
}

fn others(project: &Project, but: &str) -> zd_script::Libraries {
    Arc::new(
        project
            .objects()
            .filter_map(|o| match &o.body {
                Body::Script(s) if o.name != but => Some((o.name.clone(), s.source.clone())),
                _ => None,
            })
            .collect(),
    )
}

/// A refusal's problems, `line N: message` each, joined by `; `, as problems with their lines; the words before the first
/// line (`the object script:`) dropped.
fn problems_of(e: &str) -> Vec<FormulaProblem> {
    e.split("; ")
        .map(|part| {
            let at = part.find("line ").filter(|&i| part[i + 5..].split_once(':').is_some_and(|(n, _)| n.trim().parse::<usize>().is_ok()));
            match at {
                Some(i) => {
                    let (n, message) = part[i + 5..].split_once(':').expect("checked");
                    FormulaProblem { message: message.trim().to_string(), line: n.trim().parse().ok() }
                }
                None => FormulaProblem { message: part.to_string(), line: None },
            }
        })
        .collect()
}

fn calculated(b: &BriefFor) -> CalculatedColumn {
    let name = if b.name.trim().is_empty() { "value".to_string() } else { b.name.trim().to_string() };
    CalculatedColumn { name, unit: String::new(), script: b.source.clone(), adds: CalcAddUp::Sum, parameters: Vec::new(), scripts: Vec::new(), lookups: Vec::new(), lookup_names: Vec::new() }
}

fn calc_table(project: &Project, b: &BriefFor) -> Result<zd_core::Table, CommandError> {
    match b.table {
        Some(id) if !b.objects => match &project.object(id).ok_or(CommandError::MissingObject(id))?.body {
            Body::Table(t) => Ok((**t).clone()),
            _ => Err(CommandError::Invalid("a calculated column goes on a table".into())),
        },
        _ => Ok(crate::table_grid::objects_table(project, &crate::objects_own::solids(project), Some(crate::objects_own::OBJECTS_DENSITY))),
    }
}

/// The script checked as its editor checks it, each problem on its line; none where it holds.
pub fn check_script(project: &Project, b: &BriefFor) -> Result<Vec<FormulaProblem>, CommandError> {
    let kind = b.kind.ok_or_else(|| CommandError::Invalid("say which kind of script".into()))?;
    let libraries = super::bind_loads(project.objects(), &[b.source.as_str()]).map(|(_, l)| l);
    Ok(match kind {
        ScriptKind::Library => match zd_script::check_library(b.name.trim(), &b.source, &others(project, b.name.trim())) {
            Ok(()) => Vec::new(),
            Err(p) => p.into_iter().map(|p| FormulaProblem { message: p.message, line: p.line }).collect(),
        },
        ScriptKind::Table | ScriptKind::Allocation => match libraries {
            Ok(l) => super::check_table_script(&b.source, l),
            Err(e) => vec![FormulaProblem { message: e, line: None }],
        },
        ScriptKind::Calculated => {
            let table = calc_table(project, b)?;
            let mut column = calculated(b);
            column.scripts = super::bind_loads(project.objects(), &[b.source.as_str()]).map(|(ids, _)| ids).unwrap_or_default();
            crate::table_calc::check(&table, &column, &|id| project.object(id).cloned()).problems
        }
        ScriptKind::Object => match try_object(project, b, 1) {
            Ok(_) => Vec::new(),
            Err(e) => problems_of(&e.to_string()),
        },
        ScriptKind::Formula => {
            let mut d: zd_core::Dataflow = match (&b.content, b.dataflow.and_then(|id| project.object(id)).map(|o| &o.body)) {
                (Some(d), _) => d.clone(),
                (None, Some(Body::Dataflow(d))) => (**d).clone(),
                _ => return Err(CommandError::Invalid("a formula is checked against its dataflow: name it".into())),
            };
            let name = if b.name.trim().is_empty() { "formula" } else { b.name.trim() };
            let index = match d.steps.iter().position(|s| s.step == name) {
                Some(i) => i,
                None => {
                    let at = d.steps.iter().position(|s| s.step == "final").unwrap_or(d.steps.len());
                    let mut step = zd_core::DataflowStep::on(name);
                    step.formula = Some(zd_core::FormulaStep { script: String::new(), when: Default::default(), tests: Vec::new() });
                    d.steps.insert(at, step);
                    at
                }
            };
            if let Some(f) = d.steps[index].formula.as_mut() {
                f.script = b.source.clone();
            }
            let tables: Vec<Arc<zd_core::ParameterTable>> = d.parameters.iter().filter_map(|id| match project.object(*id).map(|o| &o.body) {
                Some(Body::ParameterTable(t)) => Some(t.clone()),
                _ => None,
            }).collect();
            let refs: Vec<&zd_core::ParameterTable> = tables.iter().map(|t| t.as_ref()).collect();
            let built_in: Vec<&str> = super::STEPS.iter().map(|(n, _)| *n).collect();
            let named = super::bind_lookups(project.objects(), &[b.source.as_str()]).map(|(_, n)| n).unwrap_or_default();
            match super::check_formula_with(&d, &refs, index, &built_in, libraries.unwrap_or_default(), named) {
                Ok(report) => report.problems,
                Err(e) => vec![FormulaProblem { message: e, line: None }],
            }
        }
    })
}

/// An object script run on the project as it stands, nothing kept: the blocks it would change, and what it would set on
/// the first `shown` of them.
fn try_object(project: &Project, b: &BriefFor, shown: usize) -> Result<Vec<String>, CommandError> {
    // The layers it's meant for, else every solid.
    let objects = if b.layers.is_empty() { crate::objects_own::solids(project) } else { Vec::new() };
    if b.layers.is_empty() && objects.is_empty() {
        return Err(CommandError::Invalid("the project has no solids to try it on".into()));
    }
    let p = super::ObjectScriptParams { objects, layers: b.layers.clone(), filter: Default::default(), script: None, source: Some(b.source.clone()), parameters: Vec::new() };
    let out = super::ObjectScript.run(&Inputs::of(project), &p)?;
    let mut said: Vec<String> = out.notes.iter().map(|n| n.text.clone()).collect();
    for change in out.changes.iter().take(shown) {
        if let zd_core::Change::Revise { id, attributes: Some(next), .. } = change
            && let Some(o) = project.object(*id)
        {
            let set: Vec<String> = next
                .iter()
                .filter(|(k, v)| o.attributes.get(k) != Some(*v))
                .map(|(k, v)| format!("{k} = {}", match v {
                    AttrValue::Number(x) => format!("{x}"),
                    other => format!("\"{}\"", zd_core::attribute_word(other)),
                }))
                .collect();
            said.push(format!("{}: {}", o.name, set.join(", ")));
        }
    }
    Ok(said)
}

/// The script tried on the project, nothing kept: a calculated column on a row (`row`, from 0) line by line; an object
/// script on every solid, what it would set on the first few; a table script, the table it would give and its first rows.
/// A formula or allocation step is tried in the Dataflow tab, on a block's column.
pub fn try_script(project: &Project, b: &BriefFor, row: usize) -> Result<Tried, CommandError> {
    let kind = b.kind.ok_or_else(|| CommandError::Invalid("say which kind of script".into()))?;
    let problems = check_script(project, b)?;
    if !problems.is_empty() {
        return Ok(Tried { said: vec!["It doesn't check: fix these first.".into()], problems });
    }
    let said = match kind {
        ScriptKind::Calculated => {
            let table = calc_table(project, b)?;
            if row >= table.rows() {
                return Err(CommandError::Invalid(format!("the table has {} rows", table.rows())));
            }
            let mut column = calculated(b);
            column.scripts = super::bind_loads(project.objects(), &[b.source.as_str()]).map(|(ids, _)| ids).unwrap_or_default();
            let lookup = |id| project.object(id).cloned();
            let calc = crate::table_calc::compile(&table, &column, &lookup).map_err(|p| CommandError::Invalid(p.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("; ")))?;
            let label = match table.column("name").map(|c| &c.values) {
                Some(zd_core::ColumnValues::Text(v)) => v[row].clone().unwrap_or_default(),
                _ => format!("row {}", row + 1),
            };
            let trace = crate::table_calc::trace_row(&table, &calc, row, &label);
            let mut said = vec![format!("on {label}:")];
            said.extend(trace.lines.iter().map(|l| format!("line {}: {}  →  {}", l.line, l.text.trim(), l.values.iter().map(|(n, v)| format!("{n} = {v}")).collect::<Vec<_>>().join(", "))));
            said.extend(trace.sets.iter().map(|(n, v)| format!("sets {n} = {v}")));
            if let Some(stop) = &trace.stopped {
                said.push(format!("stopped{}: {}", stop.line.map(|l| format!(" at line {l}")).unwrap_or_default(), stop.message));
            }
            said
        }
        ScriptKind::Object => try_object(project, b, 10)?,
        ScriptKind::Table => {
            let p = super::TableScriptParams { table: if b.objects { None } else { b.table }, objects: b.objects || b.table.is_none(), script: None, source: Some(b.source.clone()), parameters: Vec::new(), into: super::ScriptInto::New, name: String::new(), layer: super::TABLES_LAYER.into() };
            let out = super::RunTableScript.run(&Inputs::of(project), &p)?;
            let mut said: Vec<String> = out.notes.iter().map(|n| n.text.clone()).collect();
            for change in &out.changes {
                if let zd_core::Change::Create { body: Body::Table(t), .. } = change {
                    let names: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
                    said.push(format!("columns: {}", names.join(", ")));
                    for r in 0..t.rows().min(5) {
                        let cells: Vec<String> = t.columns.iter().map(|c| match &c.values {
                            zd_core::ColumnValues::Number(v) => v[r].map_or(String::new(), |x| format!("{x}")),
                            zd_core::ColumnValues::Text(v) => v[r].clone().unwrap_or_default(),
                        }).collect();
                        said.push(format!("row {}: {}", r + 1, cells.join(" | ")));
                    }
                }
            }
            said
        }
        ScriptKind::Formula | ScriptKind::Allocation => vec!["It checks. A dataflow step is tried on a block's column in the Dataflow tab, or by running the table again.".into()],
        ScriptKind::Library => vec!["It checks. A script of functions is tried through what loads it.".into()],
    };
    Ok(Tried { said, problems: Vec::new() })
}
