//! Calculated columns (the Build Plan's "Tables and scripts plan", decision 6, `docs/decisions/0099-tables-and-scripts.md`):
//! a column worked out from each row of a table by a formula, as a reserves dataflow's formula steps are (0077), the row
//! handed over as a formula's unit is: `def formula(row, p): row.strip_ratio = row["Insitu\\Waste\\volume"] / …`.
//!
//! A formula reads the row's numbers by their columns, its words, and the parameter tables the column names, a row's
//! parameters the first row of them that fits its words. It's checked as formulas are, every problem on its line, and
//! traced on one row line by line. A group's value is worked again on the group's own inputs, each added up by its rule
//! (a ratio's way), or worked on each row and then added up (summed, the largest, the smallest, or weighted by a column).

use std::sync::Arc;

use serde::Serialize;
use zd_core::{AttrValue, Attributes, CalcAddUp, CalculatedColumn, ColumnValues, ObjectId, ParamValue, ParameterTable, Table};
use zd_script::{ColumnInput, Declared, FieldDecl, Formula, Problem, ScriptValue, UnitInput, Values};

use crate::builtin::{FormulaTraceView, TraceLineView};
use crate::builtin::FormulaProblem;
use crate::reserves_view::add_up_column;

/// The project's objects a calculated column reads, by id: its parameter tables and the scripts it loads.
pub type ParameterLookup<'a> = &'a dyn Fn(ObjectId) -> Option<Arc<zd_core::Object>>;

/// The names a calculated column's formula is given: every number column of the table, and the column itself, which it
/// sets; every word column as a label; and its parameter tables' columns.
pub fn declared(table: &Table, column: &CalculatedColumn, parameters: &[Arc<ParameterTable>]) -> Declared {
    let mut fields: Vec<FieldDecl> = table
        .columns
        .iter()
        .filter(|c| matches!(c.values, ColumnValues::Number(_)))
        .map(|c| FieldDecl::number(&c.name, &c.unit))
        .collect();
    fields.push(FieldDecl::number(&column.name, &column.unit));
    let labels = table.columns.iter().filter(|c| matches!(c.values, ColumnValues::Text(_))).map(|c| c.name.clone()).collect();
    let mut params: Vec<FieldDecl> = Vec::new();
    for t in parameters {
        for c in &t.columns {
            if !params.iter().any(|p| p.name == c.name) {
                params.push(if c.text { FieldDecl::text(&c.name) } else { FieldDecl::number(&c.name, &c.unit) });
            }
        }
    }
    Declared { fields, labels, parameters: params, libraries: Default::default() }
}

/// A calculated column ready to work out: its formula compiled against the table, its parameter tables, and the
/// table's columns it reads.
pub struct Calc<'a> {
    pub column: &'a CalculatedColumn,
    formula: Formula,
    parameters: Vec<Arc<ParameterTable>>,
    /// The number columns its formula reads, read off its source: only those are handed over.
    reads: Vec<String>,
}

/// Its parameter tables, by the ids it names; one gone is said.
pub fn parameter_tables(column: &CalculatedColumn, lookup: ParameterLookup<'_>) -> Result<Vec<Arc<ParameterTable>>, String> {
    column
        .parameters
        .iter()
        .map(|&id| match lookup(id).map(|o| o.body.clone()) {
            Some(zd_core::Body::ParameterTable(t)) => Ok(t),
            _ => Err(format!("{} reads a parameter table since deleted", column.name)),
        })
        .collect()
}

/// The formula compiled against the table: every problem with its line, any one refusing it.
pub fn compile<'a>(table: &Table, column: &'a CalculatedColumn, lookup: ParameterLookup<'_>) -> Result<Calc<'a>, Vec<Problem>> {
    let parameters = parameter_tables(column, lookup).map_err(|e| vec![Problem { message: e, line: None, unit: None }])?;
    let mut names = declared(table, column, &parameters);
    names.libraries = crate::builtin::libraries_of(&|id| lookup(id), &column.scripts).map_err(|e| vec![Problem { message: e, line: None, unit: None }])?;
    let formula = Formula::compile(&column.script, names)?;
    let numbers: Vec<&str> = table.columns.iter().filter(|c| matches!(c.values, ColumnValues::Number(_))).map(|c| c.name.as_str()).collect();
    let mut reads: Vec<String> = formula.fields_read().into_iter().filter(|f| numbers.contains(&f.as_str())).collect();
    // A name read in a way the source scan can't see (built up in a loop): every number column, to be safe.
    if reads.is_empty() {
        reads = numbers.iter().map(|n| n.to_string()).collect();
    }
    Ok(Calc { column, formula, parameters, reads })
}

