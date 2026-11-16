//! The starter formulas (decision 0077, workstream 7c): Spry's Import to ROM arithmetic as Starlark a planner can read and
//! change, put after `initialise` in a dataflow, each with its known answers. They replace the Rust that worked the same
//! numbers: size filled in, limits and defaults, and the in situ, ROM and air-dried densities. Written for the dataflow's
//! own field names, so a model whose density is `RD` gets `unit.RD`.

use std::collections::BTreeMap;

use zd_core::{Dataflow, DataflowField, DataflowStep, FieldSource, FormulaStep, FormulaTest, ParamValue};
use zd_reserves::{AddUp, Field, FieldKind};

/// The field a starter formula works the air-dried tonnes a cubic metre into: what qualities and Mad are weighed by
/// (decision 0077, point 5, Spry's way).
pub const RD_ADT: &str = "rd_adt";

/// The field the starter's `discard` formula marks coal in: 1 by hand, 2 by its in situ ash, 0 kept. The working
/// sections set aside what it marks.
pub const DISCARDED: &str = "discarded";

fn n(x: f64) -> ParamValue {
    ParamValue::Number(x)
}

fn t(s: &str) -> ParamValue {
    ParamValue::Text(s.to_string())
}

fn test(name: &str, unit: &[(&str, ParamValue)], parameters: &[(&str, ParamValue)], expect: &[(&str, f64)]) -> FormulaTest {
    let map = |pairs: &[(&str, ParamValue)]| pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<BTreeMap<_, _>>();
    FormulaTest { name: name.to_string(), unit: map(unit), block: BTreeMap::new(), parameters: map(parameters), rom_coal: BTreeMap::new(), expect: expect.iter().map(|(k, v)| (k.to_string(), *v)).collect() }
}

/// Starlark's reserved words: a field named one (Vulcan's ash is `as`) can't be written `unit.as`.
const KEYWORDS: [&str; 33] = [
    "and", "as", "assert", "break", "class", "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "load",
    "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield", "None", "True",
];

fn plain_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_') && !KEYWORDS.contains(&name) && name != "False"
}

/// A formula as written for these field names: one that isn't a plain name (a reserved word such as `as`, or one with a
/// space or a dash) is read and set as `unit["as"]`, and read from the ROM section's coal as `getattr(coal, "as", None)`.
fn for_names(script: &str, names: &[String]) -> String {
    let mut out = script.to_string();
    for name in names.iter().filter(|n| !plain_name(n)) {
        for (head, write) in [("unit.", format!("unit[{name:?}]")), ("coal.", format!("getattr(coal, {name:?}, None)"))] {
            let from = format!("{head}{name}");
            let mut done = String::new();
            let mut rest = out.as_str();
            while let Some(i) = rest.find(&from) {
                let after = rest[i + from.len()..].chars().next();
                let before = rest[..i].chars().last();
                let whole = !after.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') && !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
                done.push_str(&rest[..i]);
                done.push_str(if whole { &write } else { &from });
                rest = &rest[i + from.len()..];
            }
            done.push_str(rest);
            out = done;
        }
    }
    out
}

fn step(name: &str, script: String, tests: Vec<FormulaTest>) -> DataflowStep {
    DataflowStep { formula: Some(FormulaStep { when: BTreeMap::new(), script, tests }), ..DataflowStep::on(name) }
}

/// A field's name by kind, the first of it.
fn named(d: &Dataflow, kind: FieldKind) -> Option<String> {
    d.fields.iter().find(|f| f.field.kind == kind).map(|f| f.field.name.clone())
}

/// A field's name as the in situ step's settings give it (its usual name where they don't), if the dataflow has it.
fn setting_field(d: &Dataflow, key: &str) -> Option<String> {
    let name = d.steps.iter().find(|s| s.step == "in_situ").map_or(key.to_string(), |s| s.setting(key).to_string());
    d.fields.iter().any(|f| f.field.name == name).then_some(name)
}

