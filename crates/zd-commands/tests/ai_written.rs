//! Formulas an AI wrote from the reference alone (decision 0077, 7e). Two fresh AI sessions were each given only what
//! the formula panel's Copy for AI hands over: `zd-script`'s reference, a dataflow's field and parameter names, and a
//! starter formula described in plain words (`densities` and `rom_qualities`). What each wrote is kept in
//! `tests/ai_written/` as it came. Each must compile against those names, give the starter's own worked examples (which
//! the AI never saw), and give its own. A change to the reference or the engine that breaks them shows here.

use zd_commands::builtin::{STEPS, check_formula, with_starter_formulas};
use zd_core::{Dataflow, DataflowField, DataflowStep, FieldSource, FormulaTest, ParameterTable, ValueColumn};
use zd_reserves::{AddUp, Field, FieldKind};

/// The dataflow the AI was told about: its fields, all the built-in steps and the starter's formulas.
fn dataflow() -> Dataflow {
    let field = |name: &str, kind, add_up, source| DataflowField { field: Field::new(name, kind, add_up), source };
    let attr = |name: &str| FieldSource::Attribute { attribute: name.to_string() };
    let mass = |d: &str| AddUp::Mass { density: d.to_string() };
    let mut d = Dataflow {
        fields: vec![
            field("volume", FieldKind::Volume, AddUp::Sum, FieldSource::Volume),
            field("thickness", FieldKind::Thickness, AddUp::Sum, FieldSource::Thickness),
            field("area", FieldKind::Area, AddUp::Max, FieldSource::PlanArea),
            field("length", FieldKind::Length, AddUp::Max, attr("length")),
            field("rd", FieldKind::Density, AddUp::Volume, attr("rd")),
            field("ash", FieldKind::Quality, mass("rd"), attr("ash")),
            field("mad", FieldKind::Moisture, mass("rd"), attr("mad")),
            field("rd_is", FieldKind::Density, AddUp::Volume, FieldSource::Worked),
            field("rd_rom", FieldKind::Density, AddUp::Volume, FieldSource::Worked),
            field("mis", FieldKind::Moisture, mass("rd_is"), attr("mis")),
            field("mrom", FieldKind::Moisture, mass("rd_rom"), FieldSource::Parameter { column: "mrom".into() }),
        ],
        steps: STEPS.iter().map(|(n, _)| DataflowStep::on(n)).collect(),
        ..Dataflow::default()
    };
    with_starter_formulas(&mut d);
    d
}

/// The parameter columns the AI was told about.
fn table() -> ParameterTable {
    let n = |name: &str, unit: &str| ValueColumn::number(name, unit);
    let columns = vec![
        n("min_volume", "m3"),
        n("underburden_thickness", "m"),
        n("mad", "%"),
        n("mis", "%"),
        n("mrom", "%"),
        n("min_parting", "m"),
        n("ws_strip_ratio", ""),
        n("min_coal_thickness", "m"),
        n("insitu_ash_cutoff", "%"),
        ValueColumn::text("discard"),
        n("rom_ash_cutoff", "%"),
        ValueColumn::text("align_dilution_mad"),
        ValueColumn::text("aggregate_sections"),
        ValueColumn::text("clean_coal_average"),
        n("ash_min", "%"),
        n("ash_max", "%"),
        n("ash_default", "%"),
    ];
    ParameterTable { filters: Vec::new(), columns, rows: Vec::new() }
}

/// The starter's step `name` with the AI's formula in place of its own: the starter's worked examples, then the AI's.
fn checked(name: &str, script: &str, tests: &str) {
    let mut d = dataflow();
    let at = d.steps.iter().position(|s| s.step == name).expect("the starter has the step");
    let mine: Vec<FormulaTest> = serde_json::from_str(tests).expect("the AI's worked examples read");
    let f = d.steps[at].formula.as_mut().expect("a formula step");
    let starter = f.tests.len();
    f.script = script.to_string();
    f.tests.extend(mine);
    let t = table();
    let built_in: Vec<&str> = STEPS.iter().map(|(n, _)| *n).collect();
    let report = check_formula(&d, &[&t], at, &built_in).expect("checked");
    assert!(report.problems.is_empty(), "{name}: the AI's formula doesn't compile: {:?}", report.problems);
    let failed: Vec<_> = report.tests.iter().filter(|t| !t.ok).map(|t| format!("{}: {}", t.name, t.why.clone().unwrap_or_default())).collect();
    assert!(failed.is_empty(), "{name}: {} of {} worked examples fail ({starter} the starter's): {failed:?}", failed.len(), report.tests.len());
    assert!(report.tests.len() > starter, "{name}: the AI gave worked examples of its own");
}

#[test]
fn densities_written_by_an_ai_from_the_reference_alone() {
    checked("densities", include_str!("ai_written/densities.star"), include_str!("ai_written/densities.json"));
}

#[test]
fn rom_qualities_written_by_an_ai_from_the_reference_alone() {
    checked("rom_qualities", include_str!("ai_written/rom_qualities.star"), include_str!("ai_written/rom_qualities.json"));
}
