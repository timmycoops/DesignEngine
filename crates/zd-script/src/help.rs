//! What a script can call, as the editors offer it while it's typed (completions and hover help): each name, how it's
//! called and what it gives, in a planner's words. Kept beside what runs them, and tested against it, so the editor never
//! offers a call the engine hasn't or misses one it has.

/// A call a script can make: its name, how it's called, and what it gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call {
    pub name: &'static str,
    pub signature: &'static str,
    pub what: &'static str,
}

const fn call(name: &'static str, signature: &'static str, what: &'static str) -> Call {
    Call { name, signature, what }
}

/// A table's own calls and values (`t.group(...)`, `t.rows`), as a table script holds it (decision 0099, workstream 7).
pub const TABLE_CALLS: &[Call] = &[
    call("where", "t.where({\"seam\": \"U, M\"}) or t.where(material=\"coal\")", "the rows a filter keeps, in the grid's words: lists, ranges 1-5, <0.3, >90"),
    call("group", "t.group([\"strip\", \"seam\"])", "a row a group, in the grid's order; each number added up by its own rule (tonnes summed, RD by volume, ash by tonnes, a ratio of sums)"),
    call("total", "t.total(\"ROM\\\\Coal\\\\tonnes\")", "one number: the column added up over every row by its rule; None where no row has one"),
    call("join", "t.join(other, on=[\"strip\", \"block\"], prefix=\"vulcan \", how=\"left\")", "each row with the other table's columns beside it, matched on the `on` words; how=\"outer\" adds the other's unmatched rows"),
    call("with_column", "t.with_column(\"name\", values, unit=\"t\", adds=\"sum\")", "the table with a column added or one of its own replaced, a value a row; adds: sum, max, min, volume, \"mass of rd\", \"ratio of <column> per <column>\""),
    call("select", "t.select([\"strip\", \"ROM\\\\Coal\\\\tonnes\"])", "only these columns, in this order"),
    call("drop", "t.drop([\"Summary\\\\Audit\\\\run\"])", "every column but these"),
    call("rename", "t.rename({\"old\": \"new\"})", "columns renamed"),
    call("sort", "t.sort(\"ROM\\\\Coal\\\\tonnes\", descending=True)", "the rows in a column's order, words as the grid orders them (B2 before B10), empty cells last"),
    call("row", "t.row(0)", "one row as a dict of its columns, from 0"),
    call("unit", "t.unit(\"ROM\\\\Coal\\\\tonnes\")", "a column's unit, \"\" where it has none"),
    call("rows", "t.rows", "how many rows (as len(t))"),
    call("columns", "t.columns", "the columns' names, in order"),
    call("name", "t.name", "the table's name"),
];

/// The functions only a table script has.
pub const TABLE_GLOBALS: &[Call] = &[
    call("table", "table(\"Vulcan check\")", "another table of the project, by its name"),
    call("new_table", "new_table({\"block\": blocks, \"tonnes\": tonnes}, units={\"tonnes\": \"t\"}, adds={\"tonnes\": \"sum\"}, name=\"check\")", "a table made from lists, a column each, all as long"),
    call("check", "check(\"waste processes\", t[\"ROM\\\\Waste\\\\dragline_bcm\"], t[\"ROM\\\\Waste\\\\truck_bcm\"], equals=t[\"ROM\\\\Waste\\\\volume\"], tolerance=0.01)", "a mine's own check: the parts added up on every leaf against equals, the difference written to Summary\\Checks\\<name>, each leaf out by more than the tolerance named"),
    call("required", "required(\"ROM\\\\Waste\\\\dragline_bcm\", material=\"waste\", seam=\"U, M\")", "every leaf (of that material, and the filter's) has a value in the column; each with none named and counted in Summary\\Checks"),
];