/// A row's words as attributes, for its parameter rows to be matched on.
fn words_of(table: &Table, row: usize) -> Attributes {
    let mut a = Attributes::new();
    for c in &table.columns {
        if let ColumnValues::Text(v) = &c.values
            && let Some(w) = v[row].as_deref().filter(|w| !w.is_empty())
        {
            a.insert(c.name.clone(), AttrValue::Text(w.to_string()));
        }
    }
    a
}

fn parameter_values(calc: &Calc<'_>, words: &Attributes) -> Values {
    let tables: Vec<&ParameterTable> = calc.parameters.iter().map(|t| t.as_ref()).collect();
    let mut out = Vec::new();
    for t in &calc.parameters {
        for c in &t.columns {
            if out.iter().any(|(n, _): &(&str, ScriptValue)| *n == c.name) {
                continue;
            }
            let value = match ParameterTable::lookup_in(&tables, words, &c.name).map(|f| f.value) {
                Some(ParamValue::Number(x)) => ScriptValue::Number(*x),
                Some(ParamValue::Text(w)) => ScriptValue::Text(w.clone()),
                None => ScriptValue::None,
            };
            out.push((c.name.as_str(), value));
        }
    }
    zd_script::values(out)
}

/// The unit a row is handed over as: the numbers it reads, its words, its parameters.
fn row_unit(table: &Table, calc: &Calc<'_>, row: usize) -> UnitInput {
    let mut fields: Vec<(&str, ScriptValue)> = Vec::new();
    for name in &calc.reads {
        if let Some(ColumnValues::Number(v)) = table.column(name).map(|c| &c.values) {
            fields.push((name.as_str(), v[row].map_or(ScriptValue::None, ScriptValue::Number)));
        }
    }
    for c in &table.columns {
        if let ColumnValues::Text(v) = &c.values {
            fields.push((c.name.as_str(), v[row].clone().filter(|w| !w.is_empty()).map_or(ScriptValue::None, ScriptValue::Text)));
        }
    }
    let words = words_of(table, row);
    UnitInput { fields: zd_script::values(fields), block: zd_script::values([]), parameters: parameter_values(calc, &words), section: None, rom_coal: None }
}

/// A group handed over as one unit: each number it reads added up over the group by its rule, each word where every row
/// has the same, and the parameters for those words.
fn group_unit(table: &Table, calc: &Calc<'_>, rows: &[usize]) -> UnitInput {
    let mut fields: Vec<(&str, ScriptValue)> = Vec::new();
    for name in &calc.reads {
        fields.push((name.as_str(), add_up_column(table, name, rows).map_or(ScriptValue::None, ScriptValue::Number)));
    }
    let mut words = Attributes::new();
    for c in &table.columns {
        if let ColumnValues::Text(v) = &c.values {
            let mut all = rows.iter().map(|&r| v[r].as_deref().unwrap_or(""));
            let first = all.next().unwrap_or("");
            let same = !first.is_empty() && all.all(|w| w == first);
            fields.push((c.name.as_str(), if same { ScriptValue::Text(first.to_string()) } else { ScriptValue::None }));
            if same {
                words.insert(c.name.clone(), AttrValue::Text(first.to_string()));
            }
        }
    }
    UnitInput { fields: zd_script::values(fields), block: zd_script::values([]), parameters: parameter_values(calc, &words), section: None, rom_coal: None }
}

fn number(v: Option<&ScriptValue>) -> Option<f64> {
    match v {
        Some(ScriptValue::Number(x)) if x.is_finite() => Some(*x),
        _ => None,
    }
}

