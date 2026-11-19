//! Where a solid sits in a reserves table (decisions 0075 and 0090): the table's levels, the words a solid's attributes
//! give them, the leaves a filter keeps, and the solids on them. Here, not in `zd-commands`, so a project can work out
//! what a reserves table's filter leaves out of the view as it works out a layer's.

use std::collections::{BTreeMap, HashSet};

use crate::{AttrValue, Attributes, BASE_CASE, ColumnValues, ObjectId, Table, attribute_ci, attribute_word, list_fits};

/// The table's level columns, top down: its text columns outside any folder, but the case it's of.
pub fn levels(table: &Table) -> Vec<String> {
    table
        .columns
        .iter()
        .filter(|c| !c.name.contains('\\') && !c.name.eq_ignore_ascii_case("case") && matches!(c.values, ColumnValues::Text(_)))
        .map(|c| c.name.clone())
        .collect()
}

/// The defaults a run gave levels a block has no value for (pit `Pit`), from the table's `levels` attribute
/// (`pit=Pit;strip;block;seam;bench`). A table written before it has none.
pub fn level_defaults(attributes: &Attributes) -> Vec<(String, String)> {
    let Some(AttrValue::Text(levels)) = attributes.get("levels") else { return Vec::new() };
    levels.split(';').filter_map(|l| l.split_once('=')).map(|(a, d)| (a.to_string(), d.to_string())).collect()
}

/// The words a solid's attributes give a table's levels, top down, as the run placed it: its own word, else the level's
/// default, else none ("").
pub fn leaf_of(levels: &[String], defaults: &[(String, String)], attributes: &Attributes) -> Vec<String> {
    levels
        .iter()
        .map(|l| {
            attribute_ci(attributes, l)
                .map(attribute_word)
                .filter(|w| !w.is_empty())
                .or_else(|| defaults.iter().find(|(a, _)| a.eq_ignore_ascii_case(l)).map(|(_, d)| d.clone()))
                .unwrap_or_default()
        })
        .collect()
}

/// The case a solid or table is of: its `case` attribute, else Base.
pub fn case_of(attributes: &Attributes) -> String {
    attribute_ci(attributes, "case").map(attribute_word).filter(|w| !w.is_empty()).unwrap_or_else(|| BASE_CASE.to_string())
}

/// The leaves (rows) a filter keeps, in order: every leaf with no filter. A filter names the table's columns as attributes,
/// a level (`seam: U, M`, `strip: 1-5`) or a field by its folders (`Insitu\Coal\thickness: <0.3`), the name's case aside;
/// a number is compared as one, words as words, as every filter reads them (decision 0088). A column the table hasn't is
/// a value that isn't there.
pub fn rows_fitting(table: &Table, when: &BTreeMap<String, String>) -> Vec<usize> {
    let lists: Vec<(Option<&ColumnValues>, &str)> = when
        .iter()
        .filter(|(_, list)| !list.trim().is_empty())
        .map(|(key, list)| {
            let column = table.column(key).or_else(|| table.columns.iter().find(|c| c.name.eq_ignore_ascii_case(key)));
            (column.map(|c| &c.values), list.as_str())
        })
        .collect();
    (0..table.rows())
        .filter(|&i| {
            lists.iter().all(|(values, list)| {
                let value = match values {
                    Some(ColumnValues::Number(v)) => v[i].filter(|x| x.is_finite()).map(AttrValue::Number),
                    Some(ColumnValues::Text(v)) => v[i].clone().filter(|w| !w.is_empty()).map(AttrValue::Text),
                    None => None,
                };
                list_fits(list, value.as_ref())
            })
        })
        .collect()
}

/// Whether a solid is waste by its `material`, as the seam cut writes it: anything else is coal.
pub fn is_waste(attributes: &Attributes) -> bool {
    attribute_ci(attributes, "material").map(attribute_word).is_some_and(|m| m.eq_ignore_ascii_case("waste"))
}

/// A reserves table's leaves as the solids find them: every leaf it holds and those a filter keeps, by their levels'
/// words.
pub struct TableLeaves {
    levels: Vec<String>,
    defaults: Vec<(String, String)>,
    case: String,
    held: HashSet<Vec<String>>,
    /// None: every leaf held is kept.
    kept: Option<HashSet<Vec<String>>>,
}

impl TableLeaves {
    /// The table's leaves, and those `when` keeps (decision 0088's syntax; empty keeps them all).
    pub fn new(table: &Table, attributes: &Attributes, when: &BTreeMap<String, String>) -> Self {
        let levels = levels(table);
        let words: Vec<Option<&Vec<Option<String>>>> = levels
            .iter()
            .map(|l| match table.column(l).map(|c| &c.values) {
                Some(ColumnValues::Text(v)) => Some(v),
                _ => None,
            })
            .collect();
        let row = |i: usize| -> Vec<String> { words.iter().map(|v| v.and_then(|v| v[i].clone()).unwrap_or_default()).collect() };
        let held = (0..table.rows()).map(row).collect();
        let kept = when.values().any(|l| !l.trim().is_empty()).then(|| rows_fitting(table, when).into_iter().map(row).collect());
        TableLeaves { defaults: level_defaults(attributes), case: case_of(attributes), levels, held, kept }
    }

    /// The solid's leaf in the table, if it's of the table's case and the table holds that leaf.
    pub fn leaf(&self, attributes: &Attributes) -> Option<Vec<String>> {
        if case_of(attributes) != self.case {
            return None;
        }
        let leaf = leaf_of(&self.levels, &self.defaults, attributes);
        self.held.contains(&leaf).then_some(leaf)
    }

    /// Whether the filter keeps this leaf.
    pub fn keeps(&self, leaf: &[String]) -> bool {
        self.kept.as_ref().is_none_or(|k| k.contains(leaf))
    }

    /// The solids on leaves the table holds and the filter keeps, under `node` (the levels' words from the top).
    pub fn solids<'a>(&self, node: &[String], solids: impl IntoIterator<Item = (ObjectId, &'a Attributes)>) -> Vec<ObjectId> {
        solids.into_iter().filter(|(_, a)| self.leaf(a).is_some_and(|leaf| leaf.starts_with(node) && self.keeps(&leaf))).map(|(id, _)| id).collect()
    }
}