/// The library every script has (decision 0077), written once in Rust.
pub const LIBRARY_CALLS: &[Call] = &[
    call("in_situ_rd", "in_situ_rd(rd, mad, mis)", "in situ density from an air-dried RD and the moistures (%), by Preston and Sanders"),
    call("air_dried_rd", "air_dried_rd(rd_is, mad, mis)", "the air-dried RD back from an in situ density"),
    call("moisture_factor", "moisture_factor(from, to)", "(100 − to) ÷ (100 − from): what a value on one moisture basis is multiplied by to put it on another"),
    call("to_basis", "to_basis(value, from, to)", "a value on the `from` moisture basis put on the `to` basis"),
    call("weighted", "weighted(values, weights)", "an average weighted by the weights; None where they add to nothing"),
    call("abs", "abs(x)", "the size of a number, never negative"),
    call("round", "round(x, digits)", "a number rounded to `digits` decimals (none: a whole number)"),
    call("sqrt", "sqrt(x)", "the square root; refused for a negative number"),
    call("clamp", "clamp(x, low, high)", "a number held between low and high"),
    call("lookup", "lookup(\"Rates\", \"bcm_per_hour\", equipment=\"DL1\", bench_height=unit.thickness, default=0)", "a parameter table's value by its name and the keys given: the first row that fits them; default= where none does"),
    call("interpolate", "interpolate(\"Yield curve\", unit.ash, x=\"ash\", y=\"yield\", seam=unit.seam, ends=\"hold\")", "a curve read off a parameter table: the rows that fit the keys in order of x, linear between; outside them refused, or the end held with ends=\"hold\""),
];

/// What a formula's unit holds besides its fields (decision 0077): read only, each a set of values.
pub const UNIT_PARTS: &[Call] = &[
    call("section", "unit.section.volume", "its working section's fields added up; None outside a section"),
    call("column", "unit.column.volume", "the whole column's fields added up"),
    call("block", "unit.block.strip", "its block's attributes"),
    call("above", "unit.above.area", "the unit above's fields; None for the top unit"),
    call("rom_coal", "unit.rom_coal.mad", "its ROM coal section's coal added up (after rom); None where it's in none"),
    call("place_onto", "unit.place_onto(\"Waste\\\\Drill and Blast\\\\Volume\", x, seam=unit.blasting_seam, bench=unit.blasting_bench)", "a schedule field's value written on another leaf of the block (the levels named, the rest the unit's own), not the unit's; where that leaf has nothing scheduled it's said and not written"),
];

/// What an allocation step's column offers (Waste allocation plan, decision 4): its units and the calls that read them.
pub const COLUMN_CALLS: &[Call] = &[
    call("units", "column.units", "every unit of the column, top down"),
    call("waste", "column.waste()", "the units that end in ROM waste, top down: waste, losses and discarded coal"),
    call("coal", "column.coal()", "the units that end in a ROM coal section, top down: its coal, partings and dilution"),
    call("after", "column.after(unit)", "the unit below this one; None for the last"),
    call("before", "column.before(unit)", "the unit above this one; None for the first"),
    call("group_while", "column.group_while(\"process\", \"zone\", units=column.waste())", "runs of neighbouring units with the same values of these fields, words or labels: a list of lists"),
    call("total", "column.total(\"volume\", units=[a, b])", "a field over units (every unit by default) added up by its own rule"),
    call("name", "column.name", "the column's words: Pit/3/B1"),
    call("block", "column.block.strip", "the column's own attributes, its levels above the seam"),
];

/// What a unit of an allocation step offers besides its fields, words and labels (decisions 3 and 5).
pub const ALLOCATION_UNIT: &[Call] = &[
    call("split_thickness", "top, rest = unit.split_thickness(p.topsoil_thickness)", "pieces of these thicknesses off the top, then the rest; a list one longer than asked, a piece of nothing None; the pieces add back to the unit exactly"),
    call("split_shares", "cast, dozer, dragline = unit.split_shares([0.2, 0.3, 0.5])", "the unit split by volume shares adding to 1, in order; a share of 0 None; the last piece is what's left, so they add back exactly"),
    call("place_onto", "unit.place_onto(\"Waste\\\\Drill and Blast\\\\Volume\", x, seam=unit.blasting_seam)", "a schedule field's value written on another leaf of the block, not the unit's"),
    call("block", "unit.block.strip", "its block's attributes"),
    call("p", "unit.p.min_burden", "the parameters for the unit, by its block's attributes"),
];

