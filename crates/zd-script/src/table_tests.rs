//! Known answers for table scripts (decision 0099, workstream 7) on made-up tables, with a plain `TableWork` that sums and
//! groups in the order met: what the handle does itself (filters, joins, columns, what a run gives back and refuses).
//! The engine's own rules (ash by tonnes, the stack's order) are tested where they live, in zd-commands.

use std::cmp::Ordering;
use std::sync::Arc;

use zd_core::{Table, TableColumn};
use zd_reserves::AddUp;

use super::*;

struct Sums;

impl TableWork for Sums {
    fn add_up(&self, table: &Table, column: &str, rows: &[usize]) -> Option<f64> {
        let c = table.column(column)?;
        let (Some(AddUp::Sum), zd_core::ColumnValues::Number(v)) = (&c.add_up, &c.values) else { return None };
        let x: Vec<f64> = rows.iter().filter_map(|&r| v[r]).collect();
        (!x.is_empty()).then(|| x.iter().sum())
    }

    fn groups(&self, table: &Table, by: &[String], rows: &[usize]) -> Vec<(Vec<String>, Vec<usize>)> {
        let mut out: Vec<(Vec<String>, Vec<usize>)> = Vec::new();
        for &r in rows {
            let words: Vec<String> = by.iter().map(|b| match &table.column(b).expect("by").values {
                zd_core::ColumnValues::Text(v) => v[r].clone().unwrap_or_default(),
                zd_core::ColumnValues::Number(_) => String::new(),
            }).collect();
            match out.iter_mut().find(|(w, _)| *w == words) {
                Some((_, rs)) => rs.push(r),
                None => out.push((words, vec![r])),
            }
        }
        out
    }

    fn order(&self, a: &str, b: &str) -> Ordering {
        a.cmp(b)
    }
}

/// Strip 1 blocks B1 and B2 and strip 2 block B3: waste 100, 300 and 50 m³ over coal 10, 50 and 0 t.
fn blocks() -> Table {
    let w = |v: &[&str]| v.iter().map(|s| Some(s.to_string())).collect::<Vec<_>>();
    let n = |v: &[f64]| v.iter().map(|&x| Some(x)).collect::<Vec<_>>();
    Table {
        columns: vec![
            TableColumn::text("strip", w(&["1", "1", "2"])),
            TableColumn::text("block", w(&["B1", "B2", "B3"])),
            TableColumn::adding("waste", "m3", AddUp::Sum, n(&[100.0, 300.0, 50.0])),
            TableColumn::adding("coal", "t", AddUp::Sum, n(&[10.0, 50.0, 0.0])),
        ],
        calculated: Vec::new(),
    }
}

/// A made-up Vulcan export of the same blocks, coal only, B2's 2 t over and B4 a block we haven't.
fn vulcan() -> Table {
    let w = |v: &[&str]| v.iter().map(|s| Some(s.to_string())).collect::<Vec<_>>();
    Table {
        columns: vec![
            TableColumn::text("block", w(&["B1", "B2", "B4"])),
            TableColumn::number("coal", "t", vec![Some(10.0), Some(52.0), Some(7.0)]),
        ],
        calculated: Vec::new(),
    }
}

fn run(script: &str) -> Result<TableRan, Problem> {
    let s = TableScript::compile(script, Default::default()).map_err(|p| p.into_iter().next().expect("a problem"))?;
    s.run(TableInput {
        name: "Base".into(),
        table: Arc::new(blocks()),
        tables: vec![("Base".into(), Arc::new(blocks())), ("Vulcan".into(), Arc::new(vulcan()))],
        parameters: values([("tolerance", ScriptValue::Number(1.0))]),
        work: Arc::new(Sums),
        lookups: Default::default(),
    })
}

fn numbers(t: &Table, name: &str) -> Vec<Option<f64>> {
    match &t.column(name).unwrap_or_else(|| panic!("no column {name}")).values {
        zd_core::ColumnValues::Number(v) => v.clone(),
        zd_core::ColumnValues::Text(_) => panic!("{name} holds words"),
    }
}

