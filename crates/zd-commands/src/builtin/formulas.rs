//! Formula steps (decision 0077, workstream 7b): a dataflow's own Starlark, run on the units its filter fits, between the
//! built-in steps. The engine hands `zd-script` a column's units and writes back what each formula set; a formula reads
//! and sets field values only, and never moves material, which is a built-in step's work.

use std::collections::BTreeMap;

use serde::Serialize;

use zd_core::{AttrValue, Attributes, Dataflow, DataflowStep, FormulaStep, ParamValue, ParameterTable, attribute_word};
use zd_reserves::{FieldKind, Fields};
use zd_script::{ColumnInput, Declared, FieldDecl, Formula, Kind, ScriptValue, UnitInput, Values};

/// The built-in steps a formula step may follow (decision 0077, point 4): after `initialise`, before in situ; after
/// `join_benches`, a horizon whole, before the working sections (as Spry's discard test); after `working_sections`; after
/// `rom_dilution`; after `rom`, before the final writes. Other formula steps may sit between.
pub const FORMULA_STAGES: [&str; 5] = ["initialise", "join_benches", "working_sections", "rom_dilution", "rom"];

/// The labels a unit carries, read by a formula and matched by a step's filter, never set: its seam (the horizon as
/// cut), its material, its bench (as cut, none once joined) and what it became (loss, dilution, …), none until it's set.
pub const LABELS: [&str; 4] = ["seam", "material", "bench", "became"];

/// A relative tolerance for a formula's known answers: a worked example should match to the last few digits.
pub const TEST_TOLERANCE: f64 = 1e-9;

fn unit_of(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Volume => "m3",
        FieldKind::Thickness | FieldKind::Length => "m",
        FieldKind::Area => "m2",
        FieldKind::Density => "t/m3",
        FieldKind::Moisture => "%",
        _ => "",
    }
}

/// What a dataflow's formulas may use: its fields (numbers, each with its unit), the unit's labels, and the parameter
/// tables' value columns, the first table's where two have one of a name.
pub fn declared(fields: &Fields, tables: &[&ParameterTable]) -> Declared {
    let mut parameters: Vec<FieldDecl> = Vec::new();
    for t in tables {
        for c in &t.columns {
            if !parameters.iter().any(|p| p.name == c.name) {
                parameters.push(FieldDecl { name: c.name.clone(), kind: if c.text { Kind::Text } else { Kind::Number }, unit: c.unit.clone() });
            }
        }
    }
    Declared {
        fields: fields.list().iter().map(|f| FieldDecl::number(&f.name, unit_of(f.kind))).collect(),
        labels: LABELS.iter().map(|l| l.to_string()).collect(),
        parameters,
    }
}

pub fn param_value(v: &ParamValue) -> ScriptValue {
    match v {
        ParamValue::Number(n) => ScriptValue::Number(*n),
        ParamValue::Text(t) => ScriptValue::Text(t.clone()),
    }
}

pub fn attr_value(v: &AttrValue) -> ScriptValue {
    match v {
        AttrValue::Number(n) => ScriptValue::Number(*n),
        other => ScriptValue::Text(attribute_word(other)),
    }
}

/// A block's attributes as a formula reads them (`unit.block.strip`).
pub fn block_values(attributes: &Attributes) -> Values {
    std::sync::Arc::new(attributes.iter().map(|(k, v)| (k.to_string(), attr_value(v))).collect())
}

/// The parameters for a block: each value column of the dataflow's tables, from the first row that fits the block and
/// holds it; one no row gives is left out, so reading it stops the block (0070).
pub fn parameter_values(tables: &[&ParameterTable], declared: &Declared, attributes: &Attributes) -> Values {
    std::sync::Arc::new(
        declared
            .parameters
            .iter()
            .filter_map(|p| ParameterTable::lookup_in(tables, attributes, &p.name).map(|found| (p.name.clone(), param_value(found.value))))
            .collect(),
    )
}

/// Every formula step sits at one of the stages and none takes a built-in step's name; each compiles against the
/// dataflow's names and gives its known answers. Said for the first step that doesn't, with why.
pub fn check_formula_steps(dataflow: &Dataflow, built_in: &[&str], declared: &Declared) -> Result<Vec<(usize, Formula)>, String> {
    let mut last_built_in: Option<&str> = None;
    let mut compiled = Vec::new();
    for (i, s) in dataflow.steps.iter().enumerate() {
        let Some(f) = &s.formula else {
            last_built_in = Some(s.step.as_str());
            continue;
        };
        if built_in.contains(&s.step.as_str()) {
            return Err(format!("the formula step {} has a built-in step's name: give it its own", s.step));
        }
        if !last_built_in.is_some_and(|b| FORMULA_STAGES.contains(&b)) {
            return Err(format!(
                "the formula step {} sits {}: a formula step goes after initialise (before in situ), join_benches, working_sections, rom_dilution or rom (before the final writes)",
                s.step,
                last_built_in.map_or("before any built-in step".to_string(), |b| format!("after {b}"))
            ));
        }
        let formula = Formula::compile(&f.script, declared.clone()).map_err(|problems| {
            let said: Vec<String> = problems.iter().map(|p| p.to_string()).collect();
            format!("the formula step {}: {}", s.step, said.join("; "))
        })?;
        run_tests(s, f, &formula)?;
        compiled.push((i, formula));
    }
    Ok(compiled)
}

