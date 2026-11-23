//! Add waste allocation (the Waste allocation plan, decision 7, workstream 5): the template's way of filling Spry's
//! `Scheduled` sections, added to a dataflow in one action. Its schedule fields (a section a process: prime, rehandle,
//! total and swelled volume), its word fields, four parameter tables with default rows, six allocation steps after `rom`
//! and a check step after `final`. It's one way of piecing a mine plan together, rebuilt from what the template does:
//! every step is a script a site reads, edits or replaces, built only from the engine's general calls (intervals, cuts
//! across units, splits, lookups on a grid), and every number it uses comes from a parameter row (decision 8).

use zd_core::{Dataflow, DataflowField, DataflowStep, FieldSource, ObjectId, WordField};
use zd_reserves::{AddUp, Field, FieldKind};

use crate::{CommandError, RunRecord};
use zd_core::{Body, ObjectKind};

/// The processes the starter fills, each its own section under `Scheduled\Waste`, in the order a column is dug.
pub const PROCESSES: [&str; 9] = ["Topsoil", "Freedig", "Excavator", "Partings", "Wedge", "Cast", "Dozer Push", "Dragline", "Poststrip"];

/// What each process's section holds.
pub const QUANTITIES: [&str; 4] = ["Prime Volume", "Rehandle Volume", "Total Volume", "Swelled Volume"];

/// The other schedule fields: drill and blast placed onto the blasting horizon, coal mining and coal exposed.
pub const OTHER_FIELDS: [&str; 3] = ["Waste\\Drill and Blast\\Volume", "Coal\\Mining\\Volume", "Coal\\Exposed\\Volume"];

/// The words each unit carries through the allocation.
pub const WORDS: [&str; 5] = ["process", "zone", "drill_type", "blasting_seam", "blasting_bench"];

/// The settings table, read as `p` by the column's own levels; the others are looked up by name.
pub const SETTINGS: &str = "Waste allocation settings";
pub const FACTORS: &str = "Dragline factors";
pub const PROCESS_TABLE: &str = "Waste processes";
pub const OVERRIDES: &str = "Short term overrides";

/// The four tables and their default rows, each a site's to edit: one settings row; a dragline factors grid, one set of
/// factors a zone at two dips and two zone thicknesses for a site to fill; each process's rehandle and swell; and no
/// short-term overrides.
pub fn tables() -> Vec<(&'static str, String)> {
    let processes: String = PROCESSES.iter().map(|p| format!("{p},0,1.25\n")).collect();
    let mut factors = String::from("when method,when zone,dip (deg),zone_thickness (m),cast,dozer_push,dragline,poststrip,dragline_total\n");
    for (zone, shares) in [(1, "0.2,0.2,0.6,0,0.8"), (2, "0,0,0.9,0.1,1.2")] {
        for dip in [0, 30] {
            for thickness in [0, 60] {
                factors.push_str(&format!("2 zone,{zone},{dip},{thickness},{shares}\n"));
            }
        }
    }
    vec![
        (
            SETTINGS,
            "topsoil_thickness (m),freedig_thickness (m),min_drill_thickness (m),min_burden (m),system_max (m),zones,method,dip (deg),partings_max_thickness (m),wedge_thickness (m),check_tolerance (m3)\n0.5,4.5,3,10,40,2,2 zone,12,3,2,0.01\n".to_string(),
        ),
        (FACTORS, factors),
        (PROCESS_TABLE, format!("when process,rehandle,swell_factor\n{processes}")),
        (OVERRIDES, "when strip,when block,override_thickness (m),excavator,dozer_push,dragline\n".to_string()),
    ]
}

/// Loose ground (the template's Topsoil and Freedig): off the top of the column's first waste interval.
pub const LOOSE_GROUND: &str = r#"# Loose ground: topsoil, then freedig, off the top of the column's first waste interval, each thickness from the
# Waste allocation settings row; a thickness that ends inside a unit splits it there.
def allocate(column, p):
    intervals = column.intervals()
    if not intervals:
        return
    left = intervals[0]
    for process, thickness in [("Topsoil", p.topsoil_thickness), ("Freedig", p.freedig_thickness)]:
        if not left or thickness <= 0:
            continue
        taken = column.take_from_top(left, min(thickness, column.total("thickness", units=left)))
        for u in taken:
            u.process = process
        left = [u for u in left if u not in taken]
"#;