/// `fill_size`: thickness from volume over area where a unit has none, area from volume over thickness, and RD from
/// air-dried tonnes over volume where a block carries tonnes but no RD (Spry's Initialise).
fn fill_size(volume: &str, thickness: &str, area: &str, rd: &str) -> DataflowStep {
    let script = format!(
        r#"# Size filled in, as Spry's Initialise does. A measured value is never changed: only one that's missing.
# - no thickness: the volume over the plan area;
# - no area: the volume over the thickness;
# - no RD but air-dried tonnes on the block (a `tonnes_ad` field): the tonnes over the volume.
def formula(unit):
    if unit.{thickness} == None and unit.{area}:
        unit.{thickness} = unit.{volume} / unit.{area}
        print("{thickness} filled in from {volume} over {area}")
    elif unit.{area} == None and unit.{thickness}:
        unit.{area} = unit.{volume} / unit.{thickness}
        print("{area} filled in from {volume} over {thickness}")
    tonnes = getattr(unit, "tonnes_ad", None)
    if unit.{rd} == None and tonnes != None and unit.{volume}:
        unit.{rd} = tonnes / unit.{volume}
        print("{rd} worked back from tonnes_ad over {volume}")
"#
    );
    let tests = vec![
        test("no thickness: 20,000 m3 over 5,000 m2", &[(volume, n(20_000.0)), (area, n(5_000.0)), (rd, n(1.4))], &[], &[(thickness, 4.0)]),
        test("no area: 15,000 m3 over 3 m", &[(volume, n(15_000.0)), (thickness, n(3.0)), (rd, n(1.5))], &[], &[(area, 5_000.0)]),
        test("both measured: left alone", &[(volume, n(1_000.0)), (thickness, n(2.0)), (area, n(400.0)), (rd, n(1.5))], &[], &[(thickness, 2.0), (area, 400.0)]),
    ];
    step("fill_size", script, tests)
}

/// `limits`: each field with a `<field>_default` parameter for the unit takes it where it has no value, is outside
/// `<field>_min` to `<field>_max` (inclusive) or is forced by a minimum equal to its maximum; waste takes its grade
/// defaults outright, having no grids (decisions 0073, point 3, and 0074).
fn limits(skip: &[String], geometry: &[String], ash: Option<&str>) -> DataflowStep {
    let list = |names: &[String]| names.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join(", ");
    let script = format!(
        r#"# Limits and defaults (decisions 0073, point 3, and 0074), as Spry's Initialise sets them. For each field with a
# `<field>_default` parameter for this unit:
# - coal, and any unit's size: the default where it has no value, where it's outside `<field>_min` to `<field>_max`
#   (inclusive), or always where the minimum equals the maximum;
# - waste's grades (qualities, densities, moistures): the default outright, as waste has no grids.
# So an odd value is put right in the parameters, never on the block.
SKIP = [{skip}]
SIZE = [{geometry}]

def formula(unit, p):
    if unit.became != None:
        return
    coal = unit.material.lower() == "coal"
    for name in dir(p):
        if not name.endswith("_default"):
            continue
        field = name[:-len("_default")]
        if field in SKIP or not hasattr(unit, field):
            continue
        value = unit[field]
        low = getattr(p, field + "_min", None)
        high = getattr(p, field + "_max", None)
        forced = low != None and low == high
        if coal or field in SIZE:
            if forced:
                why = "forced to its default"
            elif value == None:
                why = "with none given its default"
            elif (low != None and value < low) or (high != None and value > high):
                why = "outside its limits given its default"
            else:
                continue
            why = ("coal " if coal else "waste ") + why
        else:
            why = "waste given its default"
        unit[field] = getattr(p, name)
        print(field + ": " + why)
"#,
        skip = list(skip),
        geometry = list(geometry),
    );
    let mut tests = Vec::new();
    if let Some(ash) = ash {
        let lim = [(format!("{ash}_min"), n(5.0)), (format!("{ash}_max"), n(10.0)), (format!("{ash}_default"), n(12.0))];
        let p: Vec<(&str, ParamValue)> = lim.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
        tests.push(test("coal outside its limits: 20 % takes 12", &[("material", t("coal")), (ash, n(20.0))], &p, &[(ash, 12.0)]));
        tests.push(test("coal on its limit: 10 % kept", &[("material", t("coal")), (ash, n(10.0))], &p, &[(ash, 10.0)]));
        tests.push(test("waste takes its default outright", &[("material", t("waste")), (ash, n(3.0))], &p, &[(ash, 12.0)]));
    }
    step("limits", script, tests)
}

