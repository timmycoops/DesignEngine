//! Table scripts (the Build Plan's "Tables and scripts plan", decisions 7 and 11, `docs/decisions/0099-tables-and-scripts.md`):
//! Starlark on a whole table, `def run(t, p):`, giving back a table. The script says what to do; the work is Rust's. A
//! table is a read-only handle: `t.where(...)`, `t.group(...)`, `t.join(...)`, `t.total(...)` and the rest each make a new
//! table in Rust, a group's numbers added up by each column's own rule as the grid adds them, so a 140,000-row table is
//! never walked a row at a time in Starlark. `table("name")` reads another table of the project; `new_table({...})` makes
//! one from lists. A script never moves material: it reads a reserves table's numbers and makes others from them.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::{Arc, Mutex};

use allocative::Allocative;
use starlark::any::ProvidesStaticType;
use starlark::environment::{GlobalsBuilder, Methods, MethodsBuilder};
use starlark::eval::Evaluator;
use starlark::starlark_simple_value;
use starlark::StarlarkPagablePanic;
use starlark::values::dict::{AllocDict, UnpackDictEntries};
use starlark::values::list::{AllocList, UnpackList};
use starlark::values::none::NoneOr;
use starlark::values::{Heap, NoSerialize, StarlarkValue, UnpackValue, Value, ValueLike};
use starlark_derive::{starlark_module, starlark_value};
use zd_core::{ColumnValues, Table, TableColumn};
use zd_reserves::AddUp;

/// What the engine does for a table script, in Rust: a number column added up over some rows by its own rule (as the grid
/// adds up a subtotal), and rows put in groups in the grid's order (a seam in the stack's order, B2 before B10).
pub trait TableWork: Send + Sync {
    /// A number column added up over `rows` by its rule; none where no row has a value or it has no rule.
    fn add_up(&self, table: &Table, column: &str, rows: &[usize]) -> Option<f64>;
    /// `rows` in groups by the words of the `by` columns, in the grid's order: each group's words and its rows.
    fn groups(&self, table: &Table, by: &[String], rows: &[usize]) -> Vec<(Vec<String>, Vec<usize>)>;
    /// Two words in the grid's order (natural: B2 before B10).
    fn order(&self, a: &str, b: &str) -> Ordering;
}

/// What a table script said as it ran: things it should know (a column with no rule left empty in its groups, a key
/// repeated in a join), shared by every table it made.
pub(crate) type Notes = Arc<Mutex<Vec<String>>>;

fn note(notes: &Notes, what: String) {
    let mut n = notes.lock().expect("notes");
    if !n.contains(&what) {
        n.push(what);
    }
}

/// The run a table script is part of, for `table()` and `new_table()`: the project's tables it may read by name and the
/// engine's work.
pub(crate) struct TableRun {
    pub work: Arc<dyn TableWork>,
    pub tables: Vec<(String, Arc<Table>)>,
    pub read: Mutex<Vec<String>>,
    pub notes: Notes,
    /// The checks it asked, `check()` and `required()`, worked out on the table it gives back.
    pub checks: Mutex<Vec<crate::checks::Asked>>,
}

/// A table as a script holds it: read only; every method makes a new one.
#[derive(ProvidesStaticType, NoSerialize, StarlarkPagablePanic, Allocative)]
pub(crate) struct TableHandle {
    pub name: String,
    #[allocative(skip)]
    pub table: Arc<Table>,
    #[allocative(skip)]
    pub work: Arc<dyn TableWork>,
    #[allocative(skip)]
    pub notes: Notes,
}
starlark_simple_value!(TableHandle);

impl fmt::Debug for TableHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for TableHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "table({}, {} rows, {} columns)", self.name, self.table.rows(), self.table.columns.len())
    }
}

impl TableHandle {
    fn made(&self, name: &str, table: Table) -> TableHandle {
        TableHandle { name: name.to_string(), table: Arc::new(table), work: self.work.clone(), notes: self.notes.clone() }
    }

    fn column(&self, name: &str) -> anyhow::Result<&TableColumn> {
        self.table.column(name).ok_or_else(|| anyhow::anyhow!("the table {} has no column {name}{}", self.name, did_you_mean(name, self.table.columns.iter().map(|c| c.name.as_str()))))
    }
}

