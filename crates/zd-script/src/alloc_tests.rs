//! Allocation scripts on a made-up column with known answers (the Waste allocation plan, workstream 3): splits by
//! thickness from the top and by shares, the pieces adding back exactly, words set on each piece, runs grouped, and what
//! a script may not do refused with why.
//!
//! | unit | material | seam | m³     | m  | m²    |
//! |------|----------|------|--------|----|-------|
//! | OB   | waste    | C    | 90,000 | 30 | 3,000 |
//! | C    | coal     | C    | 6,000  | 2  | 3,000 |
//! | IB   | waste    | D    | 30,000 | 10 | 3,000 |

use std::collections::HashMap;
use std::sync::Arc;

use zd_reserves::{AddUp, Column, ColumnKey, Field, FieldKind, Fields, Labels, Origin};

use crate::{Allocation, AllocationInput, Declared, Ends, FieldDecl, ScriptValue, values};

fn fields() -> Arc<Fields> {
    Arc::new(
        Fields::new(vec![
            Field::new("volume", FieldKind::Volume, AddUp::Sum),
            Field::new("thickness", FieldKind::Thickness, AddUp::Sum),
            Field::new("area", FieldKind::Area, AddUp::Max),
            Field::new("Waste\\Excavator\\Prime Volume", FieldKind::Other, AddUp::Sum),
        ])
        .unwrap(),
    )
}

fn declared() -> Declared {
    Declared {
        fields: vec![
            FieldDecl::number("volume", "m3"),
            FieldDecl::number("thickness", "m"),
            FieldDecl::number("area", "m2"),
            FieldDecl::number("Waste\\Excavator\\Prime Volume", "m3"),
            FieldDecl::text("process"),
        ],
        labels: ["seam", "material", "bench", "became"].map(String::from).to_vec(),
        parameters: vec![FieldDecl::number("topsoil", "m"), FieldDecl::number("freedig", "m"), FieldDecl::number("wedge", "m")],
        ..Declared::default()
    }
}

const N: f64 = f64::NAN;

/// The column, OB over coal C over IB, each unit's ending as `final` would write it.
fn input(topsoil: f64) -> AllocationInput {
    let mut c = Column::new(ColumnKey::new("Base", &["5", "B1"]), fields());
    for (stack, name, material, seam, v, t) in [(0, "OB", "waste", "C", 90_000.0, 30.0), (1, "C", "coal", "C", 6_000.0, 2.0), (2, "IB", "waste", "D", 30_000.0, 10.0)] {
        let origin = Origin { object: format!("solid-{name}"), name: format!("5_B1_{name}") };
        c.push_input(origin, stack, Labels::new(material, seam), vec![v, t, 3_000.0, N]).unwrap();
    }
    let ends: HashMap<u32, Ends> = c.units().iter().map(|u| (u.id, if u.in_situ.material == "coal" { Ends::Coal } else { Ends::Waste })).collect();
    let p = values([("topsoil", ScriptValue::Number(topsoil)), ("freedig", ScriptValue::Number(4.5)), ("wedge", ScriptValue::Number(3.0))]);
    AllocationInput {
        column: c,
        name: "5/B1".into(),
        blocks: vec![values([("strip", ScriptValue::Text("5".into()))]); 3],
        parameters: vec![p.clone(); 3],
        column_block: values([("strip", ScriptValue::Text("5".into()))]),
        column_parameters: p,
        ends,
    }
}

/// Spry's loose ground and wedge, every thickness from the column's parameters: topsoil, then freedig, off the top of
/// each waste over coal; a wedge off the bottom; Excavator the rest.
const LOOSE_GROUND: &str = r#"
def allocate(column, p):
    for u in column.waste():
        below = column.after(u)
        if below == None or below.material != "coal":
            u.process = "Excavator"
            continue
        topsoil, freedig, rest = u.split_thickness(p.topsoil, p.freedig)
        topsoil.process = "Topsoil"
        freedig.process = "Freedig"
        excavator, wedge = rest.split_thickness(rest.thickness - p.wedge)
        excavator.process = "Excavator"
        excavator["Waste\\Excavator\\Prime Volume"] = excavator.volume
        wedge.process = "Wedge"
