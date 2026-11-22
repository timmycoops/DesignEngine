//! Known answers for the formula engine (decision 0077, 7a): the library to 1e-12, a made-up column worked by hand,
//! every check that refuses a formula before it's saved, the sandbox, the trace, and columns run in parallel.

use super::*;

const TIGHT: f64 = 1e-12;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= TIGHT * b.abs().max(1.0)
}

fn declared() -> Declared {
    Declared {
        fields: vec![
            FieldDecl::number("volume", "m3"),
            FieldDecl::number("thickness", "m"),
            FieldDecl::number("rd", "t/m3"),
            FieldDecl::number("mad", "%"),
            FieldDecl::number("mis", "%"),
            FieldDecl::number("ash_ad", "%"),
            FieldDecl::number("rd_is", "t/m3"),
            FieldDecl::number("tonnes_is", "t"),
            FieldDecl::number("share", ""),
            FieldDecl::text("discard"),
        ],
        labels: vec!["seam".into(), "material".into(), "bench".into()],
        parameters: vec![FieldDecl::number("insitu_ash_cutoff", "%"), FieldDecl::number("rom_ash_cutoff", "%"), FieldDecl::text("loader")],
        libraries: Default::default(),
        lookups: Default::default(),
    }
}

fn n(x: f64) -> ScriptValue {
    ScriptValue::Number(x)
}
fn t(s: &str) -> ScriptValue {
    ScriptValue::Text(s.into())
}

/// Two coal plies of seam U over a waste: 1,000 m³ at RD 1.45 (Mad 3.5, Mis 8, ash 12 %), 500 m³ at RD 1.60 (Mad 3,
/// Mis 7, ash 40 %), then 300 m³ of waste. The coal is one working section; the coal's parameters give a 35 % in situ
/// ash cut-off; the waste has none.
fn column() -> ColumnInput {
    let coal_p = values([("insitu_ash_cutoff", n(35.0)), ("loader", t("EX3600"))]);
    let block = values([("strip", n(3.0)), ("block", t("B1"))]);
    let unit = |pairs: Vec<(&str, ScriptValue)>, p: &Values, section| UnitInput { fields: values(pairs), block: block.clone(), parameters: p.clone(), section, rom_coal: section };
    ColumnInput {
        units: vec![
            unit(vec![("seam", t("U")), ("material", t("coal")), ("bench", n(110.0)), ("volume", n(1000.0)), ("thickness", n(2.0)), ("rd", n(1.45)), ("mad", n(3.5)), ("mis", n(8.0)), ("ash_ad", n(12.0))], &coal_p, Some(0)),
            unit(vec![("seam", t("U")), ("material", t("coal")), ("bench", n(100.0)), ("volume", n(500.0)), ("thickness", n(1.0)), ("rd", n(1.60)), ("mad", n(3.0)), ("mis", n(7.0)), ("ash_ad", n(40.0))], &coal_p, Some(0)),
            unit(vec![("material", t("waste")), ("volume", n(300.0)), ("thickness", n(0.6)), ("rd", n(2.3))], &values([]), None),
        ],
        sections: vec![values([("volume", n(1500.0)), ("thickness", n(3.0))])],
        rom_coals: vec![values([("volume", n(1500.0)), ("mad", n(3.2))])],
        column: values([("volume", n(1800.0))]),
    }
}

const IN_SITU: &str = r#"
def formula(unit, p):
    if unit.material != "coal":
        return
    rd = in_situ_rd(unit.rd, unit.mad, unit.mis)
    unit.rd_is = rd
    unit.tonnes_is = unit.volume * rd
    if unit.ash_ad > p.insitu_ash_cutoff:
        print("ash over the cut-off")
        unit.discard = "ash"
"#;

/// The guard on the patched Starlark (vendor/starlark/ZOVLYN.md): built with it, serde_json must still read a number as
/// a plain f64. With `arbitrary_precision` back it keeps every digit it was given, and the app's untagged numbers break.
#[test]
fn serde_json_reads_numbers_as_plain_floats() {
    let v: serde_json::Value = serde_json::from_str("0.1000000000000000000001").expect("a number");
    assert_eq!(serde_json::to_string(&v).expect("written"), "0.1", "serde_json's arbitrary_precision is on: see vendor/starlark/ZOVLYN.md");
}

