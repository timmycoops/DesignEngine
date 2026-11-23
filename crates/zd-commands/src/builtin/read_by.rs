//! What reads a parameter table (the Build Plan's "Table setup plan", decision 4): every dataflow, add-on, reserves table
//! and calculated column that reads it, through `p` (its place in their list) or by name; each of its columns, who reads
//! it, and the tables read before it that have the column too, whose rows come first where they fit. A column nothing
//! names is said so: no built-in step, field or script reads it.

use std::sync::Arc;

use serde::Serialize;
use zd_core::{Body, Dataflow, FieldSource, Object, ObjectId, ObjectKind, Project};

use crate::CommandError;

/// The parameter columns the built-in steps read by these names (decision 0073 on), each renamable by a step's settings.
pub const BUILT_IN_PARAMETERS: [&str; 20] = [
    "aggregate_sections",
    "clean_coal_average",
    "edge_dilution",
    "edge_loss",
    "floor_dilution",
    "floor_loss",
    "loss_between_coal",
    "max_area",
    "max_thickness",
    "min_coal_thickness",
    "min_parting",
    "min_volume",
    "other_dilution",
    "other_loss",
    "rom_ash_cutoff",
    "roof_dilution",
    "roof_loss",
    "underburden_thickness",
    "ws_split",
    "ws_strip_ratio",
];

/// One thing that reads the table: its kind (dataflow, add-on, table, calculated column), its name and id, and how it
/// reads it (`p, table 2 of 3`, or `by name`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reader {
    pub kind: String,
    pub name: String,
    pub id: ObjectId,
    pub how: String,
}

/// One column: the readers that name it, and the tables read before this one, by a reader that reads both, that have
/// the column too.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColumnRead {
    pub column: String,
    pub read_by: Vec<String>,
    pub shadowed_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadBy {
    pub table: String,
    pub readers: Vec<Reader>,
    pub columns: Vec<ColumnRead>,
}

/// The parameter columns a dataflow reads through `p`, with the add-ons it runs: the built-in steps' (as their settings
/// name them), its fields' parameter columns, a field's own name where it falls back to the column of its name, each
/// field's `_default`, `_min` and `_max` (the starter's limits), and what its scripts read.
pub(crate) fn read_through_p(d: &Dataflow, add_ons: &[Arc<Dataflow>]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut add = |n: String| {
        if !names.contains(&n) {
            names.push(n);
        }
    };
    if d.steps.iter().any(|s| s.is_built_in()) {
        for name in BUILT_IN_PARAMETERS {
            add(name.to_string());
        }
    }
    for flow in std::iter::once(d).chain(add_ons.iter().map(|a| a.as_ref())) {
        for s in &flow.steps {
            for v in s.settings.values() {
                add(v.clone());
            }
            if let Some(script) = s.script() {
                for n in zd_core::parameters_read_in(script) {
                    add(n);
                }
            }
        }
        for f in &flow.fields {
            match &f.source {
                FieldSource::Parameter { column } => add(column.clone()),
                FieldSource::Attribute { .. } => add(f.field.name.clone()),
                _ => {}
            }
            for end in ["default", "min", "max"] {
                add(format!("{}_{end}", f.field.name));
            }
        }
        for w in &flow.words {
            match &w.source {
                FieldSource::Parameter { column } => add(column.clone()),
                FieldSource::Attribute { .. } => add(w.name.clone()),
                _ => {}
            }
        }
    }
    names
}

/// The columns a dataflow's scripts look up in this table by name, under the name its scripts use.
fn read_by_name(d: &Dataflow, table: ObjectId, current: &str) -> Option<Vec<String>> {
    let at = d.lookups.iter().position(|&id| id == table)?;
    let name = d.lookup_names.get(at).filter(|n| !n.is_empty()).map_or(current, String::as_str);
    let mut columns: Vec<String> = Vec::new();
    for s in d.steps.iter().filter_map(|s| s.script()) {
        for c in zd_core::columns_looked_up_in(s, name) {
            if !columns.contains(&c) {
                columns.push(c);
            }
        }
    }
    Some(columns)
}

fn dataflow_of(project: &Project, id: ObjectId) -> Option<Arc<Dataflow>> {
    match &project.object(id)?.body {
        Body::Dataflow(d) => Some(d.clone()),
        _ => None,
    }
}