/// A formula step's known answers, each a unit on its own: the fields it must leave the unit with.
fn run_tests(step: &DataflowStep, f: &FormulaStep, formula: &Formula) -> Result<(), String> {
    for t in &f.tests {
        test_result(t, formula).map_err(|why| format!("the formula step {}, test {}: {why}", step.step, t.name))?;
    }
    Ok(())
}

/// One known answer: none where it holds, else why not.
fn test_result(t: &zd_core::FormulaTest, formula: &Formula) -> Result<(), String> {
    {
        let map = |m: &BTreeMap<String, zd_core::ParamValue>| -> Values { std::sync::Arc::new(m.iter().map(|(k, v)| (k.clone(), param_value(v))).collect()) };
        let rom_coal = (!t.rom_coal.is_empty()).then_some(0);
        let input = ColumnInput {
            units: vec![UnitInput { fields: map(&t.unit), block: map(&t.block), parameters: map(&t.parameters), section: None, rom_coal }],
            sections: Vec::new(),
            rom_coals: if rom_coal.is_some() { vec![map(&t.rom_coal)] } else { Vec::new() },
            column: map(&t.unit),
        };
        let out = match (formula.run(&input, &[0]), &t.stops) {
            (Err(p), Some(words)) if p.message.to_lowercase().contains(&words.to_lowercase()) => return Ok(()),
            (Err(p), Some(words)) => return Err(format!("it stopped with \"{}\", not \"{words}\"", p.message)),
            (Err(p), None) => return Err(p.to_string()),
            (Ok(_), Some(words)) => return Err(format!("it ran on; it should stop with \"{words}\"")),
            (Ok(out), None) => out,
        };
        for (field, want) in &t.expect {
            let got = out[0].get(field).cloned().or_else(|| t.unit.get(field).map(param_value));
            let ok = match (want, &got) {
                (Some(want), Some(ScriptValue::Number(x))) => (x - want).abs() <= TEST_TOLERANCE * want.abs().max(1.0),
                (None, None | Some(ScriptValue::None)) => true,
                _ => false,
            };
            if !ok {
                let got = got.map_or("nothing".to_string(), |g| g.to_string());
                let want = want.map_or("nothing".to_string(), |w| w.to_string());
                return Err(format!("{field} is {got}, not {want}"));
            }
        }
    }
    Ok(())
}

/// A problem with a formula as written, with its line where it has one.
#[derive(Debug, Clone, Serialize)]
pub struct FormulaProblem {
    pub message: String,
    pub line: Option<usize>,
}

/// A known answer, and why it doesn't hold where it doesn't.
#[derive(Debug, Clone, Serialize)]
pub struct FormulaTestResult {
    pub name: String,
    pub ok: bool,
    pub why: Option<String>,
}

/// The names a dataflow's formulas may use (decision 0077, 7d), for the editor and for Copy for AI: its fields with
/// their units, the unit's labels, and its parameter tables' columns with their units and whether they hold words.
#[derive(Debug, Clone, Serialize)]
pub struct FormulaNames {
    pub fields: Vec<(String, String)>,
    pub labels: Vec<String>,
    pub parameters: Vec<(String, String, bool)>,
}

/// One formula step as edited, checked (decision 0077, 7d): the built-in step it follows, every problem with its line,
/// and each known answer, held or not.
#[derive(Debug, Clone, Serialize)]
pub struct FormulaReport {
    pub stage: Option<String>,
    pub problems: Vec<FormulaProblem>,
    pub tests: Vec<FormulaTestResult>,
    pub names: FormulaNames,
}

/// Checks the formula step at `index` of a dataflow as edited, against its fields and parameter tables: where it sits,
/// its name, every problem compiling it with its line, and each of its known answers.
pub fn check_formula(d: &Dataflow, tables: &[&ParameterTable], index: usize, built_in: &[&str]) -> Result<FormulaReport, String> {
    let fields = d.engine_fields()?;
    let declared = declared(&fields, tables);
    let names = FormulaNames {
        fields: declared.fields.iter().map(|f| (f.name.clone(), f.unit.clone())).collect(),
        labels: declared.labels.clone(),
        parameters: declared.parameters.iter().map(|p| (p.name.clone(), p.unit.clone(), p.kind == Kind::Text)).collect(),
    };
    let step = d.steps.get(index).ok_or_else(|| format!("the dataflow has no step {}", index + 1))?;
    let f = step.formula.as_ref().ok_or_else(|| format!("{} isn't a formula step", step.step))?;
    let stage = d.steps[..index].iter().rev().find(|s| s.formula.is_none()).map(|s| s.step.clone());
    let mut problems = Vec::new();
    if built_in.contains(&step.step.as_str()) {
        problems.push(FormulaProblem { message: format!("{} is a built-in step's name: give the formula its own", step.step), line: None });
    }
    if !stage.as_deref().is_some_and(|b| FORMULA_STAGES.contains(&b)) {
        problems.push(FormulaProblem {
            message: format!("it sits {}: a formula step goes after initialise, join_benches, working_sections, rom_dilution or rom", stage.as_deref().map_or("before any built-in step".to_string(), |b| format!("after {b}"))),
            line: None,
        });
    }
    let mut tests = Vec::new();
    match Formula::compile(&f.script, declared) {
        Err(found) => problems.extend(found.into_iter().map(|p| FormulaProblem { message: p.message, line: p.line })),
        Ok(formula) => {
            for t in &f.tests {
                let held = test_result(t, &formula);
                tests.push(FormulaTestResult { name: t.name.clone(), ok: held.is_ok(), why: held.err() });
            }
        }
    }
    Ok(FormulaReport { stage, problems, tests, names })
}