/// Short-term overrides: where a row fits the column, the bottom of each waste over coal split by fixed shares.
pub const SHORT_TERM_OVERRIDES: &str = r#"# Short-term overrides: where a row of Short term overrides fits the column's strip and block, the bottom of each
# waste interval over coal, to the row's override thickness, split by its shares between the excavator, dozer push and
# dragline. No row, nothing done.
def allocate(column, p):
    keys = {"strip": column.block.strip, "block": column.block.block}
    thickness = lookup("Short term overrides", "override_thickness", default=0, **keys)
    if thickness <= 0:
        return
    shares = [lookup("Short term overrides", name, **keys) for name in ["excavator", "dozer_push", "dragline"]]
    coal = column.coal()
    for interval in column.intervals():
        below = column.after(interval[-1])
        units = [u for u in interval if u.process == None]
        if below == None or below not in coal or not units:
            continue
        taken = column.take_from_bottom(units, min(thickness, column.total("thickness", units=units)))
        for u in taken:
            for piece, process in zip(u.split_shares(shares), ["Excavator", "Dozer Push", "Dragline"]):
                if piece != None:
                    piece.process = process
"#;

/// The dragline system by zones, from the bottom up, each split by its method's factors read off a grid.
pub const DRAGLINE_SYSTEM: &str = r#"# The dragline system: each waste interval over coal at least the minimum burden thick, up to the system's maximum
# thickness, in its zones from the bottom up (zone 1 the lowest), taken across the interval's units. Each zone is split
# by its method's factors, read off Dragline factors by method and zone, nearest the dip and between the zone
# thicknesses either side, into cast, dozer push, dragline and poststrip; the dragline's rehandle from its total factor.
def allocate(column, p):
    coal = column.coal()
    zones = int(p.zones)
    for interval in column.intervals():
        below = column.after(interval[-1])
        units = [u for u in interval if u.process == None]
        if below == None or below not in coal or not units:
            continue
        thickness = column.total("thickness", units=units)
        if thickness < p.min_burden:
            continue
        zone_thickness = min(thickness, p.system_max) / zones
        for n in range(1, zones + 1):
            if not units:
                break
            zone = column.take_from_bottom(units, min(zone_thickness, column.total("thickness", units=units)))
            units = [u for u in units if u not in zone]
            f = lambda name: lookup("Dragline factors", name, method=p.method, zone=n, dip=nearest(p.dip), zone_thickness=between(zone_thickness))
            for u in zone:
                u.zone = "Zone %d" % n
                cast, dozer, dragline, poststrip = u.split_shares([f("cast"), f("dozer_push"), f("dragline"), f("poststrip")])
                for piece, process in [(cast, "Cast"), (dozer, "Dozer Push"), (dragline, "Dragline"), (poststrip, "Poststrip")]:
                    if piece != None:
                        piece.process = process
                if dragline != None:
                    prime = dragline.volume
                    dragline["Waste\\Dragline\\Rehandle Volume"] = prime * f("dragline_total") / f("dragline") - prime
"#;

/// What's left: partings between coal, a wedge over coal, the rest the excavator's.
pub const EXCAVATOR: &str = r#"# What the dragline system left: an interval between two coal sections no thicker than the partings maximum is
# Partings; a wedge off the bottom of what's left over coal; the rest Excavator.
def allocate(column, p):
    coal = column.coal()
    for interval in column.intervals():
        units = [u for u in interval if u.process == None]
        if not units:
            continue
        above = column.before(interval[0])
        below = column.after(interval[-1])
        between_coal = above != None and above in coal and below != None and below in coal
        if between_coal and column.total("thickness", units=interval) <= p.partings_max_thickness:
            for u in units:
                u.process = "Partings"
            continue
        last = column.after(units[-1])
        if last != None and last in coal and p.wedge_thickness > 0:
            wedge = column.take_from_bottom(units, min(p.wedge_thickness, column.total("thickness", units=units)))
            for u in wedge:
                u.process = "Wedge"
            units = [u for u in units if u not in wedge]
        for u in units:
            u.process = "Excavator"
"#;

/// Blasting: drilled from each interval's top, ripped where too thin, the drill and blast placed onto the horizon.
pub const BLASTING: &str = r#"# Blasting: each waste interval's units below the loose ground, drilled from the top of the interval. Thinner in all
# than the minimum drill thickness, ripped; else blasted, each unit's drill and blast volume placed onto the blasting
# horizon, the leaves the interval's top unit is written on.
def allocate(column, p):
    for interval in column.intervals():
        units = [u for u in interval if u.process not in ["Topsoil", "Freedig"]]
        if not units:
            continue
        ripped = column.total("thickness", units=units) < p.min_drill_thickness
        top = units[0]
        for u in units:
            if ripped:
                u.drill_type = "Ripped"
                continue
            u.drill_type = "Blast"
            u.blasting_seam = top.seam
            u.blasting_bench = top.bench
            u.place_onto("Waste\\Drill and Blast\\Volume", u.volume, unit=top)
"#;