#[test]
fn the_library_gives_known_answers() {
    // Preston and Sanders by hand: 1.45 × 96.5 ÷ (100 + 1.45 × 4.5 − 8) = 139.925 ÷ 98.525.
    assert!(close(library::in_situ_rd(1.45, 3.5, 8.0), 139.925 / 98.525));
    // The inverse gives the RD back, over a spread of coals.
    for rd in [1.25, 1.4, 1.6, 1.9, 2.4] {
        for mad in [1.0, 3.5, 8.0] {
            for mis in [2.0, 8.0, 15.0] {
                let back = library::air_dried_rd(library::in_situ_rd(rd, mad, mis), mad, mis);
                assert!(close(back, rd), "rd {rd} mad {mad} mis {mis}: back {back}");
            }
        }
    }
    assert!(close(library::moisture_factor(3.5, 8.0), 92.0 / 96.5));
    assert!(close(library::to_basis(12.0, 3.5, 8.0), 12.0 * 92.0 / 96.5));
    assert_eq!(library::weighted(&[10.0, 20.0], &[1.0, 3.0]), Some(17.5));
    assert_eq!(library::weighted(&[10.0], &[0.0]), None);
}

#[test]
fn a_formula_runs_on_a_column_as_worked_by_hand() {
    let f = Formula::compile(IN_SITU, declared()).expect("it compiles");
    let col = column();
    let out = f.run(&col, &[0, 1, 2]).expect("it runs");
    let rd0 = 139.925 / 98.525;
    let rd1 = 1.6 * 97.0 / (100.0 + 1.6 * 4.0 - 7.0);
    let num = |u: usize, k: &str| match out[u].get(k) {
        Some(ScriptValue::Number(x)) => *x,
        other => panic!("unit {u} {k}: {other:?}"),
    };
    assert!(close(num(0, "rd_is"), rd0));
    assert!(close(num(0, "tonnes_is"), 1000.0 * rd0));
    assert!(close(num(1, "rd_is"), rd1));
    assert!(close(num(1, "tonnes_is"), 500.0 * rd1));
    assert_eq!(out[0].get("discard"), None, "12 % ash is under the cut-off");
    assert_eq!(out[1].get("discard"), Some(&t("ash")), "40 % is over it");
    assert_eq!(out[1].said, vec!["ash over the cut-off".to_string()]);
    assert!(out[2].sets.is_empty(), "waste is left alone");
}

#[test]
fn a_formula_reads_its_section_column_block_and_the_unit_above() {
    let src = r#"
def formula(unit):
    unit.share = unit.volume / unit.section.volume if unit.section.volume else unit.volume / unit.column.volume
    if unit.above != None and unit.above.material == "coal":
        unit.thickness = unit.thickness + unit.above.thickness
    if unit.block.strip == 3 and unit.block.nothing == None:
        unit.discard = unit.block.block
"#;
    let f = Formula::compile(src, declared()).expect("it compiles");
    let out = f.run(&column(), &[0, 1, 2]).expect("it runs");
    assert_eq!(out[0].get("share"), Some(&n(1000.0 / 1500.0)));
    assert_eq!(out[1].get("share"), Some(&n(500.0 / 1500.0)));
    assert_eq!(out[2].get("share"), Some(&n(300.0 / 1800.0)), "no section: the column");
    assert_eq!(out[0].get("thickness"), None, "the top unit has none above");
    assert_eq!(out[1].get("thickness"), Some(&n(3.0)));
    assert_eq!(out[2].get("thickness"), Some(&n(1.6)));
    assert_eq!(out[0].get("discard"), Some(&t("B1")), "a block attribute missing reads None");
}

