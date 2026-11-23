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
/// The measured fields its scripts read off the blocks (decision 3 of the Table setup plan): added to a main dataflow
/// that hasn't them, each a block attribute `solid.measure` writes, so a table can take another source instead.
pub const MEASURED: [(&str, FieldKind); 2] = [("dip", FieldKind::Other), ("width", FieldKind::Length)];

pub const WORDS: [&str; 6] = ["process", "zone", "method", "drill_type", "blasting_seam", "blasting_bench"];

/// The settings table, read as `p` by the column's own levels; the others are looked up by name.
pub const SETTINGS: &str = "Waste allocation settings";
pub const FACTORS: &str = "Dragline factors";
pub const PROCESS_TABLE: &str = "Waste processes";
pub const OVERRIDES: &str = "Short term overrides";
pub const METHODS: &str = "System methods";

/// The five tables and their default rows, each a site's to edit: one settings row; the system's methods, the most zones
/// a column's thickness reaches choosing one; a dragline factors grid, one set of factors a zone at two dips, widths, coal
/// thicknesses and zone thicknesses for a site to fill; each process's rehandle and swell; and no short-term overrides.
pub fn tables() -> Vec<(&'static str, String)> {
    let processes: String = PROCESSES.iter().map(|p| format!("{p},0,1.25\n")).collect();
    let mut factors = String::from("when method,when zone,dip (deg),width (m),coal_thickness (m),zone_thickness (m),cast,dozer_push,dragline,poststrip,dragline_total\n");
    let zone_1 = "0.2,0.2,0.6,0,0.8";
    let zone_2 = "0,0,0.9,0.1,1.2";
    for (method, zones) in [("1 zone", vec![(1, zone_1)]), ("2 zone", vec![(1, zone_1), (2, zone_2)])] {
        for (zone, shares) in zones {
            for dip in [0, 30] {
                for width in [0, 200] {
                    for coal in [0, 20] {
                        for thickness in [0, 60] {
                            factors.push_str(&format!("{method},{zone},{dip},{width},{coal},{thickness},{shares}\n"));
                        }
                    }
                }
            }
        }
    }
    vec![
        (
            SETTINGS,
            "topsoil_thickness (m),freedig_thickness (m),min_drill_thickness (m),min_burden (m),system_max (m),max_zones,partings_max_thickness (m),wedge_thickness (m),check_tolerance (m3)\n0.5,4.5,3,10,40,2,3,2,0.01\n".to_string(),
        ),
        (METHODS, "when zones,method,min_thickness (m)\n1,1 zone,10\n2,2 zone,30\n".to_string()),
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
pub const DRAGLINE_SYSTEM: &str = r#"# The dragline system: on each waste interval over coal at least the minimum burden thick, the system's thickness (the
# interval's, to the system's maximum) from the bottom up across its units. Its method is the column's own: from the
# most zones down, the first whose minimum thickness System methods says it reaches. Each zone is split by its method's
# factors, read off Dragline factors by method and zone, nearest the interval's measured dip and width, between the
# coal thicknesses below and the zone thicknesses either side, into cast, dozer push, dragline and poststrip; the
# dragline's rehandle from its total factor.
def allocate(column, p):
    coal = column.coal()
    sections = column.intervals(ends="coal")
    for interval in column.intervals():
        below = column.after(interval[-1])
        units = [u for u in interval if u.process == None]
        if below == None or below not in coal or not units:
            continue
        thickness = column.total("thickness", units=units)
        if thickness < p.min_burden:
            continue
        system = min(thickness, p.system_max)
        method = None
        zones = 0
        for n in range(int(p.max_zones), 0, -1):
            need = lookup("System methods", "min_thickness", zones=n, default=-1)
            if need >= 0 and system >= need:
                zones = n
                method = lookup("System methods", "method", zones=n)
                break
        if method == None:
            continue
        dip = column.total("dip", units=units)
        width = column.total("width", units=units)
        coal_thickness = column.total("thickness", units=[s for s in sections if s[0] == below][0])
        zone_thickness = system / zones
        for n in range(1, zones + 1):
            if not units:
                break
            zone = column.take_from_bottom(units, min(zone_thickness, column.total("thickness", units=units)))
            units = [u for u in units if u not in zone]
            f = lambda name: lookup("Dragline factors", name, method=method, zone=n, dip=nearest(dip), width=nearest(width), coal_thickness=between(coal_thickness), zone_thickness=between(zone_thickness))
            for u in zone:
                u.zone = "Zone %d" % n
                u.method = method
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

/// The add-on's name in the project: made once, kept where a site has one, every dataflow and table that lists it running
/// the same steps (Table setup plan, decision 2).
pub const ADD_ON: &str = "Waste allocation";

/// The template's waste allocation as an add-on (Table setup plan, decision 2): its schedule and word fields, its six
/// allocation steps and the check step, and the settings table, read as `p`. It runs with any main dataflow that has
/// `rom` and `final`, its allocation steps before `final` and its check after.
pub fn waste_allocation_add_on(settings: ObjectId) -> Dataflow {
    Dataflow {
        fields: schedule_fields().iter().map(|name| DataflowField { field: Field::new(name, FieldKind::Other, AddUp::Sum), source: FieldSource::Worked }).collect(),
        words: WORDS.iter().map(|name| WordField::new(name, FieldSource::Worked)).collect(),
        steps: STEPS.iter().map(|(name, script)| DataflowStep::allocation(name, script)).chain(std::iter::once(DataflowStep::table_script(CHECK_STEP, CHECKS))).collect(),
        parameters: vec![settings],
        add_on: true,
        ..Dataflow::default()
    }
}

impl crate::Session {
    /// Adds waste allocation to a main dataflow (the Waste allocation plan, decision 7, made an add-on by the Table setup
    /// plan, decision 2): its four parameter tables and the Waste allocation add-on saved where the project hasn't one of
    /// that name (one it has is kept, as a site edited it), then the dataflow saved listing the add-on, so every table it
    /// makes runs it. A run each, each undone on its own. Gives back the tables' ids, settings first, and the add-on's.
    pub fn add_waste_allocation(&mut self, dataflow: ObjectId) -> Result<(Vec<ObjectId>, ObjectId), CommandError> {
        let object = self.project().object(dataflow).cloned().ok_or(CommandError::MissingObject(dataflow))?;
        let Body::Dataflow(d) = &object.body else {
            return Err(CommandError::WrongKind { id: dataflow, expected: ObjectKind::Dataflow, found: object.kind() });
        };
        let mut d = (**d).clone();
        // Checked before anything is saved, so a refusal leaves the project as it was.
        if d.add_on {
            return Err(CommandError::Invalid(format!("{} is an add-on: add waste allocation to a main dataflow", object.name)));
        }
        if let Some(s) = d.steps.iter().find(|s| scripts().iter().any(|(n, _)| *n == s.step)) {
            return Err(CommandError::Invalid(format!("{} has a step {} already: waste allocation is added once", object.name, s.step)));
        }
        let listed = d.add_ons.iter().filter_map(|id| self.project().object(*id)).any(|o| o.name == ADD_ON);
        if listed {
            return Err(CommandError::Invalid(format!("{} runs the {ADD_ON} add-on already: it's added once", object.name)));
        }
        d.with_add_ons(&[(ADD_ON, &waste_allocation_add_on(ObjectId::new()))]).map_err(CommandError::Invalid)?;
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
        let kept = self.project().objects().find(|o| o.name == ADD_ON && matches!(&o.body, Body::Dataflow(a) if a.add_on)).map(|o| o.id);
        let add_on = match kept {
            Some(id) => id,
            None => made(self.run("dataflow.save", serde_json::json!({ "name": ADD_ON, "content": waste_allocation_add_on(ids[0]) }))?)?,
        };
        d.add_ons.push(add_on);
        // The measures its scripts read, where the dataflow hasn't them: the blocks' own, a table free to take another.
        for (name, kind) in MEASURED {
            if !d.fields.iter().any(|f| f.field.name == name) {
                d.fields.push(DataflowField { field: Field::new(name, kind, AddUp::Max), source: FieldSource::Attribute { attribute: name.to_string() } });
            }
        }
        self.run("dataflow.save", serde_json::json!({ "dataflow": dataflow.to_string(), "name": object.name, "content": d }))?;
        Ok((ids, add_on))
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

    /// The add-on holds together, and put into a main dataflow its fields, words and steps go in where they belong,
    /// before final and after it; a dataflow without rom and final can't take it.
    #[test]
    fn waste_allocation_is_an_add_on_that_goes_in_where_it_belongs() {
        let settings = ObjectId::new();
        let a = waste_allocation_add_on(settings);
        a.check().unwrap();
        assert_eq!((a.fields.len(), a.words.len(), a.steps.len()), (39, 6, 7));
        let mut d = Dataflow {
            fields: vec![
                DataflowField { field: Field::new("volume", FieldKind::Volume, AddUp::Sum), source: FieldSource::Volume },
                DataflowField { field: Field::new("thickness", FieldKind::Thickness, AddUp::Sum), source: FieldSource::Thickness },
            ],
            steps: ["initialise", "rom", "final"].iter().map(|s| DataflowStep::on(s)).collect(),
            ..Dataflow::default()
        };
        let merged = d.with_add_ons(&[(ADD_ON, &a)]).unwrap();
        let names: Vec<&str> = merged.steps.iter().map(|s| s.step.as_str()).collect();
        assert_eq!(names, ["initialise", "rom", "loose_ground", "short_term_overrides", "dragline_system", "excavator_partings_wedge", "blasting", "waste_quantities", "final", "waste_checks"]);
        assert_eq!(merged.parameters, [settings]);
        merged.check().unwrap();
        assert!(merged.with_add_ons(&[(ADD_ON, &a)]).unwrap_err().contains("has the name of a step already run"));
        d.steps.retain(|s| s.step != "rom");
        assert!(d.with_add_ons(&[(ADD_ON, &a)]).unwrap_err().contains("no rom before final"));
        for (name, csv) in tables() {
            zd_core::ParameterTable::from_csv(&csv).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}
