//! Each mine's own checks in a table script (the Build Plan's "Schedule quantities plan", decision 3,
//! `docs/decisions/0101-schedule-quantities.md`): the stages are built in and balanced by the engine (0086); which process
//! splits a mine has, and what they add back to, are the mine's own script. Two helpers make every site's checks read the
//! same: `check(name, parts…, equals=, tolerance=)` and `required(column, material=)`. Each is asked as the script runs and
//! worked out on the table it gives back, a column a check under `Summary\Checks\<name>`, every leaf out named by its
//! levels.

use std::collections::BTreeMap;

use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::list::UnpackList;
use starlark::values::none::{NoneOr, NoneType};
use starlark::values::tuple::UnpackTuple;
use starlark::values::dict::UnpackDictEntries;
use starlark::values::{UnpackValue, Value, ValueLike};
use starlark_derive::starlark_module;
use zd_core::{ColumnValues, Table, TableColumn};
use zd_reserves::AddUp;

/// The folder every check's columns go under.
pub const CHECKS: &str = "Summary\\Checks";

/// A part of a check: a column of the table given back, by name, or a value a row the script worked out.
#[derive(Debug, Clone)]
pub(crate) enum Part {
    Column(String),
    Values(Vec<Option<f64>>),
}

/// A check asked as the script ran, worked out when it gives its table back.
#[derive(Debug, Clone)]
pub(crate) enum Asked {
    /// The parts added up on every leaf against `equals`.
    AddsTo { name: String, parts: Vec<Part>, equals: Part, tolerance: f64 },
    /// Every leaf the filter keeps (and of the material, where one is named) has a value in `column`.
    Required { name: String, column: String, material: Option<String>, filter: BTreeMap<String, String> },
}

/// A check's part from Starlark: a column's name, or a list of numbers (None where a row has none).
fn part_of(v: Value<'_>, what: &str) -> anyhow::Result<Part> {
    if let Some(s) = v.unpack_str() {
        return Ok(Part::Column(s.to_string()));
    }
    let Ok(list) = UnpackList::<Value>::unpack_value_err(v) else {
        anyhow::bail!("{what} is a column's name (\"ROM\\\\Waste\\\\volume\") or its values (t[\"…\"]), not {}", v.to_repr());
    };
    list.items.iter().map(|x| number(*x).map_err(|why| anyhow::anyhow!("{what}: {why}"))).collect::<anyhow::Result<_>>().map(Part::Values)
}

fn number(v: Value<'_>) -> anyhow::Result<Option<f64>> {
    if v.is_none() {
        Ok(None)
    } else if let Some(i) = v.unpack_i32() {
        Ok(Some(f64::from(i)))
    } else if let Some(x) = v.downcast_ref::<starlark::values::float::StarlarkFloat>() {
        Ok(Some(x.0).filter(|x| x.is_finite()))
    } else {
        anyhow::bail!("a check adds numbers, not {}", v.to_repr())
    }
}

/// A check's name, which becomes a folder: words, no `\`.
fn check_name(name: &str) -> anyhow::Result<String> {
    let n = name.trim();
    if n.is_empty() || n.contains('\\') {
        anyhow::bail!("a check's name is a few words with no \\ in it (it's the folder under {CHECKS}), not {name:?}");
    }
    Ok(n.to_string())
}

fn asked<'a>(eval: &'a Evaluator<'_, '_, '_>, call: &str) -> anyhow::Result<&'a std::sync::Mutex<Vec<Asked>>> {
    crate::lookups::lookups_of(eval)
        .and_then(|r| r.tables.as_ref())
        .map(|t| &t.checks)
        .ok_or_else(|| anyhow::anyhow!("{call} checks a whole table, so it's called in a table script, `def run(t, p):`"))
}