"#;

fn by_process(a: &crate::Allocated) -> Vec<(String, f64, f64)> {
    let c = &a.column;
    c.units().iter().map(|u| (u.word("process").unwrap_or("").to_string(), u.value(0), u.value(1))).collect()
}

#[test]
fn loose_ground_and_a_wedge_are_split_off_by_thickness_and_add_back_exactly() {
    let alloc = Allocation::compile(LOOSE_GROUND, declared()).unwrap();
    let before = input(0.5).column.balance();
    let a = alloc.run(input(0.5)).unwrap();
    let got = by_process(&a);
    let want = [("Topsoil", 1_500.0, 0.5), ("Freedig", 13_500.0, 4.5), ("Excavator", 66_000.0, 22.0), ("Wedge", 9_000.0, 3.0), ("", 6_000.0, 2.0), ("Excavator", 30_000.0, 10.0)];
    assert_eq!(got.len(), want.len(), "{got:?}");
    for ((p, v, t), (wp, wv, wt)) in got.iter().zip(want) {
        assert_eq!(p, wp);
        assert!((v - wv).abs() <= 1e-9 * wv && (t - wt).abs() <= 1e-9 * wt, "{p}: {v} m3, {t} m against {wv}, {wt}");
    }
    // The pieces add back to the unit: the column's summed fields as they were.
    let after = a.column.balance();
    assert!((after[0] - before[0]).abs() <= 1e-9 && (after[1] - before[1]).abs() <= 1e-12, "{after:?} against {before:?}");
    let ob: f64 = got[..4].iter().map(|g| g.1).sum();
    assert!((ob - 90_000.0).abs() <= 1e-9, "OB's pieces add to {ob}");
    // A field set on a piece, the others' left as they were.
    assert!((a.column.units()[2].value(3) - 66_000.0).abs() <= 1e-9 && a.column.units()[0].value(3).is_nan());
    // Each split noted with how, and the column's parameters it read.
    assert_eq!(a.splits.iter().map(|s| s.how.as_str()).collect::<Vec<_>>(), ["thickness 0.5, 4.5", "thickness 22"]);
    assert_eq!(a.splits[0].pieces.len(), 3);
    assert_eq!(a.read, [(None, "topsoil".to_string()), (None, "freedig".to_string()), (None, "wedge".to_string())]);
}

/// Decision 8: the same script, the row changed from 0.5 m to 1 m of topsoil; the script is unchanged.
#[test]
fn a_changed_parameter_row_changes_the_split_and_not_the_script() {
    let a = Allocation::compile(LOOSE_GROUND, declared()).unwrap().run(input(1.0)).unwrap();
    let got = by_process(&a);
    for ((p, v, _), (wp, wv)) in got.iter().zip([("Topsoil", 3_000.0), ("Freedig", 13_500.0), ("Excavator", 64_500.0), ("Wedge", 9_000.0)]) {
        assert_eq!(p, wp);
        assert!((v - wv).abs() <= 1e-9 * wv, "{p}: {v} against {wv}");
    }
}

#[test]
fn shares_split_a_unit_in_order_and_the_last_piece_is_the_rest() {
    let script = r#"
def allocate(column, p):
    ob = column.units[0]
    cast, none, dozer, dragline = ob.split_shares([0.2, 0, 0.3, 0.5])
    if none != None:
        fail("a share of 0 is no piece")
    cast.process = "Cast"
    dozer.process = "Dozer Push"
    dragline.process = "Dragline"
    for group in column.group_while("process", units=column.units):
        print(len(group), group[0].process)
    print(column.total("volume", units=[cast, dozer]))
"#;
    let a = Allocation::compile(script, declared()).unwrap().run(input(0.5)).unwrap();
    let got = by_process(&a);
    for ((p, v, t), (wp, wv, wt)) in got.iter().zip([("Cast", 18_000.0, 6.0), ("Dozer Push", 27_000.0, 9.0), ("Dragline", 45_000.0, 15.0)]) {
        assert_eq!(p, wp);
        assert!((v - wv).abs() <= 1e-9 * wv && (t - wt).abs() <= 1e-9 * wt, "{p}: {v} m3, {t} m");
    }
    let ob: f64 = got[..3].iter().map(|g| g.1).sum();
    assert_eq!(ob, 90_000.0, "the last piece is what's left, so they add back exactly");
    assert_eq!(a.said, ["1 Cast", "1 Dozer Push", "1 Dragline", "2 None", "45000.0"]);
}