fn words(t: &Table, name: &str) -> Vec<Option<String>> {
    match &t.column(name).unwrap_or_else(|| panic!("no column {name}")).values {
        zd_core::ColumnValues::Text(v) => v.clone(),
        zd_core::ColumnValues::Number(_) => panic!("{name} holds numbers"),
    }
}

/// Grouped by strip, the ratio from the strip's own sums: strip 1 is 400 / 60, never the average of 10 and 6.
#[test]
fn a_table_script_groups_and_works_a_ratio_from_the_sums() {
    let ran = run(r#"
def run(t):
    s = t.group("strip")
    ratio = [w / c if c else None for w, c in zip(s["waste"], s["coal"])]
    return s.with_column("strip_ratio", ratio, unit="m3/t").select(["strip", "waste", "coal", "strip_ratio"])
"#)
    .unwrap();
    assert_eq!(words(&ran.table, "strip"), [Some("1".into()), Some("2".into())]);
    assert_eq!(numbers(&ran.table, "waste"), [Some(400.0), Some(50.0)]);
    let r = numbers(&ran.table, "strip_ratio");
    assert!((r[0].unwrap() - 400.0 / 60.0).abs() < 1e-12, "{r:?}");
    assert_eq!(r[1], None, "strip 2 has no coal");
    assert_eq!(ran.table.column("strip_ratio").map(|c| c.unit.as_str()), Some("m3/t"));
    // The words it didn't group by agree only within a block: none at the strip.
    assert!(ran.notes.is_empty(), "{:?}", ran.notes);
}

/// Joined to the Vulcan export on block, the differences beyond the tolerance listed: B2 by 2 t, B3 not in Vulcan, and
/// with an outer join B4 not in ours; B1 agrees and isn't.
#[test]
fn a_join_lists_each_difference_beyond_tolerance() {
    let ran = run(r#"
def run(t, p):
    j = t.join(table("Vulcan"), on="block", prefix="vulcan ", how="outer")
    out = []
    for block, ours, theirs in zip(j["block"], j["coal"], j["vulcan coal"]):
        if ours == None or theirs == None:
            out.append((block, None, "in one only"))
        elif abs(ours - theirs) > p.tolerance:
            out.append((block, theirs - ours, "differs"))
    return new_table({"block": [o[0] for o in out], "difference": [o[1] for o in out], "why": [o[2] for o in out]}, units={"difference": "t"}, name="check")
"#)
    .unwrap();
    assert_eq!(words(&ran.table, "block"), [Some("B2".into()), Some("B3".into()), Some("B4".into())]);
    assert_eq!(numbers(&ran.table, "difference"), [Some(2.0), None, None]);
    assert_eq!(words(&ran.table, "why"), [Some("differs".into()), Some("in one only".into()), Some("in one only".into())]);
    assert_eq!(ran.read, ["Vulcan"], "the tables it read by name");
    assert!(ran.notes.iter().any(|n| n == "1 of Base's 3 rows had no row in Vulcan with the same block"), "{:?}", ran.notes);
}

/// A filter in the grid's words, a sort, a total, a row; a column with no rule left empty in a group, said.
#[test]
fn filters_sorts_totals_and_rows_are_rust_s() {
    let ran = run(r#"
def run(t):
    big = t.where({"waste": ">90"}).sort("coal", descending=True)
    print(big.total("coal"), big.row(0)["block"], len(big), big.rows, big.columns[0], big.unit("coal"))
    return big.with_column("note", ["a", None]).with_column("plain", [1, 2]).group("strip")
"#)
    .unwrap();
    assert_eq!(ran.said, ["60.0 B2 2 2 strip t"]);
    assert_eq!(numbers(&ran.table, "plain"), [None], "no rule: empty in the group");
    assert_eq!(words(&ran.table, "note"), [None], "the group's notes disagree");
    assert!(ran.notes.iter().any(|n| n.starts_with("plain has no rule to add up by")), "{:?}", ran.notes);
}

/// What a run refuses, each in words with its line where it has one.
#[test]
fn a_table_script_s_mistakes_are_said() {
    let line = |script: &str| run(script).unwrap_err();
    assert!(line("def formula(unit):\n    pass\n").message.contains("def run(t, p):"));
    let wrong = line("def run(t):\n    return t.group(\"stripp\")\n");
    assert!(wrong.message.contains("no column stripp") && wrong.message.contains("did you mean strip"), "{wrong:?}");
    assert_eq!(wrong.line, Some(2));
    assert!(line("def run(t):\n    x = 1\n").message.contains("is a return missing?"));
    assert!(line("def run(t):\n    return t.join(table(\"Vulcan\"), on=\"block\")\n").message.contains("give the join a prefix"));
    assert!(line("def run(t):\n    return t.where(seam=\"U\")\n").message.contains("no column seam to filter on"));
    assert!(line("def run(t):\n    return t.with_column(\"x\", [1])\n").message.contains("1 values for the table's 3 rows"));
    assert!(line("def run(t):\n    return t.with_column(\"x\", [1, \"a\", 2])\n").message.contains("mixes words and 1"));
    assert!(line("def run(t):\n    return t.with_column(\"x\", [1, 2, 3], adds=\"average\")\n").message.contains("isn't a way to add up"));
    assert!(line("def run(t):\n    return t.frobnicate()\n").message.contains("a table has no `frobnicate`"));
    assert!(line("def run(t):\n    return table(\"Minex\")\n").message.contains("no table named Minex"));
    // A formula has no tables to read.
    let f = Formula::compile("def formula(unit):\n    unit.volume = len(table(\"Base\"))\n", Declared { fields: vec![FieldDecl::number("volume", "m3")], ..Default::default() }).unwrap();
    let column = ColumnInput { units: vec![UnitInput { fields: values([]), block: values([]), parameters: values([]), section: None, rom_coal: None }], ..Default::default() };
    assert!(f.run(&column, &[0]).unwrap_err().message.contains("only by a table script"));
}

#[test]
fn add_up_words_read_as_planners_write_them() {
    assert_eq!(add_up_from_words("sum"), Ok(AddUp::Sum));
    assert_eq!(add_up_from_words(" By Volume "), Ok(AddUp::Volume));
    assert_eq!(add_up_from_words("mass of rd_is"), Ok(AddUp::Mass { density: "rd_is".into() }));
    assert_eq!(add_up_from_words("ratio of ROM\\Waste\\volume per ROM\\Coal\\tonnes"), Ok(AddUp::Ratio { of: "ROM\\Waste\\volume".into(), per: "ROM\\Coal\\tonnes".into() }));
    assert!(add_up_from_words("ratio of waste").is_err());
    assert!(is_table_script("def run(t, p):\n    return t\n") && !is_table_script("def formula(unit):\n    pass\n"));
}

/// Every table script in the reference compiles, and the strip share example gives each strip's leaves shares adding to
/// 1: 10 and 30 t in strip 1 are 0.25 and 0.75, strip 2's 5 t is 1.
#[test]
fn every_table_script_in_the_reference_compiles_and_the_shares_add_to_one() {
    let examples: Vec<&str> = REFERENCE.split("```py").skip(1).filter(|b| b.starts_with('\n') || b.starts_with("\r\n")).map(|b| b.split("```").next().unwrap_or("")).collect();
    assert!(examples.len() >= 3, "{} table script examples", examples.len());
    for e in &examples {
        if let Err(p) = TableScript::compile(e, Default::default()) {
            panic!("a reference table script doesn't compile: {p:?}\n{e}");
        }
    }
    let share = examples.iter().find(|e| e.contains("strip_coal_share")).expect("the share example");
    let w = |v: &[&str]| v.iter().map(|s| Some(s.to_string())).collect::<Vec<_>>();
    let t = Table {
        columns: vec![TableColumn::text("strip", w(&["1", "1", "2"])), TableColumn::adding("ROM\\Coal\\tonnes", "t", AddUp::Sum, vec![Some(10.0), Some(30.0), Some(5.0)])],
        calculated: Vec::new(),
    };
    let ran = TableScript::compile(share, Default::default()).unwrap().run(TableInput { name: "Base".into(), table: Arc::new(t), tables: Vec::new(), parameters: values([]), work: Arc::new(Sums), lookups: Default::default() }).unwrap();
    assert_eq!(numbers(&ran.table, "strip_coal_share"), [Some(0.25), Some(0.75), Some(1.0)]);
}