/// `densities`: in situ, ROM and air-dried densities (decision 0073, point 5), Spry's way (0077, point 5).
fn densities(names: &DensityNames) -> DataflowStep {
    let DensityNames { rd, mad, mis, rd_is, mrom, rd_rom } = names;
    let rom = match (mrom, rd_rom) {
        (Some(mrom), Some(rd_rom)) => format!(
            r#"    if unit.{mis} != None and unit.{mrom} != None:
        unit.{rd_rom} = rd_is * (100 - unit.{mis}) / (100 - unit.{mrom})
    elif coal:
        unit.{rd_rom} = None
    else:
        unit.{rd_rom} = rd_is
"#
        ),
        _ => String::new(),
    };
    let script = format!(
        r#"# In situ, ROM and air-dried densities (decision 0073, point 5), Spry's way (0077, point 5):
# - coal: in situ density by Preston and Sanders from its RD, Mad and Mis, which it must have;
# - waste: its RD over the air-dried-to-in-situ moisture factor where it has both moistures, else its RD;
# - ROM: the in situ density x (100 - Mis) / (100 - Mrom) where it has an Mrom (coal has none without one);
# - air-dried tonnes a cubic metre: the in situ density x (100 - Mis) / (100 - Mad), what qualities are weighed by.
def formula(unit):
    if unit.became != None:
        return
    coal = unit.material.lower() == "coal"
    if coal:
        missing = [name for name in ["{rd}", "{mad}", "{mis}"] if unit[name] == None]
        if missing:
            fail("coal with no " + " or ".join(missing))
        rd_is = in_situ_rd(unit.{rd}, unit.{mad}, unit.{mis})
    elif unit.{rd} == None:
        return
    elif unit.{mad} != None and unit.{mis} != None:
        rd_is = unit.{rd} / moisture_factor(unit.{mad}, unit.{mis})
    else:
        rd_is = unit.{rd}
    unit.{rd_is} = rd_is
{rom}    if unit.{mad} != None and unit.{mis} != None:
        unit.{RD_ADT} = rd_is * (100 - unit.{mis}) / (100 - unit.{mad})
    else:
        unit.{RD_ADT} = rd_is
"#
    );
    // Hand-worked: coal RD 1.4, Mad 3, Mis 8: 1.4 x 97 / (100 + 1.4 x 5 - 8) = 135.8 / 99; air dried x 92 / 97; ROM at
    // Mrom 10, x 92 / 90. Waste RD 2.3 at Mad 2, Mis 6: over the factor 94 / 98, so 2.3 x 98 / 94, and back to 2.3 air dried.
    let rd_is_coal = 135.8 / 99.0;
    let mut coal_unit = vec![("material", t("coal")), (rd.as_str(), n(1.4)), (mad.as_str(), n(3.0)), (mis.as_str(), n(8.0))];
    let mut coal_expect = vec![(rd_is.as_str(), rd_is_coal), (RD_ADT, rd_is_coal * 92.0 / 97.0)];
    if let (Some(mrom), Some(rd_rom)) = (mrom, rd_rom) {
        coal_unit.push((mrom.as_str(), n(10.0)));
        coal_expect.push((rd_rom.as_str(), rd_is_coal * 92.0 / 90.0));
    }
    let tests = vec![
        test("coal by Preston and Sanders", &coal_unit, &[], &coal_expect),
        test(
            "waste over its moisture factor",
            &[("material", t("waste")), (rd.as_str(), n(2.3)), (mad.as_str(), n(2.0)), (mis.as_str(), n(6.0))],
            &[],
            &[(rd_is.as_str(), 2.3 * 98.0 / 94.0), (RD_ADT, 2.3)],
        ),
        test("waste with no moistures: its RD", &[("material", t("waste")), (rd.as_str(), n(2.3))], &[], &[(rd_is.as_str(), 2.3), (RD_ADT, 2.3)]),
    ];
    step("densities", script, tests)
}

