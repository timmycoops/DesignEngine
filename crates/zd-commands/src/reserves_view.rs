//! The reserves table seen as Tim reads it (decision 0075, workstream 5a of 0070): a tree of its levels, and a node's
//! fields broken down by its children, the fields down the side and the children across. Tim, 15 November 2026: "I want
//! to see the break down of fields as rows instead of columns though - easier to see the break down of the values".
//!
//! Every value is added up from the table's leaves by the column's own rule (sum, by volume, by mass, largest,
//! smallest), the weight found beside it, so a parent here is what the run's own add-up gives.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use zd_core::{AttrValue, Attributes, ColumnValues, Table};
use zd_reserves::AddUp;

pub use zd_core::leaves::{case_of, leaf_of, level_defaults, levels};

/// One node of the tree: the level values down to it, its label, how many leaves are under it and its children.
#[derive(Clone, Debug, Serialize)]
pub struct TreeNode {
    pub path: Vec<String>,
    pub label: String,
    pub leaves: usize,
    pub children: Vec<TreeNode>,
}

/// The table's tree: the root (every leaf) with a child per value of the first level, and so on down. A leaf with no value
/// at a level sits under "(none)" there.
pub fn tree(table: &Table) -> TreeNode {
    tree_where(table, &BTreeMap::new())
}

/// The tree of the leaves a filter keeps (decision 0088, workstream 4): a node no leaf under it fits has no place in it.
pub fn tree_where(table: &Table, when: &BTreeMap<String, String>) -> TreeNode {
    let levels = levels(table);
    let words = level_words(table, &levels);
    node(&levels, &words, Vec::new(), "All".into(), &rows_fitting(table, when))
}

pub use zd_core::leaves::rows_fitting;

/// Each leaf's levels and other words as attributes, a leaf a row: what the filter box counts to tick. Fields are typed.
pub fn filter_rows(table: &Table) -> Vec<Attributes> {
    let words: Vec<(&str, &Vec<Option<String>>)> = table
        .columns
        .iter()
        .filter(|c| !c.name.contains('\\'))
        .filter_map(|c| match &c.values {
            ColumnValues::Text(v) => Some((c.name.as_str(), v)),
            ColumnValues::Number(_) => None,
        })
        .collect();
    (0..table.rows())
        .map(|i| {
            let mut row = Attributes::new();
            for (name, v) in &words {
                if let Some(w) = v[i].as_deref().filter(|w| !w.is_empty()) {
                    row.insert(name.to_string(), AttrValue::Text(w.to_string()));
                }
            }
            row
        })
        .collect()
}

fn node(levels: &[String], words: &[Vec<String>], path: Vec<String>, label: String, leaves: &[usize]) -> TreeNode {
    let depth = path.len();
    let children = if depth < levels.len() {
        children_of(&levels[depth], &words[depth], leaves)
            .into_iter()
            .map(|(value, under)| {
                let mut p = path.clone();
                p.push(value.clone());
                node(levels, words, p, label_of(&value), &under)
            })
            .collect()
    } else {
        Vec::new()
    };
    TreeNode { path, label, leaves: leaves.len(), children }
}

fn label_of(value: &str) -> String {
    if value.is_empty() { "(none)".into() } else { value.to_string() }
}

/// Each level's word on each leaf, "" where it has none.
fn level_words(table: &Table, levels: &[String]) -> Vec<Vec<String>> {
    levels
        .iter()
        .map(|l| match table.column(l).map(|c| &c.values) {
            Some(ColumnValues::Text(v)) => v.iter().map(|w| w.clone().unwrap_or_default()).collect(),
            _ => vec![String::new(); table.rows()],
        })
        .collect()
}

/// A node's children at `level`: each value with the leaves holding it. A seam keeps the stack's order, as the run wrote
/// it, top down; a bench goes top down by its RL; any other level in natural order (B2 before B10).
pub(crate) fn children_of(level: &str, words: &[String], leaves: &[usize]) -> Vec<(String, Vec<usize>)> {
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for &i in leaves {
        match groups.iter_mut().find(|(w, _)| *w == words[i]) {
            Some(g) => g.1.push(i),
            None => groups.push((words[i].clone(), vec![i])),
        }
    }
    if level.eq_ignore_ascii_case("bench") {
        groups.sort_by(|a, b| {
            let (x, y) = (crate::report::bench_place(&a.0), crate::report::bench_place(&b.0));
            x.0.cmp(&y.0).then(x.1.total_cmp(&y.1))
        });
    } else if !level.eq_ignore_ascii_case("seam") {
        groups.sort_by(|a, b| natural(&a.0, &b.0));
    }
    // Leaves with no value last, whatever the level.
    groups.sort_by_key(|g| g.0.is_empty());
    groups
}