fn did_you_mean<'a>(name: &str, names: impl Iterator<Item = &'a str>) -> String {
    let lower = name.to_lowercase();
    let near: Vec<&str> = names.filter(|n| {
        let n = n.to_lowercase();
        n.contains(&lower) || lower.contains(&n) || n.ends_with(&lower.rsplit('\\').next().unwrap_or("").to_string())
    }).take(3).collect();
    if near.is_empty() { String::new() } else { format!(" (did you mean {}?)", near.join(", ")) }
}

/// A cell as words, for a key: numbers as they print (1, not 1.0).
fn word_of(column: &TableColumn, row: usize) -> String {
    match &column.values {
        ColumnValues::Text(v) => v[row].clone().unwrap_or_default(),
        ColumnValues::Number(v) => v[row].map(|x| format!("{x}")).unwrap_or_default(),
    }
}

/// The table's rows `rows`, in that order.
fn take(table: &Table, rows: &[usize]) -> Table {
    let columns = table
        .columns
        .iter()
        .map(|c| TableColumn {
            values: match &c.values {
                ColumnValues::Number(v) => ColumnValues::Number(rows.iter().map(|&r| v[r]).collect()),
                ColumnValues::Text(v) => ColumnValues::Text(rows.iter().map(|&r| v[r].clone()).collect()),
            },
            ..c.clone()
        })
        .collect();
    Table { columns, calculated: Vec::new() }
}

/// A column's cell for Starlark.
fn cell<'v>(column: &TableColumn, row: usize, heap: Heap<'v>) -> Value<'v> {
    match &column.values {
        ColumnValues::Number(v) => v[row].map_or_else(Value::new_none, |x| heap.alloc(x)),
        ColumnValues::Text(v) => v[row].as_deref().filter(|w| !w.is_empty()).map_or_else(Value::new_none, |w| heap.alloc_str(w).to_value()),
    }
}

/// Names given as one word or a list.
fn names(v: Value<'_>, what: &str) -> anyhow::Result<Vec<String>> {
    if let Some(s) = v.unpack_str() {
        return Ok(vec![s.to_string()]);
    }
    let Ok(list) = UnpackList::<String>::unpack_value_err(v) else {
        anyhow::bail!("{what} is a column's name or a list of them, not {}", v.to_repr());
    };
    Ok(list.items)
}

/// A filter's words: `{"seam": "U, M", "Insitu\\Coal\\thickness": "<0.3"}`, a number as its words.
pub(crate) fn filter_words<'v>(pairs: impl IntoIterator<Item = (String, Value<'v>)>) -> anyhow::Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for (k, v) in pairs {
        let words = if let Some(s) = v.unpack_str() {
            s.to_string()
        } else if let Some(i) = v.unpack_i32() {
            i.to_string()
        } else if let Some(x) = v.downcast_ref::<starlark::values::float::StarlarkFloat>() {
            format!("{}", x.0)
        } else {
            anyhow::bail!("a filter on {k} is words, as the grid's filter takes them (\"U, M\", \"<0.3\", \"1-5\"), not {}", v.to_repr());
        };
        out.insert(k, words);
    }
    Ok(out)
}

/// How a column adds up, from words: `sum`, `max`, `min`, `volume` (averaged by the volume beside it), `mass of rd`
/// (averaged by volume × that density), `ratio of waste per coal` (a ratio of the two added up).
pub fn add_up_from_words(words: &str) -> Result<AddUp, String> {
    let w = words.trim();
    let lower = w.to_lowercase();
    let rest = |prefix: &str| lower.starts_with(prefix).then(|| w[prefix.len()..].trim().to_string());
    match lower.as_str() {
        "sum" | "add" | "added" => return Ok(AddUp::Sum),
        "max" | "largest" => return Ok(AddUp::Max),
        "min" | "smallest" => return Ok(AddUp::Min),
        "volume" | "by volume" => return Ok(AddUp::Volume),
        _ => {}
    }
    if let Some(density) = rest("by mass of ").or_else(|| rest("mass of ")).filter(|d| !d.is_empty()) {
        return Ok(AddUp::Mass { density });
    }
    if let Some(of_per) = rest("ratio of ") {
        let at = of_per.to_lowercase().find(" per ").ok_or_else(|| format!("{w}: a ratio is \"ratio of <column> per <column>\""))?;
        return Ok(AddUp::Ratio { of: of_per[..at].trim().to_string(), per: of_per[at + 5..].trim().to_string() });
    }
    Err(format!("{w} isn't a way to add up: sum, max, min, volume, \"mass of <density>\" or \"ratio of <column> per <column>\""))
}