/// Each process's quantities, coal mining and coal exposed.
pub const WASTE_QUANTITIES: &str = r#"# Each process's quantities on its units: prime the unit's volume, rehandle its share from Waste processes (the
# dragline's from its factors), total and swelled by its swell factor. Each coal unit's mining volume, and each coal
# section's volume as coal exposed on the waste unit above it.
def allocate(column, p):
    for u in column.waste():
        if u.process == None:
            u.process = "Excavator"
        field = "Waste\\" + u.process + "\\"
        prime = u.volume
        u[field + "Prime Volume"] = prime
        rehandle = u[field + "Rehandle Volume"]
        if rehandle == None:
            rehandle = prime * lookup("Waste processes", "rehandle", process=u.process)
            u[field + "Rehandle Volume"] = rehandle
        u[field + "Total Volume"] = prime + rehandle
        u[field + "Swelled Volume"] = (prime + rehandle) * lookup("Waste processes", "swell_factor", process=u.process)
    for section in column.intervals(ends="coal"):
        for u in section:
            u["Coal\\Mining\\Volume"] = u.volume
        above = column.before(section[0])
        exposer = above if above != None and above.process != None else section[0]
        exposer["Coal\\Exposed\\Volume"] = (exposer["Coal\\Exposed\\Volume"] or 0) + column.total("volume", units=section)
"#;

/// The checks, a table script step after `final`.
pub const CHECKS: &str = r#"# The waste allocation's checks on every leaf: the processes' prime volumes add to the scheduled waste, and the coal
# mining to the scheduled coal, within the settings row's tolerance; and every leaf of waste has a process.
def run(t, p):
    primes = [c for c in t.columns if c.startswith("Scheduled\\Waste\\") and c.endswith("\\Prime Volume")]
    if primes:
        check("waste processes", equals="Scheduled\\Waste\\volume", tolerance=p.check_tolerance, *primes)
    if "Scheduled\\Coal\\Mining\\Volume" in t.columns:
        check("coal mining", "Scheduled\\Coal\\Mining\\Volume", equals="Scheduled\\Coal\\volume", tolerance=p.check_tolerance)
    required("Scheduled\\Waste\\process", material="waste", name="a process on every waste leaf")
    return t
"#;

/// The allocation steps in the order they run, after `rom` and before `final`.
pub const STEPS: [(&str, &str); 6] = [
    ("loose_ground", LOOSE_GROUND),
    ("short_term_overrides", SHORT_TERM_OVERRIDES),
    ("dragline_system", DRAGLINE_SYSTEM),
    ("excavator_partings_wedge", EXCAVATOR),
    ("blasting", BLASTING),
    ("waste_quantities", WASTE_QUANTITIES),
];

/// The check step's name.
pub const CHECK_STEP: &str = "waste_checks";

/// Every script the starter adds, by step name.
pub fn scripts() -> Vec<(&'static str, &'static str)> {
    STEPS.iter().copied().chain(std::iter::once((CHECK_STEP, CHECKS))).collect()
}

/// The schedule fields the starter adds: each process's section, then drill and blast, coal mining and coal exposed.
pub fn schedule_fields() -> Vec<String> {
    PROCESSES.iter().flat_map(|p| QUANTITIES.iter().map(move |q| format!("Waste\\{p}\\{q}"))).chain(OTHER_FIELDS.iter().map(|f| f.to_string())).collect()
}

/// The dataflow with waste allocation added: the schedule and word fields it hasn't, the allocation steps before
/// `final`, the check step last, and the settings table read after its own. Refused where it has no `rom` and `final`,
/// or has one of the steps already. Gives back what was added, in words.
pub fn with_waste_allocation(d: &mut Dataflow, settings: ObjectId) -> Result<Vec<String>, String> {
    let has = |name: &str| d.steps.iter().any(|s| s.step == name && s.is_built_in());
    if !has("rom") || !has("final") {
        return Err("waste allocation goes after rom and before final, and the dataflow hasn't both".into());
    }
    if let Some(s) = d.steps.iter().find(|s| scripts().iter().any(|(n, _)| *n == s.step)) {
        return Err(format!("the dataflow has a step {} already: waste allocation is added once", s.step));
    }
    let mut added = Vec::new();
    let mut fields = 0;
    for name in schedule_fields() {
        if !d.fields.iter().any(|f| f.field.name == name) {
            d.fields.push(DataflowField { field: Field::new(&name, FieldKind::Other, AddUp::Sum), source: FieldSource::Worked });
            fields += 1;
        }
    }
    added.push(format!("{fields} schedule fields"));
    let mut words = 0;
    for name in WORDS {
        if !d.words.iter().any(|w| w.name == name) {
            d.words.push(WordField::new(name, FieldSource::Worked));
            words += 1;
        }
    }
    added.push(format!("{words} word fields"));
    let at = d.steps.iter().position(|s| s.step == "final" && s.is_built_in()).expect("final, checked above");
    for (k, (name, script)) in STEPS.iter().enumerate() {
        d.steps.insert(at + k, DataflowStep::allocation(name, script));
    }
    d.steps.push(DataflowStep::table_script(CHECK_STEP, CHECKS));
    added.push(format!("{} allocation steps and a check step", STEPS.len()));
    if !d.parameters.contains(&settings) {
        d.parameters.push(settings);
    }
    Ok(added)
}