/// What reads the parameter table `table`, and each of its columns.
pub fn read_by(project: &Project, table: ObjectId) -> Result<ReadBy, CommandError> {
    let object = project.object(table).ok_or(CommandError::MissingObject(table))?;
    let Body::ParameterTable(t) = &object.body else {
        return Err(CommandError::WrongKind { id: table, expected: ObjectKind::ParameterTable, found: object.kind() });
    };
    let mut readers: Vec<Reader> = Vec::new();
    let mut columns: Vec<ColumnRead> = t.columns.iter().map(|c| ColumnRead { column: c.name.clone(), read_by: Vec::new(), shadowed_by: Vec::new() }).collect();
    let name_of = |id: ObjectId| project.object(id).map_or_else(|| "a table since deleted".to_string(), |o| o.name.clone());
    let columns_of = |id: ObjectId| -> Vec<String> {
        match project.object(id).map(|o| &o.body) {
            Some(Body::ParameterTable(t)) => t.columns.iter().map(|c| c.name.clone()).collect(),
            _ => Vec::new(),
        }
    };
    // A reader through `p`: its place in the list, the columns it names, and the tables before this one with a column too.
    let through_p = |reader: Reader, list: &[ObjectId], names: &[String], readers: &mut Vec<Reader>, columns: &mut Vec<ColumnRead>| {
        let Some(k) = list.iter().position(|&id| id == table) else { return };
        for c in columns.iter_mut() {
            if names.iter().any(|n| n.eq_ignore_ascii_case(&c.column)) && !c.read_by.contains(&reader.name) {
                c.read_by.push(reader.name.clone());
            }
            for &earlier in &list[..k] {
                let earlier_name = name_of(earlier);
                if columns_of(earlier).iter().any(|e| e.eq_ignore_ascii_case(&c.column)) && !c.shadowed_by.contains(&earlier_name) {
                    c.shadowed_by.push(earlier_name);
                }
            }
        }
        readers.push(Reader { how: format!("p, table {} of {}", k + 1, list.len()), ..reader });
    };
    let objects: Vec<&Arc<Object>> = project.objects().collect();
    for o in &objects {
        match &o.body {
            Body::Dataflow(d) => {
                let kind = if d.add_on { "add-on" } else { "dataflow" };
                let add_ons: Vec<Arc<Dataflow>> = d.add_ons.iter().filter_map(|&id| dataflow_of(project, id)).collect();
                let reader = Reader { kind: kind.into(), name: o.name.clone(), id: o.id, how: String::new() };
                through_p(reader, &d.parameters, &read_through_p(d, &add_ons), &mut readers, &mut columns);
                if let Some(looked) = read_by_name(d, table, &object.name) {
                    for c in columns.iter_mut().filter(|c| looked.contains(&c.column) || looked.iter().any(|l| l == "*")) {
                        if !c.read_by.contains(&o.name) {
                            c.read_by.push(o.name.clone());
                        }
                    }
                    readers.push(Reader { kind: kind.into(), name: o.name.clone(), id: o.id, how: "by name".into() });
                }
            }
            Body::Table(rt) => {
                if let Some(setup) = &rt.setup
                    && let (Some(list), Some(d)) = (&setup.tables, dataflow_of(project, setup.dataflow))
                {
                    let add_on_ids = setup.add_ons.clone().unwrap_or_else(|| d.add_ons.clone());
                    let add_ons: Vec<Arc<Dataflow>> = add_on_ids.iter().filter_map(|&id| dataflow_of(project, id)).collect();
                    let reader = Reader { kind: "table".into(), name: o.name.clone(), id: o.id, how: String::new() };
                    through_p(reader, list, &read_through_p(&d, &add_ons), &mut readers, &mut columns);
                }
                for c in &rt.calculated {
                    let name = format!("{} › {}", o.name, c.name);
                    let reader = Reader { kind: "calculated column".into(), name: name.clone(), id: o.id, how: String::new() };
                    through_p(reader, &c.parameters, &zd_core::parameters_read_in(&c.script), &mut readers, &mut columns);
                    if let Some(at) = c.lookups.iter().position(|&id| id == table) {
                        let used = c.lookup_names.get(at).filter(|n| !n.is_empty()).map_or(object.name.as_str(), String::as_str);
                        let looked = zd_core::columns_looked_up_in(&c.script, used);
                        for col in columns.iter_mut().filter(|col| looked.contains(&col.column) || looked.iter().any(|l| l == "*")) {
                            if !col.read_by.contains(&name) {
                                col.read_by.push(name.clone());
                            }
                        }
                        readers.push(Reader { kind: "calculated column".into(), name, id: o.id, how: "by name".into() });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(ReadBy { table: object.name.clone(), readers, columns })
}
