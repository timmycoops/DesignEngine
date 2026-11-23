//! The Objects table's own (the Build Plan's "Table setup plan", decisions 9 and 10): calculated columns on the Objects
//! table, a formula on a solid's row worked out when shown and never stored, so the design stays the truth.
//!
//! They're kept in one table object of no rows marked as the Objects table's own (`Table.objects`), so adding, changing
//! or taking one out is a revision of it and undone as any. The Objects grid, a table script's `table("Objects")` and a
//! reserves run read them. A run takes one as a block attribute: a field whose source is the attribute of a calculated
//! column's name takes its value where the block hasn't that attribute, so Imported takes it as it takes an attribute.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use zd_core::{AttrValue, Body, Object, ObjectId, Project, Table};

use crate::table_calc::{self, CalcSaid};
use crate::table_grid::objects_table;

/// What the Objects table's own is named, on the `tables` layer, when the first calculated column makes it; and the name
/// a table script reads the Objects table by, `table("Objects")`.
pub const OBJECTS_OWN: &str = "Objects";

/// The density the Objects table's tonnes are worked by where a formula, a script or a run reads them: `rd`, where every
/// solid's is a number.
pub const OBJECTS_DENSITY: &str = "rd";

/// The table holding the Objects table's calculated columns, where the project has one.
pub fn objects_own(project: &Project) -> Option<&Arc<Object>> {
    project.objects().find(|o| matches!(&o.body, Body::Table(t) if t.objects))
}

/// Every solid of the project, in its order: the Objects table's rows when a formula is checked against it.
pub fn solids(project: &Project) -> Vec<ObjectId> {
    project.objects().filter(|o| matches!(o.body, Body::Solid(_))).map(|o| o.id).collect()
}

/// The Objects table of these objects with its calculated columns on, to be worked out when shown.
pub fn objects_table_with(project: &Project, ids: &[ObjectId], density: Option<&str>) -> Table {
    let mut t = objects_table(project, ids, density);
    if let Some(o) = objects_own(project)
        && let Body::Table(own) = &o.body
    {
        t.calculated = own.calculated.clone();
    }
    t
}

/// Each calculated column of the Objects table worked out on these objects, by name, a value an object (none where its
/// formula stopped on it), and what each said where it stopped.
pub fn calculated_values(project: &Project, ids: &[ObjectId]) -> (BTreeMap<String, Vec<Option<f64>>>, Vec<CalcSaid>) {
    let table = objects_table_with(project, ids, Some(OBJECTS_DENSITY));
    let mut values = BTreeMap::new();
    let mut said = Vec::new();
    let lookup = |id: ObjectId| project.object(id).cloned();
    let rows: Vec<usize> = (0..table.rows()).collect();
    let names = table.column("name");
    let label = |r: usize| match names.map(|c| &c.values) {
        Some(zd_core::ColumnValues::Text(v)) => v[r].clone().unwrap_or_default(),
        _ => format!("row {}", r + 1),
    };
    for column in &table.calculated {
        match table_calc::compile(&table, column, &lookup) {
            Ok(calc) => {
                let (v, s) = table_calc::leaf_values(&table, &calc, &rows, &label);
                if s.stopped > 0 {
                    said.push(s);
                }
                values.insert(column.name.clone(), v);
            }
            Err(problems) => said.push(CalcSaid { column: column.name.clone(), stopped: ids.len(), first: problems.first().map(|p| p.message.clone()) }),
        }
    }
    (values, said)
}

/// The run's blocks that take a calculated column of the Objects table as an attribute they haven't, each a copy with it
/// on; the rest are read as they are. Empty where the project's Objects table has no calculated columns.
pub fn blocks_with_calculated(project: &Project, blocks: &[ObjectId]) -> HashMap<ObjectId, Arc<Object>> {
    let mut out = HashMap::new();
    let Some(Body::Table(own)) = objects_own(project).map(|o| &o.body) else { return out };
    if own.calculated.is_empty() {
        return out;
    }
    let ids: Vec<ObjectId> = blocks.iter().copied().filter(|&id| project.object(id).is_some_and(|o| matches!(o.body, Body::Solid(_)))).collect();
    let (values, _) = calculated_values(project, &ids);
    for (k, &id) in ids.iter().enumerate() {
        let object = project.object(id).expect("kept above");
        let mut attributes = object.attributes.clone();
        let mut took = false;
        for (name, v) in &values {
            if let Some(x) = v[k]
                && !attributes.keys().any(|a| a.eq_ignore_ascii_case(name))
            {
                attributes.insert(name.clone(), AttrValue::Number(x));
                took = true;
            }
        }
        if took {
            out.insert(id, Arc::new(Object { attributes, ..(**object).clone() }));
        }
    }
    out
}

/// A reader of the project's objects that gives the run's blocks with the Objects table's calculated columns on, as a
/// run takes them: Open its column, Every block at a step and the trace read blocks as the run did.
pub fn with_calculated<'a>(project: &Project, blocks: &[ObjectId], get: &'a dyn Fn(ObjectId) -> Result<Arc<Object>, crate::CommandError>) -> impl Fn(ObjectId) -> Result<Arc<Object>, crate::CommandError> + 'a {
    let calculated = blocks_with_calculated(project, blocks);
    move |id| match calculated.get(&id) {
        Some(o) => Ok(o.clone()),
        None => get(id),
    }
}

/// The Objects table as a table script reads it (decision 10): a row a solid, its attributes and measures, and each
/// calculated column worked out as a column of numbers.
pub fn objects_rows(project: &Project) -> Table {
    let ids = solids(project);
    let mut t = objects_table(project, &ids, Some(OBJECTS_DENSITY));
    let units: BTreeMap<String, String> = match objects_own(project).map(|o| &o.body) {
        Some(Body::Table(own)) => own.calculated.iter().map(|c| (c.name.clone(), c.unit.clone())).collect(),
        _ => BTreeMap::new(),
    };
    let (values, _) = calculated_values(project, &ids);
    for (name, v) in values {
        if t.column(&name).is_none() {
            t.columns.push(zd_core::TableColumn::number(&name, units.get(&name).map_or("", String::as_str), v));
        }
    }
    t
}

/// Whether a script reads the Objects table by name, so it's worked out only for those that do.
pub fn reads_objects(source: &str) -> bool {
    source.contains(&format!("\"{OBJECTS_OWN}\"")) || source.contains(&format!("'{OBJECTS_OWN}'"))
}