/// Values from Starlark as a column: numbers (whole or not), or words; None is an empty cell.
fn column_from<'v>(name: &str, unit: &str, add_up: Option<AddUp>, items: &[Value<'v>]) -> anyhow::Result<TableColumn> {
    let words = items.iter().any(|v| v.unpack_str().is_some());
    if words {
        let mut out = Vec::with_capacity(items.len());
        for v in items {
            if v.is_none() {
                out.push(None);
            } else if let Some(s) = v.unpack_str() {
                out.push(Some(s.to_string()));
            } else {
                anyhow::bail!("the column {name} mixes words and {}: a column holds one or the other", v.to_repr());
            }
        }
        return Ok(TableColumn::text(name, out));
    }
    let mut out = Vec::with_capacity(items.len());
    for v in items {
        if v.is_none() {
            out.push(None);
        } else if let Some(i) = v.unpack_i32() {
            out.push(Some(f64::from(i)));
        } else if let Some(x) = v.downcast_ref::<starlark::values::float::StarlarkFloat>() {
            if !x.0.is_finite() {
                anyhow::bail!("the column {name} has {} in it (a division by zero?): give None where there's no value", x.0);
            }
            out.push(Some(x.0));
        } else {
            anyhow::bail!("the column {name} holds numbers or words, not {} ({})", v.get_type(), v.to_repr());
        }
    }
    let mut c = TableColumn::number(name, unit, out);
    c.add_up = add_up;
    Ok(c)
}

#[starlark_value(type = "table")]
impl<'v> StarlarkValue<'v> for TableHandle {
    fn get_methods() -> Option<&'static Methods> {
        starlark::methods_static!(TABLE_METHODS = table_methods);
        Some(TABLE_METHODS.methods())
    }

    fn length(&self) -> starlark::Result<i32> {
        Ok(i32::try_from(self.table.rows()).unwrap_or(i32::MAX))
    }

    /// `t["ROM\\Coal\\tonnes"]`: a column as a list, a cell with no value None.
    fn at(&self, index: Value<'v>, heap: Heap<'v>) -> starlark::Result<Value<'v>> {
        let Some(name) = index.unpack_str() else {
            return Err(starlark::Error::new_other(anyhow::anyhow!("a table's column is named in words, not {}; a row is t.row(i)", index.to_repr())));
        };
        let column = self.column(name).map_err(starlark::Error::new_other)?;
        Ok(heap.alloc(AllocList((0..self.table.rows()).map(|r| cell(column, r, heap)))))
    }

    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        match attribute {
            "rows" => Some(heap.alloc(i32::try_from(self.table.rows()).unwrap_or(i32::MAX))),
            "columns" => Some(heap.alloc(AllocList(self.table.columns.iter().map(|c| heap.alloc_str(&c.name).to_value())))),
            "name" => Some(heap.alloc_str(&self.name).to_value()),
            _ => None,
        }
    }

    fn has_attr(&self, attribute: &str, _heap: Heap<'v>) -> bool {
        matches!(attribute, "rows" | "columns" | "name")
    }

    fn dir_attr(&self) -> Vec<String> {
        ["rows", "columns", "name"].map(String::from).to_vec()
    }
}