/// `discard`: coal set aside before the working sections form, on each seam whole (after `join_benches`), as Spry's
/// Working Section discards it: by hand where its row says `discard` yes, or by its ash over `insitu_ash_cutoff`.
fn discard(ash: &str) -> DataflowStep {
    let script = format!(
        r#"# Discarded coal (Spry's Working Section discard), each seam whole, before the working sections form. A mark the
# working sections read: discarded coal is carried as waste, and always breaks a section.
# - 1: by hand, where its parameter row says `discard` yes;
# - 2: its ash over `insitu_ash_cutoff`;
# - 0: kept.
def formula(unit, p):
    if unit.material.lower() != "coal" or unit.became != None:
        return
    by_hand = str(getattr(p, "discard", "")).strip().lower() in ["yes", "y", "true", "1"]
    cutoff = getattr(p, "insitu_ash_cutoff", None)
    if by_hand:
        unit.{DISCARDED} = 1
    elif cutoff != None and unit.{ash} != None and unit.{ash} > cutoff:
        unit.{DISCARDED} = 2
    else:
        unit.{DISCARDED} = 0
"#
    );
    let coal = |a: f64| vec![("material", t("coal")), (ash, n(a))];
    let tests = vec![
        test("by hand", &coal(12.0), &[("discard", t("Yes")), ("insitu_ash_cutoff", n(30.0))], &[(DISCARDED, 1.0)]),
        test("ash 40 % over a 30 % cut-off", &coal(40.0), &[("insitu_ash_cutoff", n(30.0))], &[(DISCARDED, 2.0)]),
        test("ash 12 % kept", &coal(12.0), &[("insitu_ash_cutoff", n(30.0))], &[(DISCARDED, 0.0)]),
        test("no cut-off: kept", &coal(40.0), &[], &[(DISCARDED, 0.0)]),
    ];
    step("discard", script, tests)
}

/// The names `rom_qualities` is written for; any it hasn't, its part is left out.
struct RomNames {
    rd: Option<String>,
    mad: Option<String>,
    mis: Option<String>,
    mrom: Option<String>,
    rd_is: Option<String>,
    rd_rom: Option<String>,
    length: Option<String>,
    qualities: Vec<String>,
}