/// What working a column out said: rows the formula stopped on, by the first's row and line, and how many.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CalcSaid {
    pub column: String,
    pub stopped: usize,
    pub first: Option<String>,
}

/// How many rows a formula is run on at once; a batch it stops in is run again a row at a time.
const BATCH: usize = 256;

/// The column worked out on each of `rows` (the rest none), and what stopped it where it stopped: a row the formula
/// stops on (a division by an empty coal, words where a number is) has no value, said, and the rest go on. Rows are run
/// a batch at a time, and only a batch the formula stops in a row at a time, so many such rows stay quick.
pub fn leaf_values(table: &Table, calc: &Calc<'_>, rows: &[usize], label: &dyn Fn(usize) -> String) -> (Vec<Option<f64>>, CalcSaid) {
    let mut values = vec![None; table.rows()];
    let mut said = CalcSaid { column: calc.column.name.clone(), ..Default::default() };
    let input = ColumnInput { units: rows.iter().map(|&r| row_unit(table, calc, r)).collect(), ..Default::default() };
    let all: Vec<usize> = (0..rows.len()).collect();
    for batch in all.chunks(BATCH) {
        let mut keep = |sets: Vec<zd_script::UnitSets>| {
            for s in sets {
                values[rows[s.unit]] = number(s.get(&calc.column.name));
            }
        };
        match calc.formula.run(&input, batch) {
            Ok(sets) => keep(sets),
            Err(_) => {
                for &u in batch {
                    match calc.formula.run(&input, &[u]) {
                        Ok(sets) => keep(sets),
                        Err(p) => {
                            said.stopped += 1;
                            if said.first.is_none() {
                                let at = p.line.map_or(String::new(), |l| format!("line {l}: "));
                                said.first = Some(format!("{} ({at}{})", label(rows[u]), p.message));
                            }
                        }
                    }
                }
            }
        }
    }
    (values, said)
}

/// A group's value: worked again on its own inputs, or its rows' values added up by the column's rule.
pub fn group_value(table: &Table, calc: &Calc<'_>, rows: &[usize], values: &[Option<f64>]) -> Option<f64> {
    let each = || rows.iter().filter_map(|&r| values[r]);
    match &calc.column.adds {
        CalcAddUp::FromInputs => {
            if rows.is_empty() {
                return None;
            }
            let input = ColumnInput { units: vec![group_unit(table, calc, rows)], ..Default::default() };
            calc.formula.run(&input, &[0]).ok().and_then(|sets| number(sets.first().and_then(|s| s.get(&calc.column.name))))
        }
        CalcAddUp::Sum => {
            let v: Vec<f64> = each().collect();
            (!v.is_empty()).then(|| v.iter().sum())
        }
        CalcAddUp::Max => each().reduce(f64::max),
        CalcAddUp::Min => each().reduce(f64::min),
        CalcAddUp::WeightedBy { column } => {
            let Some(ColumnValues::Number(w)) = table.column(column).map(|c| &c.values) else { return None };
            let (mut sum, mut total) = (0.0, 0.0);
            for &r in rows {
                if let (Some(x), Some(w)) = (values[r], w[r]) {
                    sum += x * w;
                    total += w;
                }
            }
            (total != 0.0).then(|| sum / total)
        }
    }
}

/// The formula traced on one row (decision 6, as 0077's formulas are): the row as it found it, the lines that ran with
/// their values, what it read and set, what it printed, and where it stopped. Nothing is kept.
pub fn trace_row(table: &Table, calc: &Calc<'_>, row: usize, label: &str) -> FormulaTraceView {
    let input = ColumnInput { units: vec![row_unit(table, calc, row)], ..Default::default() };
    let before = calc
        .reads
        .iter()
        .map(|n| match table.column(n).map(|c| &c.values) {
            Some(ColumnValues::Number(v)) => (n.clone(), v[row]),
            _ => (n.clone(), None),
        })
        .collect();
    let trace = calc.formula.trace(&input, 0);
    let show = |v: &ScriptValue| v.to_string();
    FormulaTraceView {
        block: label.to_string(),
        unit: calc.column.name.clone(),
        fits: true,
        before,
        lines: trace.lines.into_iter().map(|l| TraceLineView { line: l.line, text: l.text, values: l.values }).collect(),
        reads: trace.events.iter().filter_map(|e| match e {
            zd_script::Event::Read { what, value } => Some((what.clone(), show(value))),
            _ => None,
        }).collect(),
        sets: trace.sets.iter().map(|(k, v)| (k.clone(), show(v))).collect(),
        said: trace.said,
        stopped: trace.stopped.map(|p| FormulaProblem { message: p.message, line: p.line }),
    }
}