#[starlark_module]
fn table_methods(builder: &mut MethodsBuilder) {
    /// The rows a filter keeps, in the grid's words: `t.where({"seam": "U, M"})`, `t.where(material="coal")`,
    /// `t.where({"Insitu\\Coal\\thickness": "<0.3"})`.
    fn r#where<'v>(this: &TableHandle, #[starlark(default = NoneOr::None)] filter: NoneOr<UnpackDictEntries<String, Value<'v>>>, #[starlark(kwargs)] kwargs: UnpackDictEntries<String, Value<'v>>) -> anyhow::Result<TableHandle> {
        let mut pairs = match filter {
            NoneOr::Other(f) => f.entries,
            NoneOr::None => Vec::new(),
        };
        pairs.extend(kwargs.entries);
        let when = filter_words(pairs)?;
        for key in when.keys() {
            if this.table.column(key).is_none() && !this.table.columns.iter().any(|c| c.name.eq_ignore_ascii_case(key)) {
                anyhow::bail!("the table {} has no column {key} to filter on{}", this.name, did_you_mean(key, this.table.columns.iter().map(|c| c.name.as_str())));
            }
        }
        let rows = zd_core::leaves::rows_fitting(&this.table, &when);
        Ok(this.made(&this.name, take(&this.table, &rows)))
    }

    /// A row a group of the `by` columns' words, in the grid's order: each number added up over the group by its own rule
    /// (tonnes summed, ash by tonnes, a ratio of sums), each other column of words where the group agrees.
    fn group<'v>(this: &TableHandle, by: Value<'v>) -> anyhow::Result<TableHandle> {
        let by = names(by, "group's by")?;
        for b in &by {
            match this.column(b)?.values {
                ColumnValues::Text(_) => {}
                ColumnValues::Number(_) => anyhow::bail!("{b} holds numbers: group by a column of words"),
            }
        }
        let all: Vec<usize> = (0..this.table.rows()).collect();
        let groups = this.work.groups(&this.table, &by, &all);
        let mut columns: Vec<TableColumn> = by.iter().enumerate().map(|(i, b)| TableColumn::text(b, groups.iter().map(|(w, _)| Some(w[i].clone()).filter(|w| !w.is_empty())).collect())).collect();
        for c in this.table.columns.iter().filter(|c| !by.contains(&c.name)) {
            let values = match &c.values {
                ColumnValues::Number(_) => {
                    let v: Vec<Option<f64>> = groups.iter().map(|(_, rows)| this.work.add_up(&this.table, &c.name, rows)).collect();
                    if c.add_up.is_none() && v.iter().all(Option::is_none) && !groups.is_empty() {
                        note(&this.notes, format!("{} has no rule to add up by, so it's empty in the groups: give it one, t.with_column(name, values, adds=\"sum\")", c.name));
                    }
                    ColumnValues::Number(v)
                }
                ColumnValues::Text(v) => ColumnValues::Text(
                    groups
                        .iter()
                        .map(|(_, rows)| {
                            let mut words = rows.iter().map(|&r| v[r].as_deref().unwrap_or(""));
                            let first = words.next().unwrap_or("");
                            (!first.is_empty() && words.all(|w| w == first)).then(|| first.to_string())
                        })
                        .collect(),
                ),
            };
            columns.push(TableColumn { values, ..c.clone() });
        }
        Ok(this.made(&this.name, Table { columns, calculated: Vec::new() }))
    }

    /// A number column added up over every row by its own rule; None where no row has a value.
    fn total(this: &TableHandle, column: &str) -> anyhow::Result<NoneOr<f64>> {
        match this.column(column)?.values {
            ColumnValues::Number(_) => {}
            ColumnValues::Text(_) => anyhow::bail!("{column} holds words: only numbers add up"),
        }
        let all: Vec<usize> = (0..this.table.rows()).collect();
        Ok(match this.work.add_up(&this.table, column, &all) {
            Some(x) => NoneOr::Other(x),
            None => NoneOr::None,
        })
    }

    /// Each row of this table with the other's columns beside it, matched on the `on` columns' words: the first of the
    /// other's rows with the same key (a key it repeats is said). `prefix` goes on the other's column names; `how="outer"`
    /// adds the other's rows nothing here matched, at the end.
    fn join<'v>(
        this: &TableHandle,
        other: Value<'v>,
        #[starlark(require = named)] on: Value<'v>,
        #[starlark(require = named, default = "")] prefix: &str,
        #[starlark(require = named, default = "left")] how: &str,
    ) -> anyhow::Result<TableHandle> {
        let Some(other) = other.downcast_ref::<TableHandle>() else {
            anyhow::bail!("join takes a table, from table(\"name\") or new_table(...), not {}", other.to_repr());
        };
        let on = names(on, "join's on")?;
        let outer = match how {
            "left" => false,
            "outer" => true,
            _ => anyhow::bail!("how is \"left\" (this table's rows) or \"outer\" (the other's unmatched rows too), not {how}"),
        };
        let mine: Vec<&TableColumn> = on.iter().map(|k| this.column(k)).collect::<anyhow::Result<_>>()?;
        let theirs: Vec<&TableColumn> = on.iter().map(|k| other.column(k)).collect::<anyhow::Result<_>>()?;
        let key = |cols: &[&TableColumn], r: usize| cols.iter().map(|c| word_of(c, r)).collect::<Vec<_>>();
        let mut at: HashMap<Vec<String>, usize> = HashMap::new();
        let mut repeated = 0;
        for r in 0..other.table.rows() {
            match at.entry(key(&theirs, r)) {
                std::collections::hash_map::Entry::Occupied(_) => repeated += 1,
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(r);
                }
            }
        }
        if repeated > 0 {
            note(&this.notes, format!("{} has {repeated} row{} whose {} repeat an earlier row's: the first was joined", other.name, if repeated == 1 { "" } else { "s" }, on.join(", ")));
        }
        let matched: Vec<Option<usize>> = (0..this.table.rows()).map(|r| at.get(&key(&mine, r)).copied()).collect();
        let used: std::collections::HashSet<usize> = matched.iter().flatten().copied().collect();
        let extra: Vec<usize> = if outer { (0..other.table.rows()).filter(|r| !used.contains(r) && at.get(&key(&theirs, *r)) == Some(r)).collect() } else { Vec::new() };
        let added: Vec<&TableColumn> = other.table.columns.iter().filter(|c| !on.contains(&c.name)).collect();
        for c in &added {
            let name = format!("{prefix}{}", c.name);
            if this.table.column(&name).is_some() {
                anyhow::bail!("both tables have a column {name}: give the join a prefix, prefix=\"{} \"", other.name);
            }
        }
        let mut columns = Vec::with_capacity(this.table.columns.len() + added.len());
        for c in &this.table.columns {
            // A key column of the other's unmatched rows takes their words; any other of this table's is empty there.
            let other_key = on.iter().position(|k| *k == c.name).map(|i| theirs[i]);
            let values = match &c.values {
                ColumnValues::Number(v) => ColumnValues::Number(v.iter().copied().chain(extra.iter().map(|&r| other_key.and_then(|k| match &k.values {
                    ColumnValues::Number(o) => o[r],
                    ColumnValues::Text(o) => o[r].as_deref().and_then(|w| w.parse().ok()),
                }))).collect()),
                ColumnValues::Text(v) => ColumnValues::Text(v.iter().cloned().chain(extra.iter().map(|&r| other_key.map(|k| word_of(k, r)).filter(|w| !w.is_empty()))).collect()),
            };
            columns.push(TableColumn { values, ..(*c).clone() });
        }
        for c in added {
            let rows: Vec<Option<usize>> = matched.iter().copied().chain(extra.iter().map(|&r| Some(r))).collect();
            let values = match &c.values {
                ColumnValues::Number(v) => ColumnValues::Number(rows.iter().map(|r| r.and_then(|r| v[r])).collect()),
                ColumnValues::Text(v) => ColumnValues::Text(rows.iter().map(|r| r.and_then(|r| v[r].clone())).collect()),
            };
            // A rule naming columns beside it names them with the prefix too.
            let add_up = c.add_up.clone().map(|a| match a {
                AddUp::Mass { density } if !prefix.is_empty() && !density.contains('\\') => AddUp::Mass { density: format!("{prefix}{density}") },
                AddUp::Ratio { of, per } if !prefix.is_empty() => AddUp::Ratio { of: format!("{prefix}{of}"), per: format!("{prefix}{per}") },
                other => other,
            });
            columns.push(TableColumn { name: format!("{prefix}{}", c.name), add_up, values, ..c.clone() });
        }
        let unmatched = matched.iter().filter(|m| m.is_none()).count();
        if unmatched > 0 {
            note(&this.notes, format!("{unmatched} of {}'s {} rows had no row in {} with the same {}", this.name, this.table.rows(), other.name, on.join(", ")));
        }
        Ok(this.made(&this.name, Table { columns, calculated: Vec::new() }))
    }

    /// Only these columns, in this order.
    fn select<'v>(this: &TableHandle, columns: Value<'v>) -> anyhow::Result<TableHandle> {
        let wanted = names(columns, "select's columns")?;
        let columns = wanted.iter().map(|n| this.column(n).cloned()).collect::<anyhow::Result<Vec<_>>>()?;
        Ok(this.made(&this.name, Table { columns, calculated: Vec::new() }))
    }

    /// Every column but these.
    fn drop<'v>(this: &TableHandle, columns: Value<'v>) -> anyhow::Result<TableHandle> {
        let gone = names(columns, "drop's columns")?;
        for n in &gone {
            this.column(n)?;
        }
        let columns = this.table.columns.iter().filter(|c| !gone.contains(&c.name)).cloned().collect();
        Ok(this.made(&this.name, Table { columns, calculated: Vec::new() }))
    }

    /// Columns renamed, `{"old": "new"}`.
    fn rename<'v>(this: &TableHandle, names: UnpackDictEntries<String, String>) -> anyhow::Result<TableHandle> {
        let mut table = (*this.table).clone();
        table.calculated.clear();
        for (old, new) in names.entries {
            this.column(&old)?;
            if old != new && table.column(&new).is_some() {
                anyhow::bail!("the table {} has a column {new} already", this.name);
            }
            if let Some(c) = table.columns.iter_mut().find(|c| c.name == old) {
                c.name = new;
            }
        }
        Ok(this.made(&this.name, table))
    }

    /// The table with a column added, or one of its own put in its place: a value a row, None where it has none. Numbers
    /// take a unit and a rule to add up by (`adds="sum"`, `"max"`, `"min"`, `"volume"`, `"mass of rd"`,
    /// `"ratio of waste per coal"`); with none, a group leaves it empty.
    fn with_column<'v>(
        this: &TableHandle,
        name: &str,
        values: UnpackList<Value<'v>>,
        #[starlark(require = named, default = "")] unit: &str,
        #[starlark(require = named, default = NoneOr::None)] adds: NoneOr<&str>,
    ) -> anyhow::Result<TableHandle> {
        if values.items.len() != this.table.rows() {
            anyhow::bail!("the column {name} has {} values for the table's {} rows", values.items.len(), this.table.rows());
        }
        let add_up = match adds {
            NoneOr::Other(w) => Some(add_up_from_words(w).map_err(|e| anyhow::anyhow!(e))?),
            NoneOr::None => None,
        };
        let column = column_from(name, unit, add_up, &values.items)?;
        let mut table = (*this.table).clone();
        table.calculated.clear();
        match table.columns.iter_mut().find(|c| c.name == name) {
            Some(c) => *c = column,
            None => table.columns.push(column),
        }
        Ok(this.made(&this.name, table))
    }

    /// The rows in a column's order, words in the grid's (B2 before B10), an empty cell last.
    fn sort(this: &TableHandle, column: &str, #[starlark(require = named, default = false)] descending: bool) -> anyhow::Result<TableHandle> {
        let c = this.column(column)?;
        let mut rows: Vec<usize> = (0..this.table.rows()).collect();
        rows.sort_by(|&a, &b| {
            let o = match &c.values {
                ColumnValues::Number(v) => match (v[a], v[b]) {
                    (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
                    (Some(_), None) => return Ordering::Less,
                    (None, Some(_)) => return Ordering::Greater,
                    (None, None) => Ordering::Equal,
                },
                ColumnValues::Text(v) => match (v[a].as_deref().filter(|w| !w.is_empty()), v[b].as_deref().filter(|w| !w.is_empty())) {
                    (Some(x), Some(y)) => this.work.order(x, y),
                    (Some(_), None) => return Ordering::Less,
                    (None, Some(_)) => return Ordering::Greater,
                    (None, None) => Ordering::Equal,
                },
            };
            if descending { o.reverse() } else { o }
        });
        Ok(this.made(&this.name, take(&this.table, &rows)))
    }

    /// One row as a dict of its columns, from 0.
    fn row<'v>(this: &TableHandle, i: i32, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let rows = this.table.rows();
        let r = usize::try_from(i).ok().filter(|&r| r < rows).ok_or_else(|| anyhow::anyhow!("row {i}: the table {} has rows 0 to {}", this.name, rows.saturating_sub(1)))?;
        Ok(heap.alloc(AllocDict(this.table.columns.iter().map(|c| (c.name.as_str(), cell(c, r, heap))))))
    }

    /// A column's unit, "" where it has none.
    fn unit(this: &TableHandle, column: &str) -> anyhow::Result<String> {
        Ok(this.column(column)?.unit.clone())
    }
}