/// `rom_qualities`: Spry's ROM Dilution arithmetic after the cut-off, on what came into a ROM coal section that isn't its
/// coal, against the section's coal added up by tonnes; then every unit's ROM density and air-dried tonnes reworked.
fn rom_qualities(r: &RomNames) -> DataflowStep {
    let list = |names: &[&String]| names.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join(", ");
    let moistures: Vec<&String> = [&r.mad, &r.mis, &r.mrom].into_iter().flatten().collect();
    let qualities: Vec<&String> = r.qualities.iter().collect();
    let mut body = String::from(
        "    coal = unit.rom_coal\n    if coal != None and not (unit.material.lower() == \"coal\" and unit.became == None):\n        for m in MOISTURES:\n            if unit[m] == None and getattr(coal, m) != None:\n                unit[m] = getattr(coal, m)\n                print(\"dilution and partings given their coal's \" + m)\n",
    );
    if let Some(mad) = &r.mad {
        let rd = r.rd.as_ref().map(|rd| format!("            if unit.{rd} != None:\n                unit.{rd} = unit.{rd} / factor\n")).unwrap_or_default();
        body.push_str("        align = str(getattr(p, \"align_dilution_mad\", \"\")).strip().lower() in [\"yes\", \"y\", \"true\", \"1\"]\n");
        body.push_str(&format!("        own, theirs = unit.{mad}, coal.{mad}\n"));
        body.push_str("        if align and own != None and theirs != None and own != theirs:\n            factor = (100 - theirs) / (100 - own)\n            for q in QUALITIES:\n                if unit[q] != None:\n                    unit[q] = unit[q] * factor\n");
        body.push_str(&rd);
        body.push_str(&format!("            unit.{mad} = theirs\n"));
        body.push_str("            print(\"dilution's air-dried moisture aligned with its coal's\")\n");
    }
    body.push_str("        for q in QUALITIES:\n            if unit[q] != None and unit[q] < 0 and getattr(coal, q) != None:\n                unit[q] = getattr(coal, q)\n                print(\"dilution's negative \" + q + \" given its coal's\")\n");
    if let Some(length) = &r.length {
        body.push_str(&format!("    if coal != None and coal.{length} != None:\n        unit.{length} = coal.{length}\n"));
    }
    if let (Some(rd_is), Some(mis), Some(mrom), Some(rd_rom)) = (&r.rd_is, &r.mis, &r.mrom, &r.rd_rom) {
        body.push_str(&format!("    if unit.{rd_is} != None and unit.{mis} != None and unit.{mrom} != None:\n        unit.{rd_rom} = unit.{rd_is} * (100 - unit.{mis}) / (100 - unit.{mrom})\n"));
    }
    if let (Some(rd_is), Some(mis), Some(mad)) = (&r.rd_is, &r.mis, &r.mad) {
        body.push_str(&format!("    if unit.{rd_is} != None and unit.{mis} != None and unit.{mad} != None:\n        unit.{RD_ADT} = unit.{rd_is} * (100 - unit.{mis}) / (100 - unit.{mad})\n"));
    }
    let head = "# ROM qualities (Spry's ROM Dilution, after the ROM ash cut-off), on what came into a ROM coal section that isn't its\n\
# coal (dilution, partings, coal lost into it from a neighbour), against the section's coal added up by tonnes:\n\
# - each moisture it has none of takes the coal's;\n\
# - where its row says `align_dilution_mad` yes, its air-dried moisture takes the coal's, Spry's way: each quality\n\
#   scaled by (100 - Mad coal) / (100 - Mad own), its RD divided by the same, so its in situ density holds;\n\
# - a negative quality (a site's way of saying \"as the coal\") takes the coal's.\n\
# Then each unit in a ROM coal section takes the section's length (its coal's largest), and every unit's ROM density,\n\
# in situ x (100 - Mis) / (100 - Mrom), and its air-dried tonnes a cubic metre, in situ x (100 - Mis) / (100 - Mad), are\n\
# worked again.\n";
    let script = format!("{head}MOISTURES = [{}]\nQUALITIES = [{}]\n\ndef formula(unit, p):\n{body}", list(&moistures), list(&qualities));
    let mut tests = Vec::new();
    if let (Some(mad), Some(mis), Some(mrom), Some(rd_is), Some(rd_rom), Some(q)) = (&r.mad, &r.mis, &r.mrom, &r.rd_is, &r.rd_rom, r.qualities.first()) {
        let coal: Vec<(&str, ParamValue)> = vec![(mad.as_str(), n(3.0)), (mis.as_str(), n(8.0)), (mrom.as_str(), n(10.0)), (q.as_str(), n(12.0))];
        let dilution = |own_mad: Option<f64>, quality: f64| {
            let mut u = vec![("material", t("waste")), ("became", t("dilution")), (rd_is.as_str(), n(2.3)), (q.as_str(), n(quality))];
            if let Some(m) = own_mad {
                u.push((mad.as_str(), n(m)));
            }
            u
        };
        let mut with_coal = |name: &str, unit: Vec<(&str, ParamValue)>, params: &[(&str, ParamValue)], expect: &[(&str, f64)]| {
            let mut t = test(name, &unit, params, expect);
            t.rom_coal = coal.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
            tests.push(t);
        };
        // Dilution of in situ density 2.3 with no moistures takes the coal's 3, 8 and 10 %: ROM density 2.3 x 92 / 90, air
        // dried 2.3 x 92 / 97.
        with_coal("dilution takes its coal's moistures", dilution(None, 80.0), &[], &[(mad.as_str(), 3.0), (mis.as_str(), 8.0), (mrom.as_str(), 10.0), (rd_rom.as_str(), 2.3 * 92.0 / 90.0), (RD_ADT, 2.3 * 92.0 / 97.0)]);
        // At Mad 2 aligned with the coal's 3: its 80 % ash x 97 / 98.
        with_coal("Mad aligned, Spry's way", dilution(Some(2.0), 80.0), &[("align_dilution_mad", t("yes"))], &[(mad.as_str(), 3.0), (q.as_str(), 80.0 * 97.0 / 98.0)]);
        with_coal("Mad left where not asked", dilution(Some(2.0), 80.0), &[], &[(mad.as_str(), 2.0), (q.as_str(), 80.0)]);
        with_coal("a negative quality takes its coal's", dilution(None, -1.0), &[], &[(q.as_str(), 12.0)]);
    }
    step("rom_qualities", script, tests)
}