/// The checks every table script can call.
#[starlark_module]
pub(crate) fn check_globals(builder: &mut GlobalsBuilder) {
    /// Checks that the parts add up to `equals` on every leaf of the table the script gives back:
    /// `check("waste processes", t["ROM\\Waste\\dragline_bcm"], t["ROM\\Waste\\truck_bcm"], equals=t["ROM\\Waste\\volume"],
    /// tolerance=0.01)`. Each part is a column's values or its name; a leaf with no value counts it as 0. Writes the
    /// difference (parts less `equals`) to `Summary\Checks\<name>\difference` and 1 where it's out by more than the
    /// tolerance to `…\leaves out`; the run names each leaf out.
    fn check<'v>(
        name: &str,
        #[starlark(args)] parts: UnpackTuple<Value<'v>>,
        #[starlark(require = named)] equals: Value<'v>,
        #[starlark(require = named, default = NoneOr::None)] tolerance: NoneOr<Value<'v>>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let name = check_name(name)?;
        if parts.items.is_empty() {
            anyhow::bail!("check {name} has no parts to add up: give them before equals=, check(\"{name}\", t[\"…\"], t[\"…\"], equals=t[\"…\"])");
        }
        let parts = parts.items.iter().enumerate().map(|(i, v)| part_of(*v, &format!("check {name}'s part {}", i + 1))).collect::<anyhow::Result<Vec<_>>>()?;
        let equals = part_of(equals, &format!("check {name}'s equals"))?;
        let tolerance = match tolerance {
            NoneOr::None => 0.0,
            NoneOr::Other(v) => number(v)?.filter(|t| *t >= 0.0).ok_or_else(|| anyhow::anyhow!("check {name}'s tolerance is a number, 0 or more, not {}", v.to_repr()))?,
        };
        let checks = asked(eval, "check")?;
        let mut c = checks.lock().expect("checks");
        if c.iter().any(|a| matches!(a, Asked::AddsTo { name: n, .. } | Asked::Required { name: n, .. } if *n == name)) {
            anyhow::bail!("there's a check named {name} already: give each its own name");
        }
        c.push(Asked::AddsTo { name, parts, equals, tolerance });
        Ok(NoneType)
    }

    /// Checks that every leaf of the table the script gives back has a value in `column`: of one material only with
    /// `material="waste"` (a leaf holding waste: its stage's `Waste\volume` above 0, or its `material` word), and only on
    /// the leaves a filter keeps, in the grid's words (`seam="U, M"`). Writes 1 to `Summary\Checks\<name>\leaves missing`
    /// on each leaf with none, so the Summary counts them; the run names each one.
    fn required<'v>(
        column: &str,
        #[starlark(require = named, default = NoneOr::None)] material: NoneOr<&str>,
        #[starlark(require = named, default = NoneOr::None)] name: NoneOr<&str>,
        #[starlark(kwargs)] filter: UnpackDictEntries<String, Value<'v>>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let material = material.into_option().map(|m| m.trim().to_string()).filter(|m| !m.is_empty());
        let short = column.rsplit('\\').next().unwrap_or(column);
        let name = check_name(&name.into_option().map_or_else(|| match &material {
            Some(m) => format!("{short} on {m}"),
            None => short.to_string(),
        }, str::to_string))?;
        let filter = crate::tables::filter_words(filter.entries)?;
        let checks = asked(eval, "required")?;
        let mut c = checks.lock().expect("checks");
        if c.iter().any(|a| matches!(a, Asked::AddsTo { name: n, .. } | Asked::Required { name: n, .. } if *n == name)) {
            anyhow::bail!("there's a check named {name} already: give this one its own, required(\"{column}\", name=\"…\")");
        }
        c.push(Asked::Required { name, column: column.to_string(), material, filter });
        Ok(NoneType)
    }
}

/// A leaf as a planner reads it: its levels' words (`Pit 3 B1 U`), else its row.
fn leaf(levels: &[&TableColumn], row: usize) -> String {
    let words: Vec<&str> = levels
        .iter()
        .filter_map(|c| match &c.values {
            ColumnValues::Text(v) => v[row].as_deref().filter(|w| !w.is_empty()),
            ColumnValues::Number(_) => None,
        })
        .collect();
    if words.is_empty() { format!("row {}", row + 1) } else { words.join(" ") }
}

/// "leaf" or "leaves".
fn leaves(n: usize) -> &'static str {
    if n == 1 { "leaf" } else { "leaves" }
}

/// The leaves named, the first ten and how many more.
fn leaves_named(names: &[String]) -> String {
    let shown = names.iter().take(10).cloned().collect::<Vec<_>>().join(", ");
    if names.len() > 10 { format!("{shown} and {} more", names.len() - 10) } else { shown }
}

fn values_of(table: &Table, part: &Part, what: &str) -> Result<(Vec<Option<f64>>, String), String> {
    match part {
        Part::Values(v) if v.len() == table.rows() => Ok((v.clone(), String::new())),
        Part::Values(v) => Err(format!("{what} has {} values for the {} rows of the table run gives back: a check is worked out on that table", v.len(), table.rows())),
        Part::Column(n) => match table.column(n) {
            Some(TableColumn { values: ColumnValues::Number(v), unit, .. }) => Ok((v.clone(), unit.clone())),
            Some(_) => Err(format!("{what}, {n}, holds words: a check adds numbers")),
            None => Err(format!("{what}: the table run gives back has no column {n}")),
        },
    }
}

/// Whether a leaf holds `material`: its own `material` word where the table has one; else its stage's
/// `<stage>\<Material>\volume` above 0, the stage the column's first folder, or ROM where it has none (what's scheduled).
fn of_material(table: &Table, column: &str, material: &str) -> Result<Vec<bool>, String> {
    if let Some(TableColumn { values: ColumnValues::Text(v), .. }) = table.columns.iter().find(|c| c.name.eq_ignore_ascii_case("material")) {
        return Ok(v.iter().map(|w| w.as_deref().is_some_and(|w| w.eq_ignore_ascii_case(material))).collect());
    }
    let stage = column.split_once('\\').map_or("ROM", |(s, _)| s);
    let want = format!("{stage}\\{material}\\volume");
    let Some(TableColumn { values: ColumnValues::Number(volumes), .. }) = table.columns.iter().find(|c| c.name.eq_ignore_ascii_case(&want)) else {
        return Err(format!("the table has no material column, nor a {want} above 0, to tell a leaf of {material} by"));
    };
    Ok(volumes.iter().map(|v| v.is_some_and(|x| x > 0.0)).collect())
}