fn run_of<'a>(eval: &'a Evaluator<'_, '_, '_>) -> anyhow::Result<&'a TableRun> {
    crate::lookups::lookups_of(eval).and_then(|r| r.tables.as_ref()).ok_or_else(|| anyhow::anyhow!("tables are read only by a table script, `def run(t, p):`"))
}

/// The table functions every script can call; only a table script has tables to give them.
#[starlark_module]
pub(crate) fn table_globals(builder: &mut GlobalsBuilder) {
    /// Another table of the project, by its name: a reserves table, one imported, or one a script made.
    fn table(name: &str, eval: &mut Evaluator) -> anyhow::Result<TableHandle> {
        let run = run_of(eval)?;
        let Some((n, t)) = run.tables.iter().find(|(n, _)| n == name).or_else(|| run.tables.iter().find(|(n, _)| n.eq_ignore_ascii_case(name))) else {
            anyhow::bail!("the project has no table named {name}{}", did_you_mean(name, run.tables.iter().map(|(n, _)| n.as_str())));
        };
        let mut read = run.read.lock().expect("read");
        if !read.contains(n) {
            read.push(n.clone());
        }
        Ok(TableHandle { name: n.clone(), table: t.clone(), work: run.work.clone(), notes: run.notes.clone() })
    }

    /// A table made from lists, a column each, all as long: `new_table({"strip": strips, "tonnes": tonnes},
    /// units={"tonnes": "t"}, adds={"tonnes": "sum"})`.
    fn new_table<'v>(
        columns: UnpackDictEntries<String, UnpackList<Value<'v>>>,
        #[starlark(require = named, default = NoneOr::None)] units: NoneOr<UnpackDictEntries<String, String>>,
        #[starlark(require = named, default = NoneOr::None)] adds: NoneOr<UnpackDictEntries<String, String>>,
        #[starlark(require = named, default = "new table")] name: &str,
        eval: &mut Evaluator,
    ) -> anyhow::Result<TableHandle> {
        let run = run_of(eval)?;
        let units: BTreeMap<String, String> = match units {
            NoneOr::Other(u) => u.entries.into_iter().collect(),
            NoneOr::None => BTreeMap::new(),
        };
        let adds: BTreeMap<String, String> = match adds {
            NoneOr::Other(a) => a.entries.into_iter().collect(),
            NoneOr::None => BTreeMap::new(),
        };
        let mut out = Vec::new();
        let mut rows: Option<usize> = None;
        for (n, values) in columns.entries {
            if rows.is_some_and(|r| r != values.items.len()) {
                anyhow::bail!("the column {n} has {} values, but the first had {}", values.items.len(), rows.unwrap_or(0));
            }
            rows = Some(values.items.len());
            let add_up = adds.get(&n).map(|w| add_up_from_words(w)).transpose().map_err(|e| anyhow::anyhow!(e))?;
            out.push(column_from(&n, units.get(&n).map_or("", String::as_str), add_up, &values.items)?);
        }
        for n in units.keys().chain(adds.keys()) {
            if !out.iter().any(|c| c.name == *n) {
                anyhow::bail!("units or adds name {n}, which isn't one of the new table's columns");
            }
        }
        Ok(TableHandle { name: name.to_string(), table: Arc::new(Table { columns: out, calculated: Vec::new() }), work: run.work.clone(), notes: run.notes.clone() })
    }
}