impl crate::Session {
    /// Adds waste allocation to a dataflow (the Waste allocation plan, decision 7): its four parameter tables saved where
    /// the project hasn't one of that name (one it has is kept, as a site edited it), then the dataflow saved with the
    /// fields, steps and checks added. A run each, each undone on its own. Gives back the tables' ids, settings first.
    pub fn add_waste_allocation(&mut self, dataflow: ObjectId) -> Result<Vec<ObjectId>, CommandError> {
        let object = self.project().object(dataflow).cloned().ok_or(CommandError::MissingObject(dataflow))?;
        let Body::Dataflow(d) = &object.body else {
            return Err(CommandError::WrongKind { id: dataflow, expected: ObjectKind::Dataflow, found: object.kind() });
        };
        let mut d = (**d).clone();
        // Checked before anything is saved, so a refusal leaves the project as it was.
        with_waste_allocation(&mut d.clone(), ObjectId::new()).map_err(CommandError::Invalid)?;
        let made = |record: RunRecord| record.outputs.iter().find(|o| o.change == zd_core::ChangeKind::Created).map(|o| o.id).ok_or_else(|| CommandError::Invalid("nothing was made".into()));
        let mut ids = Vec::new();
        for (name, csv) in tables() {
            let kept = self.project().objects().find(|o| o.name == name && matches!(o.body, Body::ParameterTable(_))).map(|o| o.id);
            let id = match kept {
                Some(id) => id,
                None => made(self.run("parameters.save", serde_json::json!({ "name": name, "csv": csv }))?)?,
            };
            ids.push(id);
        }
        with_waste_allocation(&mut d, ids[0]).map_err(CommandError::Invalid)?;
        self.run("dataflow.save", serde_json::json!({ "dataflow": dataflow.to_string(), "name": object.name, "content": d }))?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decision 8: the starter's scripts hold the order and the logic, every number from a parameter row. No number in
    /// any of them but 0 and 1.
    #[test]
    fn every_starter_script_reads_its_numbers_from_parameter_rows() {
        for (name, script) in scripts() {
            let mut number = String::new();
            let mut found = Vec::new();
            let mut prev = ' ';
            for c in script.chars().chain(std::iter::once(' ')) {
                if c.is_ascii_digit() || (c == '.' && !number.is_empty()) {
                    if number.is_empty() && (prev.is_ascii_alphanumeric() || prev == '_') {
                        prev = c;
                        continue;
                    }
                    number.push(c);
                } else {
                    if !number.is_empty() && !matches!(number.trim_end_matches('.'), "0" | "1") {
                        found.push(number.clone());
                    }
                    number.clear();
                }
                prev = c;
            }
            assert!(found.is_empty(), "{name} has numbers of its own: {found:?}");
        }
    }

    /// The fields, words and steps go in once, before final and after it; a second time is refused, as is a dataflow
    /// without rom and final.
    #[test]
    fn waste_allocation_is_added_once_where_it_belongs() {
        let mut d = Dataflow {
            levels: zd_core::default_levels(),
            fields: vec![
                DataflowField { field: Field::new("volume", FieldKind::Volume, AddUp::Sum), source: FieldSource::Volume },
                DataflowField { field: Field::new("thickness", FieldKind::Thickness, AddUp::Sum), source: FieldSource::Thickness },
            ],
            steps: ["initialise", "rom", "final"].iter().map(|s| DataflowStep::on(s)).collect(),
            parameters: Vec::new(),
            scripts: Vec::new(),
            lookups: Vec::new(),
            words: Vec::new(),
        };
        let settings = ObjectId::new();
        let said = with_waste_allocation(&mut d, settings).unwrap();
        assert_eq!(said, ["39 schedule fields", "5 word fields", "6 allocation steps and a check step"]);
        let names: Vec<&str> = d.steps.iter().map(|s| s.step.as_str()).collect();
        assert_eq!(names, ["initialise", "rom", "loose_ground", "short_term_overrides", "dragline_system", "excavator_partings_wedge", "blasting", "waste_quantities", "final", "waste_checks"]);
        assert_eq!(d.parameters, [settings]);
        d.check().unwrap();
        assert!(with_waste_allocation(&mut d, settings).unwrap_err().contains("has a step loose_ground already"));
        d.steps.retain(|s| s.step != "rom");
        assert!(with_waste_allocation(&mut d, settings).unwrap_err().contains("hasn't both"));
        for (name, csv) in tables() {
            zd_core::ParameterTable::from_csv(&csv).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}