#[test]
fn what_an_allocation_script_may_not_do_is_refused_with_why() {
    let refused = |body: &str| {
        let script = format!("def allocate(column, p):\n    u = column.units[0]\n    {body}\n");
        Allocation::compile(&script, declared()).unwrap().run(input(0.5)).unwrap_err().message
    };
    assert_eq!(refused("u.split_thickness(20, 20)"), "split_thickness asks 40 m of C waste, which is 30 m thick");
    assert_eq!(refused("u.split_shares([0.5, 0.4])"), "split_shares' shares add up to 0.9, not 1");
    assert_eq!(refused("u.split_shares([1.5, -0.5])"), "split_shares takes shares of 0 or more, not -0.5");
    assert_eq!(refused("u.volume = 5"), "it sets volume on unit(C waste): the volume is of record; split the unit to share it");
    assert_eq!(refused("u.seam = \"D\""), "`seam` is a label of the unit, not a field a script sets");
    assert_eq!(refused("u.process = 3"), "`process` holds words, not int (3)");
    assert!(refused("column.group_while(\"proces\")").starts_with("`proces` isn't a field, word or label of this dataflow"));
    assert_eq!(refused("column.after(3)"), "after takes units of the column, not int (3)");
    let problems = Allocation::compile("def formula(unit, p):\n    pass\n", declared()).unwrap_err();
    assert!(problems[0].message.starts_with("an allocation step defines its function as `def allocate(column, p):`"));
}

#[test]
fn an_allocation_traced_shows_its_lines_reads_sets_and_the_column_it_left() {
    let alloc = Allocation::compile(LOOSE_GROUND, declared()).unwrap();
    let (trace, done) = alloc.trace(input(0.5));
    assert!(trace.stopped.is_none(), "{:?}", trace.stopped);
    let done = done.expect("it ran through");
    assert_eq!(by_process(&done)[0].0, "Topsoil");
    assert!(trace.lines.iter().any(|l| l.text.starts_with("topsoil, freedig, rest = u.split_thickness")));
    assert!(trace.events.iter().any(|e| matches!(e, crate::Event::Read { what, .. } if what == "p.topsoil")));
    assert!(trace.sets.iter().any(|(k, v)| k == "unit(C waste).process" && *v == ScriptValue::Text("Topsoil".into())), "{:?}", trace.sets);
}

/// Every allocation script in the reference compiles as one.
#[test]
fn every_allocation_script_in_the_reference_compiles() {
    let examples: Vec<&str> = crate::REFERENCE.split("```star").skip(1).map(|b| b.split("```").next().unwrap_or("")).collect();
    assert!(examples.len() >= 2, "{} allocation examples", examples.len());
    for e in &examples {
        if let Err(p) = Allocation::compile(e, declared()) {
            panic!("a reference allocation script doesn't compile: {p:?}\n{e}");
        }
    }
}

/// Every call an allocation editor offers on the column and on a unit is one they have: each read by `hasattr`.
#[test]
fn the_allocation_calls_offered_are_there() {
    let column: Vec<String> = crate::help::COLUMN_CALLS.iter().map(|c| format!("hasattr(column, \"{}\")", c.name)).collect();
    let unit: Vec<String> = crate::help::ALLOCATION_UNIT.iter().map(|c| format!("hasattr(u, \"{}\")", c.name)).collect();
    let script = format!("def allocate(column, p):\n    u = column.units[0]\n    print([{}])\n    print([{}])\n", column.join(", "), unit.join(", "));
    let a = Allocation::compile(&script, declared()).unwrap().run(input(0.5)).unwrap();
    let all_true = |n: usize| format!("[{}]", vec!["True"; n].join(", "));
    assert_eq!(a.said, [all_true(column.len()), all_true(unit.len())]);
}