/// The checks asked, worked out on the table given back: their columns put on it, under `Summary\Checks`, and what each
/// found out of line, a line a check. Refused, with why, where a check can't be worked out on that table.
pub(crate) fn work_out(table: &mut Table, asked: &[Asked]) -> Result<Vec<String>, String> {
    let levels: Vec<TableColumn> = zd_core::leaves::levels(table).iter().filter_map(|l| table.column(l).cloned()).collect();
    let levels: Vec<&TableColumn> = levels.iter().collect();
    let mut out = Vec::new();
    let mut columns = Vec::new();
    for a in asked {
        match a {
            Asked::AddsTo { name, parts, equals, tolerance } => {
                let (equals, unit) = values_of(table, equals, &format!("check {name}'s equals"))?;
                let mut unit = unit;
                let mut sums: Vec<Option<f64>> = equals.iter().map(|e| e.map(|x| -x)).collect();
                for (i, part) in parts.iter().enumerate() {
                    let (values, u) = values_of(table, part, &format!("check {name}'s part {}", i + 1))?;
                    if unit.is_empty() {
                        unit = u;
                    }
                    for (s, v) in sums.iter_mut().zip(values) {
                        if let Some(v) = v {
                            *s = Some(s.unwrap_or(0.0) + v);
                        }
                    }
                }
                let out_rows: Vec<usize> = (0..table.rows()).filter(|&r| sums[r].is_some_and(|d| d.abs() > *tolerance)).collect();
                let flags = (0..table.rows()).map(|r| Some(if sums[r].is_some_and(|d| d.abs() > *tolerance) { 1.0 } else { 0.0 })).collect();
                if !out_rows.is_empty() {
                    let named: Vec<String> = out_rows.iter().map(|&r| format!("{} by {}", leaf(&levels, r), shown(sums[r].unwrap_or(0.0)))).collect();
                    out.push(format!("check {name}: {} {} out by more than {}: {}", out_rows.len(), leaves(out_rows.len()), shown(*tolerance), leaves_named(&named)));
                }
                columns.push(TableColumn::adding(&format!("{CHECKS}\\{name}\\difference"), &unit, AddUp::Sum, sums));
                columns.push(TableColumn::adding(&format!("{CHECKS}\\{name}\\leaves out"), "", AddUp::Sum, flags));
            }
            Asked::Required { name, column, material, filter } => {
                let values = match table.column(column).map(|c| &c.values) {
                    Some(ColumnValues::Number(v)) => v.iter().map(Option::is_some).collect::<Vec<bool>>(),
                    Some(ColumnValues::Text(v)) => v.iter().map(|w| w.as_deref().is_some_and(|w| !w.is_empty())).collect(),
                    None => vec![false; table.rows()],
                };
                let kept = zd_core::leaves::rows_fitting(table, filter);
                let of = match material {
                    Some(m) => of_material(table, column, m).map_err(|why| format!("required {name}: {why}"))?,
                    None => vec![true; table.rows()],
                };
                let missing: Vec<usize> = kept.into_iter().filter(|&r| of[r] && !values[r]).collect();
                if !missing.is_empty() {
                    let named: Vec<String> = missing.iter().map(|&r| leaf(&levels, r)).collect();
                    let of = material.as_ref().map_or(String::new(), |m| format!("{m} "));
                    // A field no leaf has a value for has no column at all, so it's missing everywhere; said, as it may be misspelt.
                    let absent = if table.column(column).is_some() {
                        String::new()
                    } else {
                        let near = crate::check::did_you_mean(column, table.columns.iter().map(|c| c.name.as_str()));
                        let near = near.trim().trim_start_matches('(').trim_end_matches(')');
                        format!(" (the table has no column {column}{}{near})", if near.is_empty() { "" } else { "; " })
                    };
                    out.push(format!("required {name}: {} {of}{} with no {column}{absent}: {}", missing.len(), leaves(missing.len()), leaves_named(&named)));
                }
                let mut flags = vec![Some(0.0); table.rows()];
                for &r in &missing {
                    flags[r] = Some(1.0);
                }
                columns.push(TableColumn::adding(&format!("{CHECKS}\\{name}\\leaves missing"), "", AddUp::Sum, flags));
            }
        }
    }
    for c in columns {
        match table.columns.iter_mut().find(|m| m.name == c.name) {
            Some(m) => *m = c,
            None => table.columns.push(c),
        }
    }
    Ok(out)
}

/// A number as a check says it: to 6 significant places, no trailing zeros.
fn shown(x: f64) -> String {
    let s = format!("{:.6}", x);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}