/// A unit in a ROM coal section reads its coal added up; one outside reads None.
#[test]
fn a_unit_reads_its_rom_section_s_coal() {
    let f = Formula::compile("def formula(unit):
    if unit.rom_coal != None:
        unit.mad = unit.rom_coal.mad
", declared()).expect("it compiles");
    let out = f.run(&column(), &[0, 2]).expect("it runs");
    assert_eq!(out[0].get("mad"), Some(&n(3.2)));
    assert!(out[1].sets.is_empty(), "the waste is in no ROM coal section");
    let (m, _) = refused("def formula(unit):
    unit.mad = unit.rom_coal.discard
");
    assert!(m.contains("adds up number fields"), "{m}");
}

/// The reference an AI is handed (decision 0077, 7e) teaches only what the engine takes: every Python example in it
/// compiles against a dataflow with the names it uses, and its worked examples read as JSON with the keys it says.
#[test]
fn every_example_in_the_reference_compiles() {
    let names = Declared {
        fields: ["volume", "thickness", "area", "rd", "rd_is", "rd_rom", "tonnes_ad_per_m3", "mad", "mis", "mrom", "ash", "ash_is", "discarded", "hours"]
            .iter()
            .map(|n| FieldDecl::number(n, ""))
            .collect(),
        labels: vec!["seam".into(), "material".into(), "bench".into(), "became".into()],
        parameters: vec![FieldDecl::number("insitu_ash_cutoff", "%")],
        libraries: Default::default(),
        lookups: Default::default(),
    };
    let blocks: Vec<&str> = REFERENCE.split("```python").skip(1).map(|b| b.split("```").next().unwrap_or("")).collect();
    assert!(blocks.len() >= 7, "{} examples", blocks.len());
    for b in &blocks {
        if let Err(problems) = Formula::compile(b, names.clone()) {
            panic!("a reference example doesn't compile: {problems:?}\n{b}");
        }
    }
    let json = REFERENCE.split("```json").nth(1).and_then(|b| b.split("```").next()).expect("a JSON example");
    let tests: serde_json::Value = serde_json::from_str(json).expect("the worked examples read as JSON");
    for t in tests.as_array().expect("a list") {
        let keys: Vec<&str> = t.as_object().unwrap().keys().map(String::as_str).collect();
        assert!(keys.iter().all(|k| ["name", "unit", "block", "parameters", "rom_coal", "expect", "stops"].contains(k)), "{keys:?}");
    }
    // The first runs: Preston and Sanders at RD 1.4, Mad 3, Mis 8 is 135.8 ÷ 99, as its worked example says.
    let f = Formula::compile(blocks.iter().find(|b| b.contains("# In situ density")).expect("the densities example"), names.clone()).expect("densities");
    let col = ColumnInput {
        units: vec![UnitInput { fields: values([("material", t("coal")), ("rd", n(1.4)), ("mad", n(3.0)), ("mis", n(8.0))]), block: values([]), parameters: values([]), section: None, rom_coal: None }],
        ..ColumnInput::default()
    };
    let out = f.run(&col, &[0]).expect("it runs");
    assert!(close(match out[0].get("rd_is") { Some(ScriptValue::Number(x)) => *x, _ => f64::NAN }, 135.8 / 99.0));
}

/// A script with Windows line endings (pasted, or checked out so) reads as one with plain new lines.
#[test]
fn windows_line_endings_are_new_lines() {
    let f = Formula::compile("def formula(unit):\r\n    unit.rd_is = unit.rd * 2\r\n", declared()).expect("it compiles");
    assert_eq!(f.run(&column(), &[0]).expect("it runs")[0].get("rd_is"), Some(&n(2.9)));
}

/// A field named as the formula runs, read and set by `unit["name"]`; a name the dataflow lacks is refused.
#[test]
fn a_field_can_be_named_as_the_formula_runs() {
    let src = "def formula(unit, p):\n    for name in dir(p):\n        if name.endswith(\"_cutoff\"):\n            unit[\"ash_ad\"] = p.insitu_ash_cutoff + unit[\"thickness\"]\n";
    let f = Formula::compile(src, declared()).expect("it compiles");
    let out = f.run(&column(), &[0]).expect("it runs");
    assert_eq!(out[0].get("ash_ad"), Some(&n(37.0)), "35 + 2 m");
    let f = Formula::compile("def formula(unit):\n    unit[\"ahs\"] = 1\n", declared()).expect("a name in quotes is checked as it runs");
    let p = f.run(&column(), &[0]).expect_err("no such field");
    assert!(p.message.contains("`ahs` isn't a field of this dataflow"), "{}", p.message);
    let f = Formula::compile("def formula(unit):\n    x = unit[\"ash_da\"]\n", declared()).expect("compiles");
    assert!(f.run(&column(), &[0]).expect_err("no such field").message.contains("did you mean `ash_ad`"));
}

/// The first problem a formula is refused for, with its line.
fn refused(src: &str) -> (String, Option<usize>) {
    let problems = Formula::compile(src, declared()).expect_err("it's refused");
    (problems[0].message.clone(), problems[0].line)
}

#[test]
fn a_formula_is_checked_before_it_is_saved() {
    let (m, l) = refused("def formula(unit):\n    unit.rd_si = 1.0\n");
    assert!(m.contains("`unit.rd_si` isn't a field") && m.contains("did you mean `rd_is`"), "{m}");
    assert_eq!(l, Some(2));
    let (m, l) = refused("def formula(unit):\n    x = 1\n    unit.seam = \"L\"\n");
    assert!(m.contains("is a label of the unit"), "{m}");
    assert_eq!(l, Some(3));
    let (m, _) = refused("def formula(unit, p):\n    unit.rd_is = p.rom_ash_cutof\n");
    assert!(m.contains("isn't a parameter column") && m.contains("did you mean `rom_ash_cutoff`"), "{m}");
    let (m, _) = refused("def formula(unit, p):\n    p.rom_ash_cutoff = 30\n");
    assert!(m.contains("parameters are read, never set"), "{m}");
    let (m, _) = refused("def formula(unit):\n    unit.rd_is = unit.section.discard\n");
    assert!(m.contains("adds up number fields"), "{m}");
    // A name that's never been given a value: Starlark finds it when the formula loads.
    let (m, l) = refused("def formula(unit):\n    unit.rd_is = rd_ad * 2\n");
    assert!(m.contains("`rd_ad` not found"), "{m}");
    assert_eq!(l, Some(2));
    let (m, l) = refused("def formula(unit):\n    unit.rd_is = (1.0 +\n\n");
    assert!(m.to_lowercase().contains("parse error"), "{m}");
    assert!(l.is_some());
    let (m, _) = refused("def calc(unit):\n    pass\n");
    assert!(m.contains("def formula(unit, p):"), "{m}");
    // Words in comments and quotes are never taken for names.
    Formula::compile("def formula(unit):\n    # unit.nonsense is fine here\n    unit.discard = \"unit.nonsense\"\n", declared()).expect("comments and quotes are left alone");
}

#[test]
fn a_formula_has_no_way_out() {
    let (m, _) = refused("def formula(unit):\n    unit.discard = open(\"C:/Windows/win.ini\").read()\n");
    assert!(m.contains("`open` not found"), "{m}");
    let (m, _) = refused("load(\"os.star\", \"system\")\ndef formula(unit):\n    pass\n");
    assert!(m.to_lowercase().contains("load") || m.contains("imports"), "{m}");
    let (m, _) = refused("import os\ndef formula(unit):\n    pass\n");
    assert!(m.to_lowercase().contains("parse error"), "{m}");
}

#[test]
fn a_problem_on_a_unit_says_the_unit_and_the_line() {
    let f = Formula::compile("def formula(unit):\n    x = 1\n    unit.rd_is = \"high\"\n", declared()).expect("names are fine");
    let p = f.run(&column(), &[0]).expect_err("words in a number field");
    assert!(p.message.contains("`rd_is` holds numbers"), "{}", p.message);
    assert_eq!((p.line, p.unit), (Some(3), Some(0)));
    let f = Formula::compile("def formula(unit):\n    unit.rd_is = unit.volume / 0\n", declared()).expect("compiles");
    let p = f.run(&column(), &[1]).expect_err("a division by zero");
    assert_eq!((p.line, p.unit), (Some(2), Some(1)), "{}", p.message);
    // The waste has no parameter row: reading one stops it, and getattr reads a default.
    let f = Formula::compile("def formula(unit, p):\n    unit.ash_ad = p.insitu_ash_cutoff\n", declared()).expect("compiles");
    let p = f.run(&column(), &[0, 2]).expect_err("no parameter for the waste");
    assert!(p.message.contains("no parameter row gives `insitu_ash_cutoff`"), "{}", p.message);
    assert_eq!(p.unit, Some(2));
    let f = Formula::compile("def formula(unit, p):\n    unit.ash_ad = getattr(p, \"insitu_ash_cutoff\", 99)\n", declared()).expect("compiles");
    let out = f.run(&column(), &[0, 2]).expect("defaults where missing");
    assert_eq!((out[0].get("ash_ad"), out[1].get("ash_ad")), (Some(&n(35.0)), Some(&n(99.0))));
}

#[test]
fn a_traced_unit_shows_what_it_read_each_line_and_what_it_set() {
    let f = Formula::compile(IN_SITU, declared()).expect("it compiles");
    let col = column();
    let tr = f.trace(&col, 1);
    assert_eq!(tr.stopped, None);
    let ran: Vec<usize> = tr.lines.iter().map(|l| l.line).collect();
    assert_eq!(ran, vec![3, 5, 6, 7, 8, 9, 10], "every line of the coal's path, the early return not taken");
    let rd = tr.lines.iter().find(|l| l.line == 5).expect("line 5");
    assert_eq!(rd.text, "rd = in_situ_rd(unit.rd, unit.mad, unit.mis)");
    let rd1 = 1.6 * 97.0 / (100.0 + 1.6 * 4.0 - 7.0);
    assert_eq!(rd.values, vec![("rd".to_string(), format!("{rd1}"))]);
    assert!(tr.events.contains(&Event::Read { what: "p.insitu_ash_cutoff".into(), value: n(35.0) }));
    assert!(tr.events.contains(&Event::Set { field: "discard".into(), value: t("ash") }));
    assert_eq!(tr.said, vec!["ash over the cut-off".to_string()]);
    assert_eq!(tr.sets.last(), Some(&("discard".to_string(), t("ash"))));
    // The first ply's ash is under the cut-off: lines 9 and 10 never run.
    let tr0 = f.trace(&col, 0);
    assert!(tr0.lines.iter().all(|l| l.line < 9), "{:?}", tr0.lines.iter().map(|l| l.line).collect::<Vec<_>>());
    // A unit that stops shows where.
    let f = Formula::compile("def formula(unit, p):\n    a = 1\n    unit.ash_ad = p.insitu_ash_cutoff\n", declared()).expect("compiles");
    let tr2 = f.trace(&col, 2);
    assert_eq!(tr2.stopped.as_ref().and_then(|p| p.line), Some(3));
}

#[test]
fn columns_run_in_parallel_give_what_they_give_one_at_a_time() {
    let f = Formula::compile(IN_SITU, declared()).expect("it compiles");
    let columns: Vec<ColumnInput> = (0..64)
        .map(|i| {
            let mut c = column();
            let x = f64::from(i);
            c.units[0].fields = values([("material", t("coal")), ("volume", n(1000.0 + x)), ("rd", n(1.3 + x / 100.0)), ("mad", n(3.0)), ("mis", n(8.0)), ("ash_ad", n(x))]);
            c
        })
        .collect();
    let all = [0usize, 1, 2];
    let jobs: Vec<(&ColumnInput, &[usize])> = columns.iter().map(|c| (c, &all[..])).collect();
    let parallel = f.run_many(&jobs);
    for (c, r) in columns.iter().zip(parallel) {
        assert_eq!(r.expect("it runs"), f.run(c, &all).expect("it runs"));
    }
}

/// Timing for the plan (run with `--release -- --ignored --nocapture`): a million made-up units, 20 to a column as a
/// block's benches are, each column its own values, through the in situ formula.
#[test]
#[ignore]
fn a_million_units_through_a_formula() {
    let f = Formula::compile(IN_SITU, declared()).expect("it compiles");
    let per = 20;
    let units: Vec<usize> = (0..per).collect();
    let columns: Vec<ColumnInput> = (0..1_000_000 / per)
        .map(|i| {
            let p = values([("insitu_ash_cutoff", n(35.0))]);
            let block = values([("strip", n(f64::from(i as u32 % 15)))]);
            ColumnInput {
                units: (0..per)
                    .map(|k| UnitInput {
                        fields: values([("seam", t("U")), ("material", t("coal")), ("volume", n(1000.0 + k as f64)), ("rd", n(1.4 + k as f64 / 100.0)), ("mad", n(3.5)), ("mis", n(8.0)), ("ash_ad", n(10.0 + k as f64))]),
                        block: block.clone(),
                        parameters: p.clone(),
                        section: None,
                        rom_coal: None,
                    })
                    .collect(),
                sections: vec![],
                rom_coals: vec![],
                column: values([]),
            }
        })
        .collect();
    let jobs: Vec<(&ColumnInput, &[usize])> = columns.iter().map(|c| (c, &units[..])).collect();
    let t0 = std::time::Instant::now();
    let out = f.run_many(&jobs);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    assert!(out.iter().all(|r| r.is_ok()));
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    println!("1,000,000 units in {} columns on {threads} threads: {ms:.0} ms", columns.len());
    let one = &columns[..2_000];
    let t1 = std::time::Instant::now();
    for c in one {
        f.run(c, &units).expect("it runs");
    }
    let us = t1.elapsed().as_secs_f64() * 1e6 / (one.len() * per) as f64;
    println!("one thread: {us:.2} us a unit");
}

/// The fields a formula reads, for the Dataflow view (decision 0084): each once, in the order first read; a field it only
/// sets isn't read, one it adds to is; labels, parameters and comments are left out, and a field read through the unit
/// above or the section counts.
#[test]
fn a_formula_s_fields_read_are_read_off_its_source() {
    let f = Formula::compile(IN_SITU, declared()).expect("compiles");
    assert_eq!(f.fields_read(), ["rd", "mad", "mis", "volume", "ash_ad"], "rd_is, tonnes_is and discard are only set; material is a label");
    let more = r#"
def formula(u):
    # u.thickness in a comment isn't read
    u.share += u.section.thickness
    u.tonnes_is = u.above.volume * 2 + u.volume
"#;
    let g = Formula::compile(more, declared()).expect("compiles");
    assert_eq!(g.fields_read(), ["share", "thickness", "volume"]);
}

/// Scripts of the project (decision 0099, workstream 6): a formula loads a function by the script's name, a script loads
/// another, and what can't be loaded is refused with why.
#[test]
fn a_formula_loads_its_functions_from_the_project_s_scripts() {
    let libs = |pairs: &[(&str, &str)]| std::sync::Arc::new(pairs.iter().map(|(n, s)| (n.to_string(), s.to_string())).collect::<Vec<_>>());
    let with = |libraries: Libraries| Declared { libraries, ..declared() };
    let doubling = "def doubled(x):\n    return x * 2\n";
    // `ash2` is the ash doubled, by a function the project's script `maths` gives through `helpers`.
    let helpers = "load(\"maths\", \"doubled\")\ndef twice_ash(unit):\n    return doubled(unit.ash_ad)\n";
    let formula = "load(\"helpers\", \"twice_ash\")\ndef formula(unit):\n    unit.ash_ad = twice_ash(unit)\n";
    let f = Formula::compile(formula, with(libs(&[("maths", doubling), ("helpers", helpers)]))).expect("it loads");
    let sets = f.run(&column(), &[0]).unwrap();
    assert_eq!(sets[0].get("ash_ad"), Some(&ScriptValue::Number(24.0)));
    // Traced through the loaded function: the formula's own lines, and what it set.
    let trace = f.trace(&column(), 0);
    assert!(trace.stopped.is_none(), "{:?}", trace.stopped);
    assert_eq!(trace.sets, [("ash_ad".to_string(), ScriptValue::Number(24.0))]);

    let refused = |source: &str, libraries: Libraries| Formula::compile(source, with(libraries)).err().map(|p| p[0].message.clone()).unwrap_or_default();
    assert!(refused(formula, libs(&[("maths", doubling)])).contains("can't load helpers: the project has no script of that name"));
    let looping = refused(formula, libs(&[("helpers", "load(\"helpers2\", \"x\")\n"), ("helpers2", "load(\"helpers\", \"twice_ash\")\nx = 1\n")]));
    assert!(looping.contains("helpers loads itself, through helpers → helpers2 → helpers"), "{looping}");

    // A script checked on its own, its problem on its line.
    assert!(check_library("maths", doubling, &libs(&[])).is_ok());
    let wrong = check_library("maths", "def doubled(x):\n    return x * 2\ny = undefined_name\n", &libs(&[])).unwrap_err();
    assert_eq!(wrong[0].line, Some(3), "{:?}", wrong);
    assert!(check_library("helpers", helpers, &libs(&[])).unwrap_err()[0].message.contains("can't load maths"));
}

/// The app's starter script for a new script of the project (ProjectScriptEditor.tsx's STARTER, kept the same) checks
/// clean as a library, and its `by_tonnes` adds 10 % and 20 % up over 1 t and 3 t to 17.5 %.
#[test]
fn the_app_s_starter_script_checks_clean() {
    let starter = r#"# Functions a formula or a calculated column loads by this script's name:
#   load("qualities", "by_tonnes")
def by_tonnes(values, tonnes):
    total = 0.0
    weighed = 0.0
    for v, t in zip(values, tonnes):
        total += t
        weighed += v * t
    return weighed / total if total else None
"#;
    assert!(check_library("qualities", starter, &std::sync::Arc::new(Vec::new())).is_ok());
    let libs = std::sync::Arc::new(vec![("qualities".to_string(), starter.to_string())]);
    let f = Formula::compile("load(\"qualities\", \"by_tonnes\")
def formula(unit):
    unit.ash_ad = by_tonnes([10.0, 20.0], [1.0, 3.0])
", Declared { libraries: libs, ..declared() }).unwrap();
    assert_eq!(f.run(&column(), &[0]).unwrap()[0].get("ash_ad"), Some(&ScriptValue::Number(17.5)));
}