/// A waste interval of two units over coal (WEATH 8 m over OB 42 m, 3,000 m²), then IB below: the interval is the run
/// of both, and the coal's its own.
fn two_horizons() -> AllocationInput {
    let mut c = Column::new(ColumnKey::new("Base", &["5", "B1"]), fields());
    for (stack, name, material, t) in [(0, "WEATH", "waste", 8.0), (1, "OB", "waste", 42.0), (2, "A", "coal", 2.0), (3, "IB", "waste", 10.0)] {
        let origin = Origin { object: format!("solid-{name}"), name: format!("5_B1_{name}") };
        c.push_input(origin, stack, Labels::new(material, name), vec![t * 3_000.0, t, 3_000.0, N]).unwrap();
    }
    let ends: HashMap<u32, Ends> = c.units().iter().map(|u| (u.id, if u.in_situ.material == "coal" { Ends::Coal } else { Ends::Waste })).collect();
    AllocationInput { column: c, ends, ..input(0.5) }
}

/// The waste interval over A is WEATH and OB, 50 m. Forty-five metres off its bottom takes OB whole (126,000 m³) and the
/// bottom 3 m of WEATH (9,000 m³), leaving WEATH's top 5 m; then 1 m off its top (3,000 m³), 4 m left. Every piece adds back
/// exactly; asking more than the units hold is refused.
#[test]
fn a_thickness_is_taken_across_the_units_of_a_waste_interval() {
    let script = r#"
def allocate(column, p):
    intervals = column.intervals()
    print([[u.seam for u in i] for i in intervals], [[u.seam for u in i] for i in column.intervals(ends="coal")])
    zone = column.take_from_bottom(intervals[0], 45)
    for u in zone:
        u.process = "Dragline"
    left = [u for u in intervals[0] if u not in zone]
    top = column.take_from_top(left, 1)
    top[0].process = "Topsoil"
    print(len(zone), column.total("thickness", units=zone), column.total("volume", units=left))
"#;
    let a = Allocation::compile(script, declared()).unwrap().run(two_horizons()).unwrap();
    assert_eq!(a.said[0], r#"[["WEATH", "OB"], ["IB"]] [["A"]]"#);
    assert_eq!(a.said[1], "2 45.0 12000.0", "WEATH after its top metre went: 4 m");
    let got = by_process(&a);
    let want = [("Topsoil", 3_000.0, 1.0), ("", 12_000.0, 4.0), ("Dragline", 9_000.0, 3.0), ("Dragline", 126_000.0, 42.0), ("", 6_000.0, 2.0), ("", 30_000.0, 10.0)];
    assert_eq!(got.len(), want.len(), "{got:?}");
    for ((p, v, t), (wp, wv, wt)) in got.iter().zip(want) {
        assert_eq!(p, wp);
        assert!((v - wv).abs() <= 1e-9 * wv && (t - wt).abs() <= 1e-9 * wt, "{p}: {v} m3, {t} m against {wv}, {wt}");
    }
    let weath: f64 = got[..3].iter().map(|g| g.1).sum();
    assert!((weath - 24_000.0).abs() <= 1e-9, "WEATH's pieces add back: {weath}");
    assert_eq!(a.splits.iter().map(|s| s.how.as_str()).collect::<Vec<_>>(), ["thickness 3 off the bottom", "thickness 1 off the top"]);
    let refused = Allocation::compile("def allocate(column, p):\n    column.take_from_top(column.intervals()[0], 60)\n", declared()).unwrap().run(two_horizons()).unwrap_err().message;
    assert_eq!(refused, "take_from_top asks 60 m of units 50 m thick");
}
