//! Any table as a grid (the Build Plan's "Tables and scripts plan", decision 1): a row a leaf, a column a field, grouped
//! by any of its word columns with a subtotal row a group, each added up by the column's own rule. A reserves table and
//! the blocks' attributes are the same concept here: the blocks become a [`Table`] (a row a solid) by [`objects_table`],
//! and both go through [`grid`].
//!
//! Every subtotal is [`add_up_column`] over the group's leaves, the add-up the Breakdown and the report use, so a strip
//! here is the Breakdown's strip node to the last bit.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Body, ColumnValues, ObjectId, Project, Table, TableColumn};
use zd_reserves::AddUp;

use crate::reserves_view::{add_up_column, children_of, levels, natural, rows_fitting};
use crate::table_calc::{self, Calc, ParameterLookup};

/// The most leaf rows a grid gives; past that the rest are counted in [`Grid::cut`], and a group is opened to see them.
pub const GRID_LEAVES: usize = 5_000;

/// What the grid is asked for.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct GridAsk {
    /// The leaves added up: the filter's words (decision 0088), none for every leaf.
    #[serde(default)]
    pub when: BTreeMap<String, String>,
    /// Word columns to group by, outermost first.
    #[serde(default)]
    pub group: Vec<String>,
    /// The groups opened, each by its words down to it; a group not here shows only its subtotal.
    #[serde(default)]
    pub open: Vec<Vec<String>>,
    /// The columns shown, in this order; none for every column.
    #[serde(default)]
    pub columns: Vec<String>,
    /// A column to sort each group's rows by; the groups' own order (a seam's stack, a bench's RL, natural) without.
    #[serde(default)]
    pub sort: Option<GridSort>,
    /// The most leaf rows given; [`GRID_LEAVES`] when none.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct GridSort {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
}

/// A column as the grid shows it: its full name (a folder path for a reserves field), its last part, its folders, unit,
/// whether it holds words, and how it adds up.
#[derive(Clone, Debug, Serialize)]
pub struct GridColumn {
    pub name: String,
    pub label: String,
    pub folders: Vec<String>,
    pub unit: String,
    pub text: bool,
    pub add_up: Option<AddUp>,
    /// Written by a run, entered by a person, or calculated (decisions 2 and 6): only an entered column's cells are typed in.
    pub kind: zd_core::ColumnKind,
    /// A calculated column's script and how it adds up, for its editor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calculated: Option<zd_core::CalculatedColumn>,
}