/// What a calculated column's formula may use, for its editor and Copy for AI: the table's numbers with their units,
/// its words, and the parameters with theirs (words marked). Names only, never a value.
#[derive(Debug, Clone, Serialize)]
pub struct CalcNames {
    pub fields: Vec<(String, String)>,
    pub labels: Vec<String>,
    pub parameters: Vec<(String, String, bool)>,
}

/// A calculated column checked: its problems, each on its line, and the names it may use.
#[derive(Debug, Clone, Serialize)]
pub struct CalcCheck {
    pub problems: Vec<FormulaProblem>,
    pub names: CalcNames,
}

pub fn check(table: &Table, column: &CalculatedColumn, lookup: ParameterLookup<'_>) -> CalcCheck {
    let parameters = parameter_tables(column, lookup).unwrap_or_default();
    let d = declared(table, column, &parameters);
    let names = CalcNames {
        fields: d.fields.iter().filter(|f| f.name != column.name).map(|f| (f.name.clone(), f.unit.clone())).collect(),
        labels: d.labels.clone(),
        parameters: d.parameters.iter().map(|p| (p.name.clone(), p.unit.clone(), p.kind == zd_script::Kind::Text)).collect(),
    };
    let mut problems = match compile(table, column, lookup) {
        Ok(_) => Vec::new(),
        Err(p) => p.into_iter().map(|p| FormulaProblem { message: p.message, line: p.line }).collect(),
    };
    if problems.is_empty() && !sets_itself(&column.script, &column.name) {
        problems.push(FormulaProblem { message: format!("the formula never sets `row.{}`, so the column has no value: set it, `row.{0} = …`", column.name), line: None });
    }
    CalcCheck { problems, names }
}