/// Natural order: runs of digits compared as numbers, so 2 comes before 10 and B2 before B10.
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) if p.is_ascii_digit() && q.is_ascii_digit() => {
                let (mut m, mut n) = (String::new(), String::new());
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    m.push(c);
                    x.next();
                }
                while let Some(c) = y.peek().copied().filter(char::is_ascii_digit) {
                    n.push(c);
                    y.next();
                }
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let order = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(p), Some(q)) => {
                let order = p.to_ascii_lowercase().cmp(&q.to_ascii_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// One column of a breakdown: the node's total, or one of its children (and, where a child is opened, its children after
/// it, one level deeper).
#[derive(Clone, Debug, Serialize)]
pub struct BreakdownColumn {
    pub label: String,
    pub path: Vec<String>,
    /// 0 for the node itself, 1 for its children, 2 for a child's children, and so on.
    pub depth: usize,
    pub leaves: usize,
    /// Whether it has children of its own to open.
    pub opens: bool,
    pub open: bool,
}

/// One field of a breakdown: its column in the table (a folder path), its folders and name, its unit, and its value in
/// each breakdown column, added up by its rule; a field of words, its words instead.
#[derive(Clone, Debug, Serialize)]
pub struct BreakdownField {
    pub column: String,
    pub folders: Vec<String>,
    pub name: String,
    pub unit: String,
    pub values: Vec<Option<f64>>,
    pub words: Option<Vec<Option<String>>>,
}

/// A node's fields broken down by its children.
#[derive(Clone, Debug, Serialize)]
pub struct Breakdown {
    pub levels: Vec<String>,
    pub node: Vec<String>,
    pub columns: Vec<BreakdownColumn>,
    pub fields: Vec<BreakdownField>,
    /// Leaves not shown, past the first `LEAVES_SHOWN`, when the node is broken down to its leaves.
    pub hidden: usize,
}

/// The most leaves a node's leaves breakdown shows across; past that, a lower node is picked to see the rest.
pub const LEAVES_SHOWN: usize = 500;

/// The node at `node` (its level values, top down) broken down: the fields down the side, in the table's order, and across
/// the node's total then its children, each child in `open` followed by its own children. A field with no value in any
/// column is left out.
pub fn breakdown(table: &Table, node: &[String], open: &[Vec<String>]) -> Result<Breakdown, String> {
    breakdown_where(table, node, open, &BTreeMap::new())
}

/// [`breakdown`] over the leaves a filter keeps (decision 0088, workstream 4): the node's total and every column add up
/// only those, each by its own rule, so a filtered node is what the run's add-up gives for them.
pub fn breakdown_where(table: &Table, node: &[String], open: &[Vec<String>], when: &BTreeMap<String, String>) -> Result<Breakdown, String> {
    let (levels, words, mut columns, mut sets) = node_column(table, node, when)?;
    let under = sets[0].clone();
    add_children(&levels, &words, node, &under, 1, open, &mut columns, &mut sets);
    let fields = fields_of(table, &sets);
    Ok(Breakdown { levels, node: node.to_vec(), columns, fields, hidden: 0 })
}

/// The node at `node` broken down to its leaves (Tim, 16 November 2026: "just the leaves ... from left to right (top to
/// bottom of the stack)"): its total, then every leaf under it in the tree's order, a seam's benches top down before the
/// next seam's, with no subtotals between. A leaf is labelled by its words below the node ("U 130" under a block). Past
/// `LEAVES_SHOWN` leaves the rest are counted in `hidden`, not shown.
pub fn breakdown_leaves(table: &Table, node: &[String]) -> Result<Breakdown, String> {
    breakdown_leaves_where(table, node, &BTreeMap::new())
}

/// [`breakdown_leaves`] over the leaves a filter keeps.
pub fn breakdown_leaves_where(table: &Table, node: &[String], when: &BTreeMap<String, String>) -> Result<Breakdown, String> {
    let (levels, words, mut columns, mut sets) = node_column(table, node, when)?;
    let under = sets[0].clone();
    let mut leaves = Vec::new();
    leaves_in_order(&levels, &words, node, &under, &mut leaves);
    let hidden = leaves.len().saturating_sub(LEAVES_SHOWN);
    for (path, rows) in leaves.into_iter().take(LEAVES_SHOWN) {
        let below: Vec<&str> = path[node.len()..].iter().map(String::as_str).filter(|w| !w.is_empty()).collect();
        let label = if below.is_empty() { "(none)".to_string() } else { below.join(" ") };
        columns.push(BreakdownColumn { label, path, depth: 1, leaves: rows.len(), opens: false, open: false });
        sets.push(rows);
    }
    let fields = fields_of(table, &sets);
    Ok(Breakdown { levels, node: node.to_vec(), columns, fields, hidden })
}

/// Every leaf under `path`, with its rows, in the tree's order.
fn leaves_in_order(levels: &[String], words: &[Vec<String>], path: &[String], rows: &[usize], out: &mut Vec<(Vec<String>, Vec<usize>)>) {
    let at = path.len();
    if at >= levels.len() {
        out.push((path.to_vec(), rows.to_vec()));
        return;
    }
    for (value, under) in children_of(&levels[at], &words[at], rows) {
        let mut p = path.to_vec();
        p.push(value);
        leaves_in_order(levels, words, &p, &under, out);
    }
}

type NodeColumn = (Vec<String>, Vec<Vec<String>>, Vec<BreakdownColumn>, Vec<Vec<usize>>);

/// The table's levels and words, and the node's own column (its total) with its rows: those under it a filter keeps. A
/// node a filter keeps nothing of is empty, not refused.
fn node_column(table: &Table, node: &[String], when: &BTreeMap<String, String>) -> Result<NodeColumn, String> {
    let levels = levels(table);
    if node.len() > levels.len() {
        return Err(format!("the table has {} levels, not {}", levels.len(), node.len()));
    }
    let words = level_words(table, &levels);
    let kept = rows_fitting(table, when);
    let under: Vec<usize> = kept.iter().copied().filter(|&i| node.iter().enumerate().all(|(d, w)| words[d][i] == *w)).collect();
    if under.is_empty() && table.rows() > 0 && kept.len() == table.rows() {
        return Err(format!("no leaves under {}", node.join(" > ")));
    }
    let columns = vec![BreakdownColumn {
        label: node.last().map_or("All".into(), |w| label_of(w)),
        path: node.to_vec(),
        depth: 0,
        leaves: under.len(),
        opens: node.len() < levels.len(),
        open: true,
    }];
    Ok((levels, words, columns, vec![under]))
}

/// Each folder field added up over each column's rows; a field with no value in any column is left out.
fn fields_of(table: &Table, sets: &[Vec<usize>]) -> Vec<BreakdownField> {
    let stack = seam_order(table);
    table
        .columns
        .iter()
        .filter(|c| c.name.contains('\\'))
        .filter_map(|c| {
            let parts: Vec<&str> = c.name.split('\\').collect();
            let (name, folders) = parts.split_last()?;
            let (values, words) = match &c.values {
                ColumnValues::Number(_) => (sets.iter().map(|leaves| add_up_column(table, &c.name, leaves)).collect::<Vec<_>>(), None),
                ColumnValues::Text(v) => (vec![None; sets.len()], Some(sets.iter().map(|leaves| words_of(v, leaves, &stack)).collect::<Vec<_>>())),
            };
            let held = values.iter().any(Option::is_some) || words.as_ref().is_some_and(|w| w.iter().any(Option::is_some));
            held.then(|| BreakdownField { column: c.name.clone(), folders: folders.iter().map(|f| f.to_string()).collect(), name: name.to_string(), unit: c.unit.clone(), values, words })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn add_children(levels: &[String], words: &[Vec<String>], path: &[String], leaves: &[usize], depth: usize, open: &[Vec<String>], columns: &mut Vec<BreakdownColumn>, sets: &mut Vec<Vec<usize>>) {
    let at = path.len();
    if at >= levels.len() {
        return;
    }
    for (value, under) in children_of(&levels[at], &words[at], leaves) {
        let mut p = path.to_vec();
        p.push(value.clone());
        let opened = open.contains(&p) && at + 1 < levels.len();
        columns.push(BreakdownColumn { label: label_of(&value), path: p.clone(), depth, leaves: under.len(), opens: at + 1 < levels.len(), open: opened });
        sets.push(under.clone());
        if opened {
            add_children(levels, words, &p, &under, depth + 1, open, columns, sets);
        }
    }
}

/// The distinct words of a column over some leaves, each leaf's own "a; b" split: seams (and sections, named for their
/// seams) in the stack's order, top down, as working sections are (app review brief, 17 Nov, item 5); any other word
/// after them, in the order met.
fn words_of(values: &[Option<String>], leaves: &[usize], stack: &HashMap<String, usize>) -> Option<String> {
    let mut seen: Vec<&str> = Vec::new();
    for &i in leaves {
        for w in values[i].as_deref().unwrap_or("").split("; ").filter(|w| !w.is_empty()) {
            if !seen.contains(&w) {
                seen.push(w);
            }
        }
    }
    seen.sort_by_key(|w| stack.get(*w).copied().unwrap_or(usize::MAX));
    (!seen.is_empty()).then(|| seen.join("; "))
}

/// The seams in the stack's order, top down, from the table itself: within each column (a case and its levels above the
/// seam) the run writes a seam's leaves before the seam below, so each column says which seams come before which, and
/// the columns together give the whole stack, though no one column holds every seam (a seam that subcrops, or is lost
/// in some blocks). Where two seams are never in one column, the one met first comes first.
fn seam_order(table: &Table) -> HashMap<String, usize> {
    let levels = levels(table);
    let Some(at) = levels.iter().position(|l| l.eq_ignore_ascii_case("seam")) else { return HashMap::new() };
    let words = level_words(table, &levels);
    let case = match table.column("case").map(|c| &c.values) {
        Some(ColumnValues::Text(v)) => v.iter().map(|w| w.clone().unwrap_or_default()).collect(),
        _ => vec![String::new(); table.rows()],
    };
    let mut met: Vec<String> = Vec::new();
    let mut after: HashMap<String, Vec<String>> = HashMap::new();
    let mut last: HashMap<Vec<String>, String> = HashMap::new();
    for i in 0..table.rows() {
        let seam = &words[at][i];
        if seam.is_empty() {
            continue;
        }
        if !met.contains(seam) {
            met.push(seam.clone());
        }
        let mut column = vec![case[i].clone()];
        column.extend(words[..at].iter().map(|w| w[i].clone()));
        if let Some(above) = last.insert(column, seam.clone()).filter(|above| above != seam) {
            let below = after.entry(above).or_default();
            if !below.contains(seam) {
                below.push(seam.clone());
            }
        }
    }
    // Top down: a seam goes once every seam seen above it has gone, the first met first; a loop (a column out of the
    // stack's order) is broken by taking the first met left.
    let mut above_count: HashMap<&str, usize> = met.iter().map(|m| (m.as_str(), 0)).collect();
    for below in after.values().flatten() {
        *above_count.get_mut(below.as_str()).expect("met") += 1;
    }
    let mut order: HashMap<String, usize> = HashMap::new();
    while order.len() < met.len() {
        let next = met.iter().find(|m| !order.contains_key(*m) && above_count[m.as_str()] == 0).or_else(|| met.iter().find(|m| !order.contains_key(*m))).expect("one left");
        order.insert(next.clone(), order.len());
        for below in after.get(next).into_iter().flatten() {
            let n = above_count.get_mut(below.as_str()).expect("met");
            *n = n.saturating_sub(1);
        }
    }
    order
}

/// A number column added up over some leaves by its own rule, over the leaves with a value in it: summed; by volume, the
/// `volume` beside it under the same folders; by mass, that volume × the density the rule names (beside it, or a full
/// path); the largest or smallest. None where no leaf has a value, or no weight.
pub fn add_up_column(table: &Table, name: &str, leaves: &[usize]) -> Option<f64> {
    let column = table.column(name)?;
    if column.add_up.is_none()
        && let ColumnValues::Number(v) = &column.values
        && let Some(kind) = Geometry::of(name)
    {
        return add_up_geometry(table, name, v, kind, leaves);
    }
    let (Some(rule), ColumnValues::Number(v)) = (&column.add_up, &column.values) else { return None };
    let folder = name.rsplit_once('\\').map_or("", |(f, _)| f);
    let beside = |field: &str| -> Option<&Vec<Option<f64>>> {
        let path = if field.contains('\\') || folder.is_empty() { field.to_string() } else { format!("{folder}\\{field}") };
        match &table.column(&path)?.values {
            ColumnValues::Number(v) => Some(v),
            _ => None,
        }
    };
    let volume = beside("volume");
    let weighted = |w: &dyn Fn(usize) -> Option<f64>| {
        let (mut sum, mut total) = (0.0, 0.0);
        for &i in leaves {
            let (Some(x), Some(w)) = (v[i], w(i)) else { continue };
            if w != 0.0 {
                sum += w * x;
                total += w;
            }
        }
        (total != 0.0).then(|| sum / total)
    };
    match rule {
        AddUp::Sum => {
            let values: Vec<f64> = leaves.iter().filter_map(|&i| v[i]).collect();
            (!values.is_empty()).then(|| values.iter().sum())
        }
        AddUp::Volume => weighted(&|i| volume.and_then(|vol| vol[i])),
        AddUp::Mass { density } => {
            let d = beside(density);
            weighted(&|i| Some(volume?[i]? * d?[i]?))
        }
        AddUp::Ratio { of, per } => {
            let (Some(of), Some(per)) = (beside(of), beside(per)) else { return None };
            let (top, bottom): (f64, f64) = leaves.iter().map(|&i| (of[i].unwrap_or(0.0), per[i].unwrap_or(0.0))).fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
            (bottom != 0.0).then(|| top / bottom)
        }
        AddUp::Max => leaves.iter().filter_map(|&i| v[i]).reduce(f64::max),
        AddUp::Min => leaves.iter().filter_map(|&i| v[i]).reduce(f64::min),
    }
}

/// Every breakdown column's leaves summed for one field, to check a parent equals its children: the node's value and the
/// sum of its children's (depth 1).
pub fn children_sum(b: &Breakdown, field: &str) -> Option<(f64, f64)> {
    let f = b.fields.iter().find(|f| f.column == field)?;
    let total = f.values[0]?;
    let children: f64 = b.columns.iter().zip(&f.values).filter(|(c, _)| c.depth == 1).filter_map(|(_, v)| *v).sum();
    Some((total, children))
}

/// A field's value for each solid of a reserves table's case (decision 0075, point 7: blocks coloured by any reserves
/// field): the leaf the solid is on, matched by its levels' words as Select its solids matches it; or with `by_block`, the
/// block it's in (the levels above the seam), the field added up by its own rule over the block's leaves. A solid on no
/// leaf of the table, or whose leaf or block has no value, is left out.
pub fn values_by_solid<'a>(table: &Table, attributes: &Attributes, column: &str, by_block: bool, solids: impl IntoIterator<Item = (zd_core::ObjectId, &'a Attributes)>) -> Vec<(zd_core::ObjectId, f64)> {
    let levels = levels(table);
    let defaults = level_defaults(attributes);
    let case = case_of(attributes);
    let depth = if by_block { levels.iter().position(|l| l.eq_ignore_ascii_case("seam")).unwrap_or(levels.len().saturating_sub(1)) } else { levels.len() };
    // Each row's words for the levels, cut to the depth: the leaves under each key.
    let words: Vec<&Vec<Option<String>>> = levels
        .iter()
        .filter_map(|l| match table.column(l).map(|c| &c.values) {
            Some(ColumnValues::Text(v)) => Some(v),
            _ => None,
        })
        .collect();
    let mut under: std::collections::HashMap<Vec<String>, Vec<usize>> = std::collections::HashMap::new();
    for row in 0..table.rows() {
        let key: Vec<String> = words.iter().take(depth).map(|v| v[row].clone().unwrap_or_default()).collect();
        under.entry(key).or_default().push(row);
    }
    let mut value: std::collections::HashMap<Vec<String>, Option<f64>> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for (id, a) in solids {
        if case_of(a) != case {
            continue;
        }
        let mut key = leaf_of(&levels, &defaults, a);
        key.truncate(depth);
        let found = *value.entry(key.clone()).or_insert_with(|| under.get(&key).and_then(|rows| add_up_column(table, column, rows)));
        if let Some(x) = found.filter(|x| x.is_finite()) {
            out.push((id, x));
        }
    }
    out
}

/// A block's geometry, which doesn't add up across leaves as a field does (decision 0086).
#[derive(Clone, Copy, PartialEq)]
enum Geometry {
    Thickness,
    Area,
    Length,
    /// A block's width and dip: its own, area-weighted above it.
    Width,
    Dip,
}

impl Geometry {
    fn of(name: &str) -> Option<Self> {
        match name.rsplit_once('\\').map_or(name, |(_, f)| f) {
            "thickness" => Some(Self::Thickness),
            "area" => Some(Self::Area),
            "length" => Some(Self::Length),
            "width" => Some(Self::Width),
            "dip" => Some(Self::Dip),
            _ => None,
        }
    }
}

/// Each leaf's block: its words for the levels above the seam (the column a reserves run makes), as `Dataflow::
/// column_levels` splits them; every level but the last where there's no seam level.
fn blocks_of(table: &Table, leaves: &[usize]) -> Vec<String> {
    let levels = levels(table);
    let upper = levels.iter().position(|l| l.eq_ignore_ascii_case("seam")).unwrap_or(levels.len().saturating_sub(1));
    let words: Vec<&Vec<Option<String>>> = levels[..upper]
        .iter()
        .chain(std::iter::once(&"case".to_string()))
        .filter_map(|l| match table.column(l).map(|c| &c.values) {
            Some(ColumnValues::Text(v)) => Some(v),
            _ => None,
        })
        .collect();
    leaves.iter().map(|&i| words.iter().map(|v| v[i].as_deref().unwrap_or("")).collect::<Vec<_>>().join("\u{1}")).collect()
}

/// Thickness, area, length, width and dip over some leaves (decision 0086): a leaf its own; above it, by block, thickness
/// added down a block (its benches and seams stacked) and averaged over blocks by their areas, area each block's largest
/// added over blocks, length a block's own and none above it, width and dip a block's own averaged over blocks by their
/// areas.
fn add_up_geometry(table: &Table, name: &str, v: &[Option<f64>], kind: Geometry, leaves: &[usize]) -> Option<f64> {
    let folder = name.rsplit_once('\\').map_or("", |(f, _)| f);
    let area = match table.column(&format!("{folder}\\area")).map(|c| &c.values) {
        Some(ColumnValues::Number(a)) => Some(a),
        _ => None,
    };
    let held: Vec<usize> = leaves.iter().copied().filter(|&i| v[i].is_some()).collect();
    if held.is_empty() {
        return None;
    }
    // Each block's added thickness or largest value, and its largest area.
    let mut blocks: Vec<(String, f64, Option<f64>)> = Vec::new();
    for (i, block) in held.iter().zip(blocks_of(table, &held)) {
        let x = v[*i].unwrap_or(0.0);
        let a = area.and_then(|a| a[*i]);
        match blocks.iter_mut().find(|b| b.0 == block) {
            Some(b) => {
                b.1 = if kind == Geometry::Thickness { b.1 + x } else { b.1.max(x) };
                b.2 = match (b.2, a) {
                    (Some(p), Some(q)) => Some(p.max(q)),
                    (p, q) => p.or(q),
                };
            }
            None => blocks.push((block, x, a)),
        }
    }
    match kind {
        Geometry::Length => (blocks.len() == 1).then(|| blocks[0].1),
        Geometry::Area => Some(blocks.iter().map(|b| b.1).sum()),
        Geometry::Thickness | Geometry::Width | Geometry::Dip => {
            let weighed: f64 = blocks.iter().map(|b| b.2.unwrap_or(0.0)).sum();
            if weighed > 0.0 {
                Some(blocks.iter().map(|b| b.1 * b.2.unwrap_or(0.0)).sum::<f64>() / weighed)
            } else {
                Some(blocks.iter().map(|b| b.1).sum::<f64>() / blocks.len() as f64)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zd_core::TableColumn;

    use super::*;

    fn n(v: &[f64]) -> Vec<Option<f64>> {
        v.iter().map(|x| if x.is_nan() { None } else { Some(*x) }).collect()
    }

    fn t(v: &[&str]) -> Vec<Option<String>> {
        v.iter().map(|w| if w.is_empty() { None } else { Some(w.to_string()) }).collect()
    }

    /// Strip 1 with blocks B2 and B10, B10 holding seam U on benches 110 and 100 and seam L; coal volumes, RD and ash.
    fn table() -> Table {
        Table {
            columns: vec![
                TableColumn::text("case", t(&["Base", "Base", "Base", "Base"])),
                TableColumn::text("strip", t(&["1", "1", "1", "1"])),
                TableColumn::text("block", t(&["B10", "B10", "B10", "B2"])),
                TableColumn::text("seam", t(&["U", "U", "L", "U"])),
                TableColumn::text("bench", t(&["100", "110", "", ""])),
                TableColumn::adding("ROM\\Coal\\volume", "m3", AddUp::Sum, n(&[1_000.0, 3_000.0, 2_000.0, 4_000.0])),
                TableColumn::adding("ROM\\Coal\\rd", "t/m3", AddUp::Volume, n(&[1.4, 1.6, 1.5, 1.5])),
                TableColumn::adding("ROM\\Coal\\ash", "%", AddUp::Mass { density: "rd".into() }, n(&[10.0, 20.0, 12.0, f64::NAN])),
                TableColumn::text("ROM\\section", t(&["U", "U", "L", "U"])),
                TableColumn::adding("Loss\\Roof\\volume", "m3", AddUp::Sum, n(&[f64::NAN, f64::NAN, f64::NAN, f64::NAN])),
            ],
            calculated: Vec::new(),
            setup: None,
            objects: false,
        }
    }

    /// Three sections in the stack U, M and L (app review brief, 17 Nov, item 5): block B1, met first, has only M and L
    /// (U subcrops there), and B2 has all three. At All and at the strip the ROM sections read U; M; L, the stack's order,
    /// as the working sections do, not M; L; U, the order met; a block's own reads its own seams, top down.
    #[test]
    fn section_lists_follow_the_stack_not_the_order_met() {
        let table = Table {
            columns: vec![
                TableColumn::text("case", t(&["Base", "Base", "Base", "Base", "Base"])),
                TableColumn::text("strip", t(&["1", "1", "1", "1", "1"])),
                TableColumn::text("block", t(&["B1", "B1", "B2", "B2", "B2"])),
                TableColumn::text("seam", t(&["M", "L", "U", "M", "L"])),
                TableColumn::adding("ROM\\Coal\\volume", "m3", AddUp::Sum, n(&[1.0, 1.0, 1.0, 1.0, 1.0])),
                TableColumn::text("ROM\\section", t(&["M", "L", "U", "M", "L"])),
                TableColumn::text("Working section\\id", t(&["4", "6", "2", "4", "6"])),
            ],
            calculated: Vec::new(),
            setup: None,
            objects: false,
        };
        let words = |node: &[&str], column: &str| {
            let node: Vec<String> = node.iter().map(|w| w.to_string()).collect();
            let b = breakdown(&table, &node, &[]).unwrap();
            b.fields.iter().find(|f| f.column == column).and_then(|f| f.words.as_ref()?[0].clone())
        };
        assert_eq!(words(&[], "ROM\\section").as_deref(), Some("U; M; L"), "All, in the stack's order");
        assert_eq!(words(&["1"], "ROM\\section").as_deref(), Some("U; M; L"), "the strip");
        assert_eq!(words(&["1", "B1"], "ROM\\section").as_deref(), Some("M; L"), "B1's own, top down");
        assert_eq!(words(&[], "Working section\\id").as_deref(), Some("4; 6; 2"), "not seams: in the order met");
    }

    #[test]
    fn the_tree_follows_the_levels_in_natural_order_benches_top_down() {
        let tree = tree(&table());
        assert_eq!(tree.leaves, 4);
        let strip = &tree.children[0];
        assert_eq!(strip.children.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["B2", "B10"]);
        let b10 = &strip.children[1];
        assert_eq!(b10.children.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["U", "L"], "seams in the stack's order");
        assert_eq!(b10.children[0].children.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["110", "100"], "benches top down");
        assert_eq!(b10.children[1].children[0].label, "(none)");
    }

    /// B10's breakdown: its total and its seams, U opened to its benches. Volume sums (6,000 = 4,000 + 2,000), RD by
    /// volume ((1.4 × 1,000 + 1.6 × 3,000) ÷ 4,000 = 1.55 for U), ash by mass ((10 × 1,400 + 20 × 4,800) ÷ 6,200 for U).
    /// A field with no value anywhere is left out; the section's words are joined.
    #[test]
    fn a_node_breaks_down_by_its_children_each_field_by_its_rule() {
        let b = breakdown(&table(), &["1".into(), "B10".into()], &[vec!["1".into(), "B10".into(), "U".into()]]).unwrap();
        let labels: Vec<(&str, usize)> = b.columns.iter().map(|c| (c.label.as_str(), c.depth)).collect();
        assert_eq!(labels, [("B10", 0), ("U", 1), ("110", 2), ("100", 2), ("L", 1)]);
        let f = |name: &str| b.fields.iter().find(|f| f.column == name).unwrap();
        assert_eq!(f("ROM\\Coal\\volume").values, [Some(6_000.0), Some(4_000.0), Some(3_000.0), Some(1_000.0), Some(2_000.0)]);
        let rd = f("ROM\\Coal\\rd").values[1].unwrap();
        assert!((rd - 1.55).abs() < 1e-12, "{rd}");
        let ash = f("ROM\\Coal\\ash").values[1].unwrap();
        assert!((ash - (10.0 * 1_400.0 + 20.0 * 4_800.0) / 6_200.0).abs() < 1e-12, "{ash}");
        assert!(b.fields.iter().all(|f| f.column != "Loss\\Roof\\volume"), "no values, left out");
        assert_eq!(f("ROM\\section").words.as_ref().unwrap()[0].as_deref(), Some("U; L"));
        assert_eq!(f("ROM\\Coal\\volume").folders, ["ROM", "Coal"]);
        let (total, children) = children_sum(&b, "ROM\\Coal\\volume").unwrap();
        assert!((total - children).abs() < 1e-9);
    }

    /// Strip 1 to its leaves, top of the stack to the bottom with no subtotals: B2's U (no bench), then B10's U on 110
    /// and 100, then B10's L. The leaves add up to the strip (10,000 m³).
    #[test]
    fn a_node_breaks_down_to_its_leaves_in_stack_order() {
        let b = breakdown_leaves(&table(), &["1".into()]).unwrap();
        let labels: Vec<(&str, usize)> = b.columns.iter().map(|c| (c.label.as_str(), c.depth)).collect();
        assert_eq!(labels, [("1", 0), ("B2 U", 1), ("B10 U 110", 1), ("B10 U 100", 1), ("B10 L", 1)]);
        assert_eq!(b.hidden, 0);
        assert!(b.columns[1..].iter().all(|c| !c.opens && c.leaves == 1));
        assert_eq!(b.columns[4].path, ["1", "B10", "L", ""]);
        let f = |name: &str| b.fields.iter().find(|f| f.column == name).unwrap();
        assert_eq!(f("ROM\\Coal\\volume").values, [Some(10_000.0), Some(4_000.0), Some(3_000.0), Some(1_000.0), Some(2_000.0)]);
        let (total, children) = children_sum(&b, "ROM\\Coal\\volume").unwrap();
        assert!((total - children).abs() < 1e-9);
        // Under a block, a leaf is labelled from the block down; under a leaf, the leaf alone.
        let b10 = breakdown_leaves(&table(), &["1".into(), "B10".into()]).unwrap();
        assert_eq!(b10.columns.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["B10", "U 110", "U 100", "L"]);
        let leaf = breakdown_leaves(&table(), &["1".into(), "B10".into(), "L".into(), "".into()]).unwrap();
        assert_eq!(leaf.columns.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["(none)", "(none)"]);
    }

    /// A filter on the leaves (decision 0088, workstream 4): `seam: U` keeps B10's two U benches and B2's U, 8,000 m³ of
    /// the strip's 10,000; the tree holds U alone; B10's RD is then U's by volume, 1.55. `bench: >=110` keeps one leaf;
    /// a field compared, `ROM\Coal\ash: <15`, keeps the two leaves of ash under 15 (10 and 12), and B2 with none is out;
    /// `seam: !U` is B10's L alone. A filter keeping nothing under a node gives it empty, not refused.
    #[test]
    fn a_filter_keeps_the_leaves_that_fit_and_each_field_adds_up_over_them() {
        let when = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<BTreeMap<_, _>>();
        let table = table();
        assert_eq!(rows_fitting(&table, &when(&[])), [0, 1, 2, 3]);
        assert_eq!(rows_fitting(&table, &when(&[("Seam", "u")])), [0, 1, 3], "the name's case and the word's aside");
        let strip = breakdown_where(&table, &["1".into()], &[], &when(&[("seam", "U")])).unwrap();
        let f = |b: &Breakdown, name: &str| b.fields.iter().find(|f| f.column == name).unwrap().values.clone();
        assert_eq!(f(&strip, "ROM\\Coal\\volume"), [Some(8_000.0), Some(4_000.0), Some(4_000.0)], "the strip, B2 and B10 over U alone");
        assert_eq!(strip.columns[0].leaves, 3);
        let b10 = breakdown_where(&table, &["1".into(), "B10".into()], &[], &when(&[("seam", "U")])).unwrap();
        assert!((f(&b10, "ROM\\Coal\\rd")[0].unwrap() - 1.55).abs() < 1e-12, "RD by volume over the U leaves");
        let tree = tree_where(&table, &when(&[("seam", "U")]));
        assert_eq!((tree.leaves, tree.children[0].children[1].children.len()), (3, 1), "B10 holds U alone");
        assert_eq!(rows_fitting(&table, &when(&[("bench", ">=110")])), [1]);
        assert_eq!(rows_fitting(&table, &when(&[("ROM\\Coal\\ash", "<15")])), [0, 2], "B2 has no ash: a value that isn't there");
        assert_eq!(rows_fitting(&table, &when(&[("seam", "!U")])), [2]);
        let none = breakdown_where(&table, &["1".into(), "B2".into()], &[], &when(&[("seam", "L")])).unwrap();
        assert_eq!((none.columns[0].leaves, none.fields.len()), (0, 0), "nothing kept under B2: empty, not refused");
        let leaves = breakdown_leaves_where(&table, &["1".into()], &when(&[("seam", "U")])).unwrap();
        assert_eq!(leaves.columns.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["1", "B2 U", "B10 U 110", "B10 U 100"]);
        let rows = filter_rows(&table);
        assert_eq!((rows.len(), rows[2].get("seam"), rows[2].get("bench")), (4, Some(&AttrValue::Text("L".into())), None), "levels' words, a blank left out; no fields");
        assert!(rows[0].keys().all(|k| !k.contains('\\')));
    }

    #[test]
    fn a_node_not_in_the_table_is_refused() {
        assert!(breakdown(&table(), &["2".into()], &[]).unwrap_err().contains("no leaves under 2"));
        assert!(breakdown(&table(), &["1".into(), "B10".into(), "U".into(), "100".into(), "x".into()], &[]).is_err());
    }

    #[test]
    fn natural_order_reads_numbers_as_numbers() {
        let mut v = vec!["B10", "B2", "b1", "B02x", "10", "9"];
        v.sort_by(|a, b| natural(a, b));
        assert_eq!(v, ["9", "10", "b1", "B2", "B02x", "B10"]);
    }

    /// Thickness, area and length (decision 0086) on B10 (seam U on benches 110 and 100, 2 m and 3 m over 1,000 m², and
    /// L, 1.5 m over 1,200 m²) and B2 (U, 4 m over 3,000 m²), all written with no add-up rule: a bench its own; U in B10
    /// its benches stacked, 5 m over 1,000 m²; B10 every seam stacked, 6.5 m over its largest, 1,200 m²; the strip
    /// (6.5 × 1,200 + 4 × 3,000) ÷ 4,200 = 4.714 m over 4,200 m²; length a block's own, none for the strip.
    #[test]
    fn thickness_area_and_length_add_up_by_block() {
        let mut t = table();
        t.columns.push(TableColumn { name: "ROM\\Coal\\thickness".into(), unit: "m".into(), add_up: None, kind: zd_core::ColumnKind::Written, values: ColumnValues::Number(n(&[3.0, 2.0, 1.5, 4.0])) });
        t.columns.push(TableColumn { name: "ROM\\Coal\\area".into(), unit: "m2".into(), add_up: None, kind: zd_core::ColumnKind::Written, values: ColumnValues::Number(n(&[1_000.0, 1_000.0, 1_200.0, 3_000.0])) });
        t.columns.push(TableColumn { name: "ROM\\Coal\\length".into(), unit: "m".into(), add_up: None, kind: zd_core::ColumnKind::Written, values: ColumnValues::Number(n(&[100.0, 100.0, 100.0, 120.0])) });
        let at = |name: &str, leaves: &[usize]| add_up_column(&t, &format!("ROM\\Coal\\{name}"), leaves);
        assert_eq!(at("thickness", &[1]), Some(2.0), "a bench its own");
        assert_eq!((at("thickness", &[0, 1]), at("area", &[0, 1])), (Some(5.0), Some(1_000.0)), "U in B10, its benches stacked");
        assert_eq!((at("thickness", &[0, 1, 2]), at("area", &[0, 1, 2]), at("length", &[0, 1, 2])), (Some(6.5), Some(1_200.0), Some(100.0)), "B10");
        let strip = at("thickness", &[0, 1, 2, 3]).unwrap();
        assert!((strip - (6.5 * 1_200.0 + 4.0 * 3_000.0) / 4_200.0).abs() < 1e-12, "{strip}");
        assert_eq!((at("area", &[0, 1, 2, 3]), at("length", &[0, 1, 2, 3])), (Some(4_200.0), None), "the strip");
    }

    /// Width and dip (decision 0086), a block's own and area-weighted over blocks: B10 50 m wide dipping 4° over
    /// 1,200 m², B2 30 m dipping 8° over 3,000 m²; the strip (50 × 1,200 + 30 × 3,000) ÷ 4,200 = 35.71 m and
    /// (4 × 1,200 + 8 × 3,000) ÷ 4,200 = 6.857°.
    #[test]
    fn width_and_dip_are_a_block_s_own_area_weighted_above() {
        let mut t = table();
        t.columns.push(TableColumn { name: "ROM\\Coal\\area".into(), unit: "m2".into(), add_up: None, kind: zd_core::ColumnKind::Written, values: ColumnValues::Number(n(&[1_000.0, 1_000.0, 1_200.0, 3_000.0])) });
        t.columns.push(TableColumn { name: "ROM\\Coal\\width".into(), unit: "m".into(), add_up: None, kind: zd_core::ColumnKind::Written, values: ColumnValues::Number(n(&[50.0, 50.0, 50.0, 30.0])) });
        t.columns.push(TableColumn { name: "ROM\\Coal\\dip".into(), unit: "deg".into(), add_up: None, kind: zd_core::ColumnKind::Written, values: ColumnValues::Number(n(&[4.0, 4.0, 4.0, 8.0])) });
        let at = |name: &str, leaves: &[usize]| add_up_column(&t, &format!("ROM\\Coal\\{name}"), leaves).unwrap();
        assert_eq!((at("width", &[0, 1, 2]), at("dip", &[0, 1, 2])), (50.0, 4.0), "B10 its own");
        assert!((at("width", &[0, 1, 2, 3]) - (50.0 * 1_200.0 + 30.0 * 3_000.0) / 4_200.0).abs() < 1e-12);
        assert!((at("dip", &[0, 1, 2, 3]) - (4.0 * 1_200.0 + 8.0 * 3_000.0) / 4_200.0).abs() < 1e-12);
    }

    /// Blocks coloured by a reserves field (decision 0075, point 7): each solid its leaf's ROM coal volume (B10's U bench
    /// 100 1,000 m³, bench 110 3,000 m³, its L 2,000 m³), or by block B10's 6,000 m³ for every solid of B10; a solid of
    /// another case, or on no leaf of the table, left out.
    #[test]
    fn a_field_s_value_for_each_solid_by_leaf_or_by_block() {
        let t = table();
        let attrs = |block: &str, seam: &str, bench: &str, case: &str| -> Attributes {
            let mut a: Attributes = [("strip".to_string(), AttrValue::Number(1.0)), ("block".to_string(), AttrValue::Text(block.into())), ("seam".to_string(), AttrValue::Text(seam.into()))].into_iter().collect();
            if !bench.is_empty() {
                a.insert("bench".into(), AttrValue::Text(bench.into()));
            }
            if !case.is_empty() {
                a.insert("case".into(), AttrValue::Text(case.into()));
            }
            a
        };
        let ids: Vec<zd_core::ObjectId> = (0..6).map(|_| zd_core::ObjectId::new()).collect();
        let solids = [
            attrs("B10", "U", "100", ""),
            attrs("B10", "U", "110", ""),
            attrs("B10", "L", "", ""),
            attrs("B2", "U", "", ""),
            attrs("B10", "U", "100", "Pit shell"),
            attrs("B99", "U", "", ""),
        ];
        let table_attrs = Attributes::new();
        let given = || ids.iter().copied().zip(solids.iter());
        let by_leaf = values_by_solid(&t, &table_attrs, "ROM\\Coal\\volume", false, given());
        assert_eq!(by_leaf, [(ids[0], 1_000.0), (ids[1], 3_000.0), (ids[2], 2_000.0), (ids[3], 4_000.0)]);
        let by_block = values_by_solid(&t, &table_attrs, "ROM\\Coal\\volume", true, given());
        assert_eq!(by_block, [(ids[0], 6_000.0), (ids[1], 6_000.0), (ids[2], 6_000.0), (ids[3], 4_000.0)]);
    }
}