/// A cell: a number or words.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Cell {
    Number(f64),
    Text(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKind {
    /// Every leaf the filter keeps.
    Total,
    /// A group's subtotal.
    Group,
    /// One row of the table.
    Leaf,
}

/// One row of the grid.
#[derive(Clone, Debug, Serialize)]
pub struct GridRow {
    pub kind: RowKind,
    /// The group words down to it: a group's own, a leaf's group's.
    pub path: Vec<String>,
    pub label: String,
    /// 0 for the total, 1 for the outermost groups, and one deeper a level; a leaf one under its group.
    pub depth: usize,
    /// The leaves under it (1 for a leaf).
    pub leaves: usize,
    pub open: bool,
    /// A leaf's row in the table.
    pub row: Option<usize>,
    /// A leaf's words for the table's levels, top down, as the Breakdown's tree names it.
    pub words: Option<Vec<String>>,
    /// A cell a shown column: a group's added up by the column's rule, words where every leaf has the same.
    pub cells: Vec<Option<Cell>>,
}

/// A table as a grid.
#[derive(Clone, Debug, Serialize)]
pub struct Grid {
    /// The columns shown.
    pub columns: Vec<GridColumn>,
    /// Every column the table has, to pick from.
    pub every: Vec<GridColumn>,
    /// The word columns it can be grouped by.
    pub groups: Vec<String>,
    /// The table's levels, top down, as `words` gives them.
    pub levels: Vec<String>,
    pub rows: Vec<GridRow>,
    /// The leaves the filter keeps.
    pub leaves: usize,
    /// Leaf rows under open groups left out past the limit.
    pub cut: usize,
    /// What working out its calculated columns said: a formula that doesn't compile, rows it stopped on.
    pub said: Vec<String>,
}

fn grid_column(c: &TableColumn) -> GridColumn {
    let mut parts: Vec<String> = c.name.split('\\').map(str::to_string).collect();
    let label = parts.pop().unwrap_or_default();
    GridColumn { name: c.name.clone(), label, folders: parts, unit: c.unit.clone(), text: matches!(c.values, ColumnValues::Text(_)), add_up: c.add_up.clone(), kind: c.kind, calculated: None }
}

fn calc_column(c: &zd_core::CalculatedColumn) -> GridColumn {
    let mut parts: Vec<String> = c.name.split('\\').map(str::to_string).collect();
    let label = parts.pop().unwrap_or_default();
    GridColumn { name: c.name.clone(), label, folders: parts, unit: c.unit.clone(), text: false, add_up: None, kind: zd_core::ColumnKind::Calculated, calculated: Some(c.clone()) }
}

/// The table as a grid: a total row, a subtotal a group of each `group` column under its parent, and the leaves under
/// each innermost group that's open (under the total when there's no group), past the limit counted, not given.
pub fn grid(table: &Table, ask: &GridAsk) -> Result<Grid, String> {
    grid_with(table, ask, &|_| None)
}

/// [`grid`] with the table's calculated columns worked out (decision 6), their parameter tables found by `lookup`. Each
/// calculated column shown is worked on every leaf the filter keeps, a group's value its own rule's.
pub fn grid_with(table: &Table, ask: &GridAsk, lookup: ParameterLookup<'_>) -> Result<Grid, String> {
    table.check()?;
    let fitting = rows_fitting(table, &ask.when);
    let levels = levels(table);
    let mut every: Vec<GridColumn> = table.columns.iter().map(grid_column).collect();
    every.extend(table.calculated.iter().map(calc_column));
    let groups: Vec<String> = every.iter().filter(|c| c.text && !c.name.contains('\\')).map(|c| c.name.clone()).collect();
    // The calculated columns asked for, shown or sorted by, worked on the leaves kept; one that can't is said, empty.
    let wanted = |n: &str| ask.columns.is_empty() || ask.columns.iter().any(|c| c == n) || ask.sort.as_ref().is_some_and(|s| s.column == n);
    let label = |r: usize| {
        let words: Vec<String> = levels.iter().map(|l| word_at(table, l, r)).filter(|w| !w.is_empty()).collect();
        if words.is_empty() { format!("row {}", r + 1) } else { words.join(" ") }
    };
    let mut virtuals: Vec<TableColumn> = Vec::new();
    let mut calcs: Vec<(String, Calc<'_>, Vec<Option<f64>>)> = Vec::new();
    let mut said = Vec::new();
    for c in table.calculated.iter().filter(|c| wanted(&c.name)) {
        match table_calc::compile(table, c, lookup) {
            Ok(calc) => {
                let (values, stops) = table_calc::leaf_values(table, &calc, &fitting, &label);
                if stops.stopped > 0 {
                    said.push(format!("{}: no value on {} row{}, {}", c.name, stops.stopped, if stops.stopped == 1 { "" } else { "s" }, stops.first.unwrap_or_default()));
                }
                virtuals.push(TableColumn::number(&c.name, &c.unit, values.clone()));
                calcs.push((c.name.clone(), calc, values));
            }
            Err(problems) => {
                said.push(format!("{}: {}", c.name, problems.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("; ")));
                virtuals.push(TableColumn::number(&c.name, &c.unit, vec![None; table.rows()]));
            }
        }
    }
    let find = |n: &str| table.column(n).or_else(|| virtuals.iter().find(|v| v.name == n));
    let shown: Vec<&TableColumn> = if ask.columns.is_empty() {
        table.columns.iter().chain(virtuals.iter()).collect()
    } else {
        ask.columns.iter().map(|n| find(n).ok_or_else(|| format!("the table has no column {n}"))).collect::<Result<_, _>>()?
    };
    let mut group_words = Vec::new();
    for g in &ask.group {
        match table.column(g).map(|c| &c.values) {
            Some(ColumnValues::Text(v)) => group_words.push(v.iter().map(|w| w.clone().unwrap_or_default()).collect::<Vec<_>>()),
            Some(ColumnValues::Number(_)) => return Err(format!("{g} holds numbers: group by a column of words")),
            None => return Err(format!("the table has no column {g} to group by")),
        }
    }
    let sort = match &ask.sort {
        Some(s) => Some((find(&s.column).ok_or_else(|| format!("the table has no column {} to sort by", s.column))?, s.descending)),
        None => None,
    };
    let open: BTreeSet<&Vec<String>> = ask.open.iter().collect();
    let mut build = Build { table, shown: &shown, calcs: &calcs, group: &ask.group, words: &group_words, levels: &levels, sort, open, budget: ask.limit.unwrap_or(GRID_LEAVES), cut: 0, rows: Vec::new() };
    build.rows.push(GridRow { kind: RowKind::Total, path: Vec::new(), label: "All".into(), depth: 0, leaves: fitting.len(), open: true, row: None, words: None, cells: build.cells(&fitting) });
    build.under(Vec::new(), &fitting);
    let (rows, cut) = (build.rows, build.cut);
    let columns = shown.iter().map(|c| table.calculated.iter().find(|d| d.name == c.name && table.column(&c.name).is_none()).map_or_else(|| grid_column(c), calc_column)).collect();
    Ok(Grid { columns, every, groups, levels, rows, leaves: fitting.len(), cut, said })
}

struct Build<'a> {
    table: &'a Table,
    shown: &'a [&'a TableColumn],
    /// The calculated columns worked out: a group's value is its own rule's, not a stored column's.
    calcs: &'a [(String, Calc<'a>, Vec<Option<f64>>)],
    group: &'a [String],
    words: &'a [Vec<String>],
    levels: &'a [String],
    sort: Option<(&'a TableColumn, bool)>,
    open: BTreeSet<&'a Vec<String>>,
    budget: usize,
    cut: usize,
    rows: Vec<GridRow>,
}