/// Whether the source sets the column on the row, by `.name =` or `["name"] =`.
fn sets_itself(script: &str, name: &str) -> bool {
    let set = |s: &str| script.lines().any(|l| l.split('#').next().unwrap_or("").replace(' ', "").contains(s));
    set(&format!(".{name}=")) || set(&format!("[\"{name}\"]=")) || set(&format!("['{name}']="))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table_grid::{Cell, GridAsk, grid_with};
    use zd_core::TableColumn;
    use zd_reserves::AddUp;

    /// Two blocks of strip 1 at 100 m³ of waste over 10 t of coal and 300 over 50, ash 10 and 20; B3 of strip 2 with no
    /// coal at all.
    fn made_up() -> Table {
        let w = |v: &[&str]| v.iter().map(|s| Some(s.to_string())).collect::<Vec<_>>();
        let n = |v: &[f64]| v.iter().map(|&x| Some(x)).collect::<Vec<_>>();
        Table {
            columns: vec![
                TableColumn::text("strip", w(&["1", "1", "2"])),
                TableColumn::text("block", w(&["B1", "B2", "B3"])),
                TableColumn::text("seam", w(&["U", "U", "L"])),
                TableColumn::adding("Insitu\\Waste\\volume", "m3", AddUp::Sum, n(&[100.0, 300.0, 50.0])),
                TableColumn::adding("Insitu\\Coal\\tonnes", "t", AddUp::Sum, n(&[10.0, 50.0, 0.0])),
                TableColumn::number("Insitu\\Coal\\ash", "%", n(&[10.0, 20.0, 15.0])),
            ],
            calculated: Vec::new(),
        }
    }

    fn calc(name: &str, script: &str, adds: CalcAddUp) -> CalculatedColumn {
        CalculatedColumn { name: name.into(), unit: String::new(), script: script.into(), adds, parameters: Vec::new(), scripts: Vec::new() }
    }

    const RATIO: &str = "def formula(row):\n    row.strip_ratio = row[\"Insitu\\\\Waste\\\\volume\"] / row[\"Insitu\\\\Coal\\\\tonnes\"]\n";

    fn none(_: ObjectId) -> Option<Arc<zd_core::Object>> {
        None
    }

    fn numbers(cells: &[Option<Cell>]) -> Vec<Option<f64>> {
        cells.iter().map(|c| match c {
            Some(Cell::Number(x)) => Some(*x),
            _ => None,
        }).collect()
    }

    #[test]
    fn a_ratio_adds_up_from_the_group_s_own_inputs_not_its_rows() {
        let mut t = made_up();
        t.calculated.push(calc("strip_ratio", RATIO, CalcAddUp::FromInputs));
        let g = grid_with(&t, &GridAsk { group: vec!["strip".into()], open: vec![vec!["1".into()]], columns: vec!["strip_ratio".into()], ..Default::default() }, &none).unwrap();
        let ratios: Vec<(String, Option<f64>)> = g.rows.iter().map(|r| (r.label.clone(), numbers(&r.cells)[0])).collect();
        // Strip 1: 400 / 60 = 6.67, not the average 8 of its blocks' 10 and 6. B3 has no coal: no value, said.
        assert_eq!(ratios[1].0, "1");
        assert!((ratios[1].1.unwrap() - 400.0 / 60.0).abs() < 1e-12, "{ratios:?}");
        assert_eq!((ratios[2].1, ratios[3].1), (Some(10.0), Some(6.0)));
        assert_eq!(ratios[4], ("2".into(), None), "strip 2 has no coal");
        assert!((ratios[0].1.unwrap() - 450.0 / 60.0).abs() < 1e-12, "the total from its own inputs: {:?}", ratios[0]);
        assert_eq!(g.columns[0].kind, zd_core::ColumnKind::Calculated);
        assert_eq!(g.said.len(), 1);
        assert_eq!(g.said[0].split(" (line").next(), Some("strip_ratio: no value on 1 row, 2 B3 L"), "{:?}", g.said);
        assert!(g.said[0].contains("(line 2: "), "{:?}", g.said);
    }

    #[test]
    fn a_test_on_each_row_is_added_up_by_its_rule() {
        let mut t = made_up();
        let low = "def formula(row):\n    ash = row[\"Insitu\\\\Coal\\\\ash\"]\n    row.low_ash = row[\"Insitu\\\\Coal\\\\tonnes\"] if ash < 12 else 0\n";
        t.calculated.push(calc("low_ash", low, CalcAddUp::Sum));
        t.calculated.push(calc("ash_by_tonnes", "def formula(row):\n    row.ash_by_tonnes = row[\"Insitu\\\\Coal\\\\ash\"]\n", CalcAddUp::WeightedBy { column: "Insitu\\Coal\\tonnes".into() }));
        t.calculated.push(calc("most_ash", "def formula(row):\n    row.most_ash = row[\"Insitu\\\\Coal\\\\ash\"]\n", CalcAddUp::Max));
        let g = grid_with(&t, &GridAsk { group: vec!["strip".into()], columns: vec!["low_ash".into(), "ash_by_tonnes".into(), "most_ash".into()], ..Default::default() }, &none).unwrap();
        let strip1 = numbers(&g.rows[1].cells);
        // 10 t under 12 % and 0; ash by tonnes (10 × 10 + 20 × 50) / 60; the largest 20.
        assert_eq!(strip1[0], Some(10.0));
        assert!((strip1[1].unwrap() - 1_100.0 / 60.0).abs() < 1e-12);
        assert_eq!(strip1[2], Some(20.0));
    }

    #[test]
    fn a_row_s_parameters_are_the_first_row_that_fits_its_words() {
        let mut t = made_up();
        let prices = Arc::new(ParameterTable::from_csv("when seam,price ($/t)\nU,80\n,60\n").unwrap());
        let id = ObjectId::new();
        let object = Arc::new(zd_core::Object { id, revision: 1, name: "prices".into(), layer: zd_core::LayerId::new(), provenance: zd_core::RunId::new(), attributes: Default::default(), body: zd_core::Body::ParameterTable(prices) });
        let lookup = |asked: ObjectId| (asked == id).then(|| object.clone());
        let mut value = calc("value", "def formula(row, p):\n    row.value = row[\"Insitu\\\\Coal\\\\tonnes\"] * p.price\n", CalcAddUp::Sum);
        value.parameters = vec![id];
        t.calculated.push(value);
        let g = grid_with(&t, &GridAsk { columns: vec!["value".into()], ..Default::default() }, &lookup).unwrap();
        // U at $80: 800 + 4,000; L at the default $60 on no coal: 0.
        assert_eq!(numbers(&g.rows[0].cells)[0], Some(4_800.0));
        assert_eq!(numbers(&g.rows[3].cells)[0], Some(0.0));
    }

    #[test]
    fn a_formula_is_checked_and_traced_as_the_dataflow_s_are() {
        let t = made_up();
        let wrong = calc("strip_ratio", "def formula(row):\n    row.strip_ratio = row.Insitu_volume\n", CalcAddUp::FromInputs);
        let checked = check(&t, &wrong, &none);
        assert_eq!(checked.problems.len(), 1);
        assert_eq!(checked.problems[0].line, Some(2));
        let never = calc("strip_ratio", "def formula(row):\n    x = 1\n", CalcAddUp::FromInputs);
        assert!(check(&t, &never, &none).problems[0].message.contains("never sets `row.strip_ratio`"));
        assert!(check(&t, &calc("strip_ratio", RATIO, CalcAddUp::FromInputs), &none).problems.is_empty());
        assert!(checked.names.fields.iter().any(|(n, u)| n == "Insitu\\Coal\\tonnes" && u == "t"));
        assert_eq!(checked.names.labels, ["strip", "block", "seam"]);

        let ratio = calc("strip_ratio", RATIO, CalcAddUp::FromInputs);
        let c = compile(&t, &ratio, &none).unwrap();
        let traced = trace_row(&t, &c, 1, "1 B2 U");
        assert_eq!(traced.sets, [("strip_ratio".to_string(), "6".to_string())]);
        assert_eq!(traced.lines.iter().map(|l| l.line).collect::<Vec<_>>(), [2]);
        assert!(traced.stopped.is_none());
        let stopped = trace_row(&t, &c, 2, "2 B3 L");
        assert!(stopped.stopped.is_some_and(|p| p.line == Some(2)), "division by zero on B3");
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use zd_core::TableColumn;

    /// Across batches: 600 rows, every third with no coal. Those 200 stop, said once with the first; the other 400 each
    /// have their own waste over coal, rows before and after a stop alike.
    #[test]
    fn rows_a_formula_stops_on_leave_the_rest_their_values() {
        let n = 600;
        let rows: Vec<usize> = (0..n).collect();
        let t = Table {
            columns: vec![
                TableColumn::text("block", rows.iter().map(|r| Some(format!("B{r}"))).collect()),
                TableColumn::number("waste", "m3", rows.iter().map(|&r| Some(r as f64 + 1.0)).collect()),
                TableColumn::number("coal", "m3", rows.iter().map(|&r| if r % 3 == 2 { None } else { Some(2.0) }).collect()),
            ],
            calculated: Vec::new(),
        };
        let c = CalculatedColumn { name: "ratio".into(), unit: String::new(), script: "def formula(row):\n    row.ratio = row.waste / row.coal\n".into(), adds: CalcAddUp::Sum, parameters: Vec::new(), scripts: Vec::new() };
        let none = |_: ObjectId| -> Option<Arc<zd_core::Object>> { None };
        let calc = compile(&t, &c, &none).unwrap();
        let (values, said) = leaf_values(&t, &calc, &rows, &|r| format!("B{r}"));
        assert_eq!(said.stopped, 200);
        assert!(said.first.as_deref().unwrap_or("").starts_with("B2 (line 2: "), "{:?}", said.first);
        for r in rows {
            let want = if r % 3 == 2 { None } else { Some((r as f64 + 1.0) / 2.0) };
            assert_eq!(values[r], want, "row {r}");
        }
    }
}