struct DensityNames {
    rd: String,
    mad: String,
    mis: String,
    rd_is: String,
    mrom: Option<String>,
    rd_rom: Option<String>,
}

/// The starter formulas for a dataflow, as it names its fields, each with the built-in step it follows: after
/// `initialise`, `fill_size` where it has an area, `limits` always, and `densities` where it has the moistures and an in
/// situ density to work; after `join_benches`, `discard` where it forms working sections from an ash; after `rom`,
/// `rom_qualities`.
pub fn starter_formulas(d: &Dataflow) -> Vec<(&'static str, DataflowStep)> {
    let has_step = |name: &str| d.steps.iter().any(|s| s.step == name);
    let mut steps: Vec<(&'static str, DataflowStep)> = Vec::new();
    let volume = named(d, FieldKind::Volume).unwrap_or_else(|| "volume".into());
    let thickness = named(d, FieldKind::Thickness).unwrap_or_else(|| "thickness".into());
    let rd = setting_field(d, "rd").or_else(|| d.fields.iter().find(|f| f.field.kind == FieldKind::Density && matches!(f.source, FieldSource::Attribute { .. })).map(|f| f.field.name.clone()));
    if let (Some(area), Some(rd)) = (named(d, FieldKind::Area), rd.clone()) {
        steps.push(("initialise", fill_size(&volume, &thickness, &area, &rd)));
    }
    let skip: Vec<String> = d.fields.iter().filter(|f| matches!(f.field.kind, FieldKind::Volume | FieldKind::Setting)).map(|f| f.field.name.clone()).collect();
    let geometry: Vec<String> = d.fields.iter().filter(|f| matches!(f.field.kind, FieldKind::Thickness | FieldKind::Area | FieldKind::Length)).map(|f| f.field.name.clone()).collect();
    let ash = d.fields.iter().find(|f| f.field.kind == FieldKind::Quality).map(|f| f.field.name.clone());
    steps.push(("initialise", limits(&skip, &geometry, ash.as_deref())));
    if let (Some(rd), Some(mad), Some(mis), Some(rd_is)) = (rd.clone(), setting_field(d, "mad"), setting_field(d, "mis"), setting_field(d, "rd_is")) {
        steps.push(("initialise", densities(&DensityNames { rd, mad, mis, rd_is, mrom: setting_field(d, "mrom"), rd_rom: setting_field(d, "rd_rom") })));
    }
    let ws_ash = d.steps.iter().find(|s| s.step == "working_sections").map(|s| s.setting("ash").to_string());
    if let (true, Some(a)) = (has_step("join_benches"), ws_ash.filter(|a| d.fields.iter().any(|f| &f.field.name == a))) {
        steps.push(("join_benches", discard(&a)));
    }
    if has_step("rom") {
        let qualities = d.fields.iter().filter(|f| f.field.kind == FieldKind::Quality).map(|f| f.field.name.clone()).collect();
        let r = RomNames {
            rd,
            mad: setting_field(d, "mad"),
            mis: setting_field(d, "mis"),
            mrom: setting_field(d, "mrom"),
            rd_is: setting_field(d, "rd_is"),
            rd_rom: setting_field(d, "rd_rom"),
            length: named(d, FieldKind::Length),
            qualities,
        };
        steps.push(("rom", rom_qualities(&r)));
    }
    let names: Vec<String> = d.fields.iter().map(|f| f.field.name.clone()).collect();
    for (_, s) in steps.iter_mut() {
        if let Some(f) = s.formula.as_mut() {
            f.script = for_names(&f.script, &names);
        }
    }
    steps
}

/// A dataflow given the starter formulas after `initialise` (none it already has, by name). Where it gets `densities`,
/// it also gets the `rd_adt` field that works into, and its qualities and Mad are weighed by it, as Spry weighs them by
/// air-dried tonnes (decision 0077, point 5).
pub fn with_starter_formulas(d: &mut Dataflow) {
    let new: Vec<(&str, DataflowStep)> = starter_formulas(d).into_iter().filter(|(_, s)| !d.steps.iter().any(|t| t.step == s.step)).collect();
    if new.iter().any(|(_, s)| s.step == "discard") && !d.fields.iter().any(|f| f.field.name == DISCARDED) {
        d.fields.push(DataflowField { field: Field::new(DISCARDED, FieldKind::Other, AddUp::Max), source: FieldSource::Worked });
    }
    if new.iter().any(|(_, s)| s.step == "densities") && !d.fields.iter().any(|f| f.field.name == RD_ADT) {
        let rd = setting_field(d, "rd");
        for f in d.fields.iter_mut() {
            if let (AddUp::Mass { density }, Some(rd)) = (&mut f.field.add_up, &rd)
                && density == rd
            {
                *density = RD_ADT.to_string();
            }
        }
        let after = d.fields.iter().position(|f| Some(&f.field.name) == setting_field(d, "rd_is").as_ref()).map_or(d.fields.len(), |i| i + 1);
        // It starts as the block's RD, so Imported, written before any step, weighs qualities as before, "as read"; the
        // densities formula then works it.
        let source = rd.as_ref().and_then(|rd| d.fields.iter().find(|f| &f.field.name == rd)).map(|f| f.source.clone()).unwrap_or(FieldSource::Worked);
        d.fields.insert(after, DataflowField { field: Field::new(RD_ADT, FieldKind::Density, AddUp::Volume), source });
    }
    // Each after its stage, in order, after any formula already there.
    for (anchor, s) in new {
        let Some(mut at) = d.steps.iter().position(|t| t.step == anchor) else { continue };
        while d.steps.get(at + 1).is_some_and(|t| t.formula.is_some()) {
            at += 1;
        }
        d.steps.insert(at + 1, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, kind: FieldKind, add_up: AddUp, source: FieldSource) -> DataflowField {
        DataflowField { field: Field::new(name, kind, add_up), source }
    }

    fn attr(name: &str) -> FieldSource {
        FieldSource::Attribute { attribute: name.to_string() }
    }

    /// The usual fields; with `moist`, the moistures and worked densities too.
    fn flow(moist: bool) -> Dataflow {
        let mass = |d: &str| AddUp::Mass { density: d.to_string() };
        let mut fields = vec![
            field("volume", FieldKind::Volume, AddUp::Sum, FieldSource::Volume),
            field("thickness", FieldKind::Thickness, AddUp::Sum, FieldSource::Thickness),
            field("area", FieldKind::Area, AddUp::Max, FieldSource::PlanArea),
            field("rd", FieldKind::Density, AddUp::Volume, attr("rd")),
            field("ash", FieldKind::Quality, mass("rd"), attr("ash")),
        ];
        if moist {
            fields.push(field("mad", FieldKind::Moisture, mass("rd"), attr("mad")));
            fields.push(field("rd_is", FieldKind::Density, AddUp::Volume, FieldSource::Worked));
            fields.push(field("rd_rom", FieldKind::Density, AddUp::Volume, FieldSource::Worked));
            fields.push(field("mis", FieldKind::Moisture, mass("rd_is"), attr("mis")));
            fields.push(field("mrom", FieldKind::Moisture, mass("rd_rom"), FieldSource::Parameter { column: "mrom".into() }));
        }
        Dataflow { fields, steps: vec![DataflowStep::on("initialise"), DataflowStep::on("in_situ")], ..Dataflow::default() }
    }

    fn checked(d: &Dataflow) -> usize {
        let fields = d.engine_fields().expect("fields hold together");
        let built_in: Vec<&str> = super::super::STEPS.iter().map(|(n, _)| *n).collect();
        super::super::check_formula_steps(d, &built_in, &super::super::formula_declared(&fields, &[])).expect("the starter formulas compile and give their known answers").len()
    }

    #[test]
    fn the_starter_formulas_compile_and_give_their_known_answers() {
        let mut d = flow(true);
        with_starter_formulas(&mut d);
        let steps: Vec<&str> = d.steps.iter().map(|s| s.step.as_str()).collect();
        assert_eq!(steps, ["initialise", "fill_size", "limits", "densities", "in_situ"]);
        assert_eq!(checked(&d), 3);
        // With the rest of the steps: discard after the benches are joined, the ROM qualities after rom.
        let mut whole = flow(true);
        whole.steps = super::super::STEPS.iter().map(|(n, _)| DataflowStep::on(n)).collect();
        with_starter_formulas(&mut whole);
        let at = |name: &str| whole.steps.iter().position(|s| s.step == name).unwrap();
        assert_eq!(at("discard"), at("join_benches") + 1);
        assert_eq!(at("rom_qualities"), at("rom") + 1);
        assert!(whole.fields.iter().any(|f| f.field.name == DISCARDED));
        assert_eq!(checked(&whole), 5);
        // Air-dried tonnes Spry's way: rd_adt worked, starting as the block's RD; ash and Mad weighed by it.
        let rd_adt = d.fields.iter().find(|f| f.field.name == RD_ADT).expect("rd_adt");
        assert_eq!(rd_adt.source, attr("rd"));
        for name in ["ash", "mad"] {
            let f = d.fields.iter().find(|f| f.field.name == name).unwrap();
            assert_eq!(f.field.add_up, AddUp::Mass { density: RD_ADT.into() }, "{name}");
        }
        // Given twice, nothing is added twice.
        let before = d.clone();
        with_starter_formulas(&mut d);
        assert_eq!(d, before);
    }

    /// Vulcan's ash code is `as`, a reserved word: the starter writes it `unit["as"]`, and the formulas still compile and
    /// give their answers.
    #[test]
    fn a_field_named_a_reserved_word_is_written_in_brackets() {
        let mut d = flow(true);
        for f in d.fields.iter_mut().filter(|f| f.field.name == "ash") {
            f.field.name = "as".into();
        }
        d.steps = super::super::STEPS.iter().map(|(n, _)| DataflowStep::on(n)).collect();
        for s in d.steps.iter_mut().filter(|s| s.step == "working_sections") {
            s.settings.insert("ash".into(), "as".into());
        }
        with_starter_formulas(&mut d);
        let discard = d.steps.iter().find(|s| s.step == "discard").and_then(|s| s.formula.as_ref()).expect("discard");
        assert!(discard.script.contains(r#"unit["as"] > cutoff"#) && !discard.script.contains("unit.as"), "{}", discard.script);
        assert_eq!(checked(&d), 5);
        assert_eq!(for_names("x = unit.as_ad + unit.as + coal.as", &["as".into()]), r#"x = unit.as_ad + unit["as"] + getattr(coal, "as", None)"#);
    }

    #[test]
    fn a_dataflow_without_moistures_gets_size_and_limits_only() {
        let mut d = flow(false);
        with_starter_formulas(&mut d);
        let steps: Vec<&str> = d.steps.iter().map(|s| s.step.as_str()).collect();
        assert_eq!(steps, ["initialise", "fill_size", "limits", "in_situ"]);
        assert_eq!(checked(&d), 2);
        assert!(!d.fields.iter().any(|f| f.field.name == RD_ADT));
    }
}