impl Build<'_> {
    /// A group's cells: each number column added up by its rule over the leaves, a word column's words where they agree.
    fn cells(&self, leaves: &[usize]) -> Vec<Option<Cell>> {
        self.shown
            .iter()
            .map(|c| match &c.values {
                ColumnValues::Number(_) if let Some((_, calc, values)) = self.calcs.iter().find(|(n, _, _)| *n == c.name) => {
                    table_calc::group_value(self.table, calc, leaves, values).map(Cell::Number)
                }
                ColumnValues::Number(_) => add_up_column(self.table, &c.name, leaves).map(Cell::Number),
                ColumnValues::Text(v) => {
                    let mut words = leaves.iter().map(|&i| v[i].as_deref().unwrap_or(""));
                    let first = words.next()?;
                    (!first.is_empty() && words.all(|w| w == first)).then(|| Cell::Text(first.to_string()))
                }
            })
            .collect()
    }

    fn leaf_cells(&self, row: usize) -> Vec<Option<Cell>> {
        self.shown
            .iter()
            .map(|c| match &c.values {
                ColumnValues::Number(v) => v[row].map(Cell::Number),
                ColumnValues::Text(v) => v[row].clone().filter(|w| !w.is_empty()).map(Cell::Text),
            })
            .collect()
    }

    /// What's under the group at `path` (its `leaves`): the next level's groups, or the leaves themselves.
    fn under(&mut self, path: Vec<String>, leaves: &[usize]) {
        let depth = path.len();
        if depth < self.group.len() {
            let mut children: Vec<(String, Vec<usize>, Vec<Option<Cell>>)> =
                children_of(&self.group[depth], &self.words[depth], leaves).into_iter().map(|(w, under)| {
                    let cells = self.cells(&under);
                    (w, under, cells)
                }).collect();
            if let Some(k) = self.sort_index() {
                let descending = self.sort.is_some_and(|s| s.1);
                children.sort_by(|a, b| order(a.2[k].as_ref(), b.2[k].as_ref(), descending));
            }
            let innermost = depth + 1 == self.group.len();
            for (word, under, cells) in children {
                let mut p = path.clone();
                p.push(word.clone());
                let open = self.open.contains(&p);
                let label = if word.is_empty() { "(none)".to_string() } else { word };
                // Grouped down to one leaf with nothing of its own left to say (every level grouped by, bench and all):
                // the group is the leaf, picked and typed in as one, not a group over a row that repeats it.
                if innermost && under.len() == 1 && self.own_words(under[0]).is_none() && self.budget > 0 {
                    let row = under[0];
                    let words: Vec<String> = self.levels.iter().map(|l| word_at(self.table, l, row)).collect();
                    self.budget -= 1;
                    self.rows.push(GridRow { kind: RowKind::Leaf, path: path.clone(), label, depth: depth + 1, leaves: 1, open: false, row: Some(row), words: Some(words), cells: self.leaf_cells(row) });
                    continue;
                }
                self.rows.push(GridRow { kind: RowKind::Group, path: p.clone(), label, depth: depth + 1, leaves: under.len(), open, row: None, words: None, cells });
                if open {
                    self.under(p, &under);
                }
            }
            return;
        }
        let mut rows = leaves.to_vec();
        if let Some((column, descending)) = self.sort {
            rows.sort_by(|&a, &b| order(leaf_cell(column, a).as_ref(), leaf_cell(column, b).as_ref(), descending));
        }
        let take = rows.len().min(self.budget);
        self.cut += rows.len() - take;
        self.budget -= take;
        for &row in &rows[..take] {
            let words: Vec<String> = self.levels.iter().map(|l| word_at(self.table, l, row)).collect();
            let label = self.leaf_label(row, &words);
            self.rows.push(GridRow { kind: RowKind::Leaf, path: path.clone(), label, depth: depth + 1, leaves: 1, open: false, row: Some(row), words: Some(words), cells: self.leaf_cells(row) });
        }
    }

    fn sort_index(&self) -> Option<usize> {
        let (column, _) = self.sort?;
        self.shown.iter().position(|c| c.name == column.name)
    }

    /// A leaf's label: its name where the table has one (a row an object); else its levels' words that aren't grouped by,
    /// "3 B1 U 130" under the total, "U 130" under a strip and block.
    fn leaf_label(&self, row: usize, words: &[String]) -> String {
        if let Some(own) = self.own_words(row) {
            return own;
        }
        // Nothing of its own left (its groups say it all): its full words, never a row number a planner can't read.
        let all: Vec<&str> = words.iter().filter(|w| !w.is_empty()).map(String::as_str).collect();
        if all.is_empty() { format!("row {}", row + 1) } else { all.join(" ") }
    }

    /// What a leaf has to say for itself under its groups: its name where the table has one (a row an object), else its
    /// levels' words that aren't grouped by; none when its groups say it all.
    fn own_words(&self, row: usize) -> Option<String> {
        if let Some(ColumnValues::Text(v)) = self.table.column("name").map(|c| &c.values) {
            return v[row].clone().filter(|n| !n.is_empty());
        }
        let label: Vec<String> = self.levels.iter().filter(|l| !self.group.contains(l)).map(|l| word_at(self.table, l, row)).filter(|w| !w.is_empty()).collect();
        (!label.is_empty()).then(|| label.join(" "))
    }
}