/// Starlark's own functions and words a script uses most, offered as they're typed.
pub const STARLARK: &[Call] = &[
    call("len", "len(x)", "how many items, or a table's rows"),
    call("range", "range(n)", "0 to n − 1"),
    call("zip", "zip(a, b)", "items of lists side by side: for w, c in zip(waste, coal)"),
    call("enumerate", "enumerate(items)", "each item with its place: for i, x in enumerate(items)"),
    call("sorted", "sorted(items, reverse=False)", "a list in order"),
    call("reversed", "reversed(items)", "a list back to front"),
    call("min", "min(a, b)", "the smallest"),
    call("max", "max(a, b)", "the largest"),
    call("any", "any(items)", "whether any is true"),
    call("all", "all(items)", "whether every one is true"),
    call("getattr", "getattr(p, \"name\", default)", "a value by a name worked out as it runs, the default where it isn't there"),
    call("hasattr", "hasattr(unit, \"name\")", "whether a value has a name"),
    call("dir", "dir(p)", "the names a value has"),
    call("print", "print(\"text\", x)", "shown with the run, against the unit or step"),
    call("fail", "fail(\"why\")", "stops with these words: the block or run says why"),
    call("str", "str(x)", "a value as words"),
    call("int", "int(x)", "a whole number"),
    call("float", "float(x)", "a number"),
    call("list", "list(items)", "a list"),
    call("dict", "dict()", "a dict of names to values"),
    call("type", "type(x)", "what kind of value it is"),
    call("load", "load(\"script\", \"function\")", "a function from a script of the project, by the script's name"),
];

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{ColumnInput, Declared, FieldDecl, Formula, ScriptValue, TableInput, TableScript, UnitInput, values};

    /// The table calls offered are exactly what a table holds: `dir(t)` names every one, and nothing more.
    #[test]
    fn the_table_calls_offered_are_the_table_s_own() {
        struct NoWork;
        impl crate::TableWork for NoWork {
            fn add_up(&self, _: &zd_core::Table, _: &str, _: &[usize]) -> Option<f64> {
                None
            }
            fn groups(&self, _: &zd_core::Table, _: &[String], _: &[usize]) -> Vec<(Vec<String>, Vec<usize>)> {
                Vec::new()
            }
            fn order(&self, a: &str, b: &str) -> std::cmp::Ordering {
                a.cmp(b)
            }
        }
        let script = TableScript::compile("def run(t):\n    print(\",\".join(sorted(dir(t))))\n    return t\n", Default::default()).unwrap();
        let ran = script.run(TableInput { name: "t".into(), table: Arc::new(zd_core::Table::default()), tables: Vec::new(), parameters: values([]), work: Arc::new(NoWork), lookups: Default::default() }).unwrap();
        let mut offered: Vec<&str> = TABLE_CALLS.iter().map(|c| c.name).collect();
        offered.sort_unstable();
        assert_eq!(ran.said, [offered.join(",")]);
        // The table script's own functions are there to call.
        assert!(TableScript::compile("def run(t):\n    return [table, new_table] and t\n", Default::default()).is_ok());
    }

    /// Every library function and Starlark word offered is one a formula can call.
    #[test]
    fn the_functions_offered_are_there() {
        let names: Vec<&str> = LIBRARY_CALLS.iter().chain(STARLARK).map(|c| c.name).filter(|n| *n != "load").collect();
        let source = format!("def formula(unit):\n    unit.volume = len([{}])\n", names.join(", "));
        let f = Formula::compile(&source, Declared { fields: vec![FieldDecl::number("volume", "m3")], ..Default::default() }).unwrap();
        let column = ColumnInput { units: vec![UnitInput { fields: values([]), block: values([]), parameters: values([]), section: None, rom_coal: None }], ..Default::default() };
        assert_eq!(f.run(&column, &[0]).unwrap()[0].get("volume"), Some(&ScriptValue::Number(names.len() as f64)));
        for p in UNIT_PARTS {
            assert!(Formula::compile(&format!("def formula(unit):\n    x = unit.{}\n", p.name), Declared::default()).is_ok(), "{}", p.name);
        }
    }
}