fn word_at(table: &Table, column: &str, row: usize) -> String {
    match table.column(column).map(|c| &c.values) {
        Some(ColumnValues::Text(v)) => v[row].clone().unwrap_or_default(),
        _ => String::new(),
    }
}

fn leaf_cell(column: &TableColumn, row: usize) -> Option<Cell> {
    match &column.values {
        ColumnValues::Number(v) => v[row].map(Cell::Number),
        ColumnValues::Text(v) => v[row].clone().filter(|w| !w.is_empty()).map(Cell::Text),
    }
}

/// Numbers by value, words in natural order; a cell with nothing last whichever way.
fn order(a: Option<&Cell>, b: Option<&Cell>, descending: bool) -> Ordering {
    let by = |x: &Cell, y: &Cell| match (x, y) {
        (Cell::Number(p), Cell::Number(q)) => p.total_cmp(q),
        (Cell::Text(p), Cell::Text(q)) => natural(p, q),
        (Cell::Number(_), Cell::Text(_)) => Ordering::Less,
        (Cell::Text(_), Cell::Number(_)) => Ordering::Greater,
    };
    match (a, b) {
        (Some(x), Some(y)) => {
            let o = by(x, y);
            if descending { o.reverse() } else { o }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// The density columns a blocks table weighs its tonnes by, the likeliest first: `rd`, then any named for density.
pub fn density_columns(table: &Table) -> Vec<String> {
    let likely = |k: &str| {
        let k = k.to_ascii_lowercase();
        if k == "rd" { 0 } else if k.ends_with("_rd") || k.contains("density") || k == "sg" { 1 } else { 2 }
    };
    let mut out: Vec<String> = table.columns.iter().filter(|c| matches!(c.values, ColumnValues::Number(_)) && likely(&c.name) < 2).map(|c| c.name.clone()).collect();
    out.sort_by_key(|n| likely(n));
    out
}

/// Objects as a table, a row an object in the order given (decision 1: the blocks table is a table like any other):
/// `name`, `layer` and `kind`; a solid's own `volume`, the volume of record, summed; then every attribute, a column of
/// numbers where every value it has is a number, words otherwise (a yes/no as `yes` or `no`). With `density`, a column
/// of numbers named there, that column adds up by volume and `tonnes` (its volume × it, summed) follows `volume`.
/// Anything gone is left out.
pub fn objects_table(project: &Project, ids: &[ObjectId], density: Option<&str>) -> Table {
    let objects: Vec<_> = ids.iter().filter_map(|&id| project.object(id)).collect();
    let layer_name = |o: &zd_core::Object| project.layer(o.layer).map(|l| l.name.clone()).unwrap_or_default();
    let volumes: Vec<Option<f64>> = objects.iter().map(|o| if let Body::Solid(s) = &o.body { Some(s.volume()) } else { None }).collect();

    // Every attribute any object has, numbers where all its values are; the cut's own first, the rest by name.
    let mut numeric: BTreeMap<String, bool> = BTreeMap::new();
    for o in &objects {
        for (k, v) in o.attributes.iter() {
            let n = matches!(v, AttrValue::Number(_));
            numeric.entry(k.to_string()).and_modify(|was| *was &= n).or_insert(n);
        }
    }
    const FIRST: [&str; 7] = ["strip", "block", "seam", "horizon", "bench", "material", "case"];
    let mut keys: Vec<String> = numeric.keys().filter(|k| !["name", "layer", "kind", "volume", "tonnes"].contains(&k.as_str())).cloned().collect();
    keys.sort_by(|a, b| {
        let place = |k: &str| FIRST.iter().position(|f| *f == k).unwrap_or(FIRST.len());
        place(a).cmp(&place(b)).then_with(|| natural(a, b))
    });

    let text = |name: &str, f: &dyn Fn(&zd_core::Object) -> String| TableColumn::text(name, objects.iter().map(|o| Some(f(o))).collect());
    let mut columns = vec![
        text("name", &|o| o.name.clone()),
        text("layer", &|o| layer_name(o)),
        text("kind", &|o| format!("{:?}", o.kind()).to_ascii_lowercase()),
        TableColumn::adding("volume", "m3", AddUp::Sum, volumes.clone()),
    ];
    let density = density.filter(|d| numeric.get(*d) == Some(&true));
    if let Some(d) = density {
        let tonnes = objects.iter().zip(&volumes).map(|(o, v)| match (v, o.attributes.get(d)) {
            (Some(v), Some(AttrValue::Number(rd))) => Some(v * rd),
            _ => None,
        });
        columns.push(TableColumn::adding("tonnes", "t", AddUp::Sum, tonnes.collect()));
    }
    for k in keys {
        let column = if numeric[&k] {
            let values = objects.iter().map(|o| match o.attributes.get(&k) {
                Some(AttrValue::Number(x)) => Some(*x),
                _ => None,
            });
            let mut c = TableColumn::number(&k, "", values.collect());
            if Some(k.as_str()) == density {
                c.unit = "t/m3".into();
                c.add_up = Some(AddUp::Volume);
            }
            c
        } else {
            let values = objects.iter().map(|o| match o.attributes.get(&k) {
                Some(AttrValue::Text(w)) => Some(w.clone()),
                Some(AttrValue::Number(x)) => Some(x.to_string()),
                Some(AttrValue::Bool(b)) => Some(if *b { "yes" } else { "no" }.to_string()),
                None => None,
            });
            TableColumn::text(&k, values.collect())
        };
        columns.push(column);
    }
    Table { columns, calculated: Vec::new(), setup: None, objects: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two strips, three blocks, coal and waste: volumes and RDs chosen so every add-up is known by hand.
    fn made_up() -> Table {
        let w = |v: &[&str]| v.iter().map(|s| Some(s.to_string())).collect::<Vec<_>>();
        let n = |v: &[f64]| v.iter().map(|&x| Some(x)).collect::<Vec<_>>();
        Table {
            columns: vec![
                TableColumn::text("strip", w(&["1", "1", "1", "2"])),
                TableColumn::text("block", w(&["B1", "B1", "B10", "B2"])),
                TableColumn::text("seam", w(&["U", "L", "U", "U"])),
                TableColumn::adding("Insitu\\Coal\\volume", "m3", AddUp::Sum, n(&[100.0, 300.0, 200.0, 400.0])),
                TableColumn::adding("Insitu\\Coal\\rd", "t/m3", AddUp::Volume, n(&[1.4, 1.6, 1.5, 1.3])),
                TableColumn::adding("Insitu\\Coal\\ash", "%", AddUp::Mass { density: "rd".into() }, n(&[10.0, 20.0, 15.0, 12.0])),
            ],
            calculated: Vec::new(),
            setup: None,
            objects: false,
        }
    }

    fn number(c: &Option<Cell>) -> f64 {
        match c {
            Some(Cell::Number(x)) => *x,
            other => panic!("not a number: {other:?}"),
        }
    }

    #[test]
    fn a_strip_s_subtotal_adds_up_by_each_column_s_rule() {
        let g = grid(&made_up(), &GridAsk { group: vec!["strip".into()], ..Default::default() }).unwrap();
        let labels: Vec<&str> = g.rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["All", "1", "2"]);
        let strip1 = &g.rows[1];
        assert_eq!(strip1.leaves, 3);
        assert_eq!(number(&strip1.cells[3]), 600.0);
        // RD by volume: (140 + 480 + 300) / 600; ash by tonnes: (1,400 + 9,600 + 4,500) / 920.
        assert!((number(&strip1.cells[4]) - 920.0 / 600.0).abs() < 1e-12);
        assert!((number(&strip1.cells[5]) - 15_500.0 / 920.0).abs() < 1e-12);
        // Words where every leaf agrees: strip 1 has one strip, two blocks.
        assert_eq!(strip1.cells[0], Some(Cell::Text("1".into())));
        assert_eq!(strip1.cells[1], None);
        assert_eq!(number(&g.rows[0].cells[3]), 1_000.0);
    }

    #[test]
    fn an_open_group_shows_its_leaves_labelled_by_what_isn_t_grouped() {
        let ask = GridAsk { group: vec!["strip".into()], open: vec![vec!["1".into()]], ..Default::default() };
        let g = grid(&made_up(), &ask).unwrap();
        let rows: Vec<(RowKind, &str, usize)> = g.rows.iter().map(|r| (r.kind, r.label.as_str(), r.depth)).collect();
        assert_eq!(rows, [(RowKind::Total, "All", 0), (RowKind::Group, "1", 1), (RowKind::Leaf, "B1 U", 2), (RowKind::Leaf, "B1 L", 2), (RowKind::Leaf, "B10 U", 2), (RowKind::Group, "2", 1)]);
        assert_eq!(g.rows[2].words.as_deref(), Some(&["1".to_string(), "B1".into(), "U".into()][..]));
        assert_eq!(g.rows[3].row, Some(1));
    }

    #[test]
    fn groups_sort_by_their_subtotals_and_leaves_past_the_limit_are_counted() {
        let ask = GridAsk { group: vec!["block".into()], sort: Some(GridSort { column: "Insitu\\Coal\\volume".into(), descending: false }), ..Default::default() };
        let g = grid(&made_up(), &ask).unwrap();
        let labels: Vec<&str> = g.rows.iter().skip(1).map(|r| r.label.as_str()).collect();
        // B10 has 200 m³; B1 and B2 tie at 400 and keep their natural order.
        assert_eq!(labels, ["B10", "B1", "B2"]);
        let flat = grid(&made_up(), &GridAsk { limit: Some(2), ..Default::default() }).unwrap();
        assert_eq!(flat.rows.len(), 3);
        assert_eq!(flat.cut, 2);
        assert_eq!(flat.leaves, 4);
    }

    #[test]
    fn a_filter_and_columns_picked_narrow_the_grid() {
        let mut when = BTreeMap::new();
        when.insert("seam".to_string(), "U".to_string());
        let ask = GridAsk { when, columns: vec!["Insitu\\Coal\\volume".into()], ..Default::default() };
        let g = grid(&made_up(), &ask).unwrap();
        assert_eq!(g.columns.len(), 1);
        assert_eq!(g.every.len(), 6);
        assert_eq!(g.groups, ["strip", "block", "seam"]);
        assert_eq!(number(&g.rows[0].cells[0]), 700.0);
        assert!(grid(&made_up(), &GridAsk { group: vec!["Insitu\\Coal\\volume".into()], ..Default::default() }).unwrap_err().contains("numbers"));
        assert!(grid(&made_up(), &GridAsk { group: vec!["pit".into()], ..Default::default() }).unwrap_err().contains("no column pit"));
    }
}
