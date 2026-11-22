//! Lookups (the Build Plan's "Schedule quantities plan", decision 1): a parameter value by any keys a script gives, not
//! only the block's attributes. `lookup("Rates", "bcm_per_hour", equipment="DL1", bench_height=unit.thickness)` reads a
//! parameter table by its name; `p.at(seam="U", thickness=x)` reads the script's own tables, in their order. Either gives
//! the first row that fits the keys, matched as every parameter row is (lists, ranges `10-15`, `<0.3`), and says which
//! table, row and value it read when the script is traced.

use std::sync::Arc;

use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::dict::UnpackDictEntries;
use starlark::values::none::NoneOr;
use starlark::values::{Value, ValueLike};
use starlark_derive::starlark_module;
use zd_core::{AttrValue, Attributes, ParamValue, ParameterTable};

use crate::handles::{Event, to_starlark};
use crate::ScriptValue;

/// The parameter tables a script may look up: its own, read in order by `p.at`, and those it names, by name.
#[derive(Debug, Clone, Default)]
pub struct Lookups {
    pub own: Vec<Arc<ParameterTable>>,
    pub named: Vec<(String, Arc<ParameterTable>)>,
}

impl Lookups {
    /// A named table, its name as written, else case aside.
    fn named(&self, name: &str) -> Option<&(String, Arc<ParameterTable>)> {
        self.named.iter().find(|(n, _)| n == name).or_else(|| self.named.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)))
    }
}

/// Keys as a parameter row is matched against them: words as words, numbers as numbers; None leaves a key out.
pub(crate) fn keys_of<'v>(pairs: &[(String, Value<'v>)]) -> anyhow::Result<Attributes> {
    let mut out = Attributes::new();
    for (k, v) in pairs {
        if v.is_none() {
            continue;
        }
        let value = if let Some(s) = v.unpack_str() {
            AttrValue::Text(s.to_string())
        } else if let Some(i) = v.unpack_i32() {
            AttrValue::Number(f64::from(i))
        } else if let Some(x) = v.downcast_ref::<starlark::values::float::StarlarkFloat>() {
            AttrValue::Number(x.0)
        } else if let Some(b) = v.unpack_bool() {
            AttrValue::Text(if b { "yes" } else { "no" }.into())
        } else {
            anyhow::bail!("a key is words or a number, and {k} is {}", v.to_repr());
        };
        out.insert(k.clone(), value);
    }
    Ok(out)
}

/// Keys as words, for what's said and traced: `equipment DL1, bench_height 12`.
pub(crate) fn keys_words(keys: &Attributes) -> String {
    if keys.is_empty() {
        return "no keys".into();
    }
    keys.iter()
        .map(|(k, v)| match v {
            AttrValue::Text(t) => format!("{k} {t}"),
            AttrValue::Number(x) => format!("{k} {x}"),
            other => format!("{k} {other:?}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn script_value(v: &ParamValue) -> ScriptValue {
    match v {
        ParamValue::Number(x) => ScriptValue::Number(*x),
        ParamValue::Text(t) => ScriptValue::Text(t.clone()),
    }
}

/// The run a script is part of, as `lookup` and `p.at` find it.
pub(crate) fn lookups_of<'a>(eval: &'a Evaluator<'_, '_, '_>) -> Option<&'a crate::ScriptRun> {
    eval.extra.and_then(|e| e.downcast_ref::<crate::ScriptRun>())
}

/// The value of `column` from the first row of `table` (named `name`) that fits `keys`, or why there's none.
pub(crate) fn look_up(name: &str, table: &ParameterTable, column: &str, keys: &Attributes) -> Result<(usize, ScriptValue), String> {
    if table.column(column).is_none() {
        let near = crate::check::did_you_mean(column, table.columns.iter().map(|c| c.name.as_str()));
        let hint = if near.is_empty() { format!(" (it has {})", table.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")) } else { near };
        return Err(format!("{name} has no column {column}{hint}"));
    }
    match table.lookup(keys, column) {
        Some((row, value)) => Ok((row, script_value(value))),
        None => Err(format!("no row of {name} gives {column} for {}", keys_words(keys))),
    }
}

#[starlark_module]
pub(crate) fn lookup_globals(builder: &mut GlobalsBuilder) {
    /// A parameter table's value by its name and the keys given: `lookup("Rates", "bcm_per_hour", equipment="DL1",
    /// bench_height=unit.thickness)`. The first row that fits every key it filters on; `default=` where none does.
    fn lookup<'v>(
        table: &str,
        column: &str,
        #[starlark(require = named, default = NoneOr::None)] default: NoneOr<Value<'v>>,
        #[starlark(kwargs)] keys: UnpackDictEntries<String, Value<'v>>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let run = lookups_of(eval).ok_or_else(|| anyhow::anyhow!("lookup reads the parameter tables of a dataflow, a calculated column or a table script"))?;
        let Some((name, t)) = run.lookups.named(table) else {
            let named: Vec<&str> = run.lookups.named.iter().map(|(n, _)| n.as_str()).collect();
            anyhow::bail!(
                "no parameter table named {table} is looked up here{}: name it in quotes, lookup(\"{table}\", …), and save, so it's found by name",
                if named.is_empty() { String::new() } else { format!(" (it looks up {})", named.join(", ")) }
            );
        };
        let keys = keys_of(&keys.entries)?;
        let log = run.log.clone();
        match look_up(name, t, column, &keys) {
            Ok((row, value)) => {
                if let Some(log) = &log {
                    log.lock().expect("trace log").push(Event::Read { what: format!("lookup {name} row {}: {column} ({})", row + 1, keys_words(&keys)), value: value.clone() });
                }
                Ok(to_starlark(&value, eval.heap()))
            }
            Err(why) => match default {
                NoneOr::Other(d) if t.column(column).is_some() => {
                    if let Some(log) = &log {
                        log.lock().expect("trace log").push(Event::Read { what: format!("lookup {name}: {column} ({}), its default", keys_words(&keys)), value: ScriptValue::Text(d.to_repr()) });
                    }
                    Ok(d)
                }
                _ => Err(anyhow::anyhow!(why)),
            },
        }
    }
}

/// A curve read in a script (Schedule quantities plan, decision 2): `interpolate("Yield curve", unit.ash, x="ash",
/// y="yield", seam=unit.seam)`, the rows that fit the keys in order of `x`, linear between; `ends="hold"` holds the end
/// value outside them, else outside is refused.
#[starlark_module]
pub(crate) fn curve_globals(builder: &mut GlobalsBuilder) {
    fn interpolate<'v>(
        table: &str,
        at: Value<'v>,
        #[starlark(require = named)] x: &str,
        #[starlark(require = named)] y: &str,
        #[starlark(require = named, default = "refuse")] ends: &str,
        #[starlark(kwargs)] keys: UnpackDictEntries<String, Value<'v>>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<f64> {
        let run = lookups_of(eval).ok_or_else(|| anyhow::anyhow!("interpolate reads the parameter tables of a dataflow, a calculated column or a table script"))?;
        let Some((name, t)) = run.lookups.named(table) else {
            anyhow::bail!("no parameter table named {table} is read here: name it in quotes, interpolate(\"{table}\", …), and save, so it's found by name");
        };
        let ends = match ends {
            "refuse" => zd_core::CurveEnds::Refuse,
            "hold" => zd_core::CurveEnds::Hold,
            other => anyhow::bail!("ends is \"refuse\" (the default) or \"hold\", not {other}"),
        };
        let value = if let Some(i) = at.unpack_i32() {
            f64::from(i)
        } else if let Some(v) = at.downcast_ref::<starlark::values::float::StarlarkFloat>() {
            v.0
        } else if at.is_none() {
            anyhow::bail!("no {x} to read {name} at (None)");
        } else {
            anyhow::bail!("{name} is read at a number of {x}, not {}", at.to_repr());
        };
        let keys = keys_of(&keys.entries)?;
        let on = t.interpolate(&keys, x, y, value, ends).map_err(|why| anyhow::anyhow!("{name}: {why}{}", if keys.is_empty() { String::new() } else { format!(" (for {})", keys_words(&keys)) }))?;
        if let Some(log) = &run.log {
            let rows = if on.rows.0 == on.rows.1 { format!("row {}", on.rows.0 + 1) } else { format!("rows {} to {}", on.rows.0 + 1, on.rows.1 + 1) };
            log.lock().expect("trace log").push(Event::Read { what: format!("interpolate {name} {rows}: {y} at {x} {value}{}", if keys.is_empty() { String::new() } else { format!(" ({})", keys_words(&keys)) }), value: ScriptValue::Number(on.y) });
        }
        Ok(on.y)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zd_core::ParameterTable;

    use crate::{ColumnInput, Declared, FieldDecl, Formula, ScriptValue, UnitInput, values};

    use super::Lookups;

    /// A dragline's rate by bench height: 0 to 10 m at 1,800 BCM/h, 10 to 15 m at 2,400; a dozer's 900 whatever the height.
    fn rates() -> Arc<ParameterTable> {
        Arc::new(ParameterTable::from_csv("when equipment,when bench_height,bcm_per_hour (bcm/h)\nDL1,0-10,1800\nDL1,10-15,2400\nD11,,900\n").unwrap())
    }

    fn names(lookups: Lookups) -> Declared {
        Declared {
            fields: vec![FieldDecl::number("volume", "m3"), FieldDecl::number("thickness", "m"), FieldDecl::number("hours", "h"), FieldDecl::number("rate", "bcm/h")],
            lookups: Arc::new(lookups),
            ..Default::default()
        }
    }

    fn unit(volume: f64, thickness: f64) -> ColumnInput {
        ColumnInput {
            units: vec![UnitInput { fields: values([("volume", ScriptValue::Number(volume)), ("thickness", ScriptValue::Number(thickness))]), block: values([]), parameters: values([]), section: None, rom_coal: None }],
            ..Default::default()
        }
    }

    fn number(v: Option<&ScriptValue>) -> f64 {
        match v {
            Some(ScriptValue::Number(x)) => *x,
            other => panic!("not a number: {other:?}"),
        }
    }

    /// A 12 m unit of 48,000 BCM takes the 10-15 m row: 2,400 BCM/h, 20 hours. A 6 m unit takes 1,800. The trace says
    /// which table and row the rate came from.
    #[test]
    fn a_rate_is_looked_up_by_a_worked_value() {
        let script = "def formula(unit):\n    unit.rate = lookup(\"Rates\", \"bcm_per_hour\", equipment=\"DL1\", bench_height=unit.thickness)\n    unit.hours = unit.volume / unit.rate\n";
        let f = Formula::compile(script, names(Lookups { own: Vec::new(), named: vec![("Rates".into(), rates())] })).unwrap();
        let ran = f.run(&unit(48_000.0, 12.0), &[0]).unwrap();
        assert_eq!(number(ran[0].get("rate")), 2_400.0);
        assert_eq!(number(ran[0].get("hours")), 20.0);
        assert_eq!(number(f.run(&unit(48_000.0, 6.0), &[0]).unwrap()[0].get("rate")), 1_800.0);
        let traced = f.trace(&unit(48_000.0, 12.0), 0);
        assert!(traced.events.iter().any(|e| matches!(e, crate::Event::Read { what, value: ScriptValue::Number(x) } if what == "lookup Rates row 2: bcm_per_hour (bench_height 12, equipment DL1)" && *x == 2_400.0)), "{:?}", traced.events);
    }

    /// No row fits a 20 m bench: said with the table and the keys, unless a default is given. A column the table hasn't, and
    /// a table not looked up here, are said too.
    #[test]
    fn a_lookup_no_row_gives_is_said_or_takes_its_default() {
        let lookups = || Lookups { own: Vec::new(), named: vec![("Rates".into(), rates())] };
        let stops = |script: &str| Formula::compile(script, names(lookups())).unwrap().run(&unit(1.0, 20.0), &[0]).unwrap_err().message;
        let why = stops("def formula(unit):\n    unit.rate = lookup(\"Rates\", \"bcm_per_hour\", equipment=\"DL1\", bench_height=unit.thickness)\n");
        assert_eq!(why, "no row of Rates gives bcm_per_hour for bench_height 20, equipment DL1");
        let f = Formula::compile("def formula(unit):\n    unit.rate = lookup(\"Rates\", \"bcm_per_hour\", equipment=\"DL1\", bench_height=unit.thickness, default=0)\n", names(lookups())).unwrap();
        assert_eq!(number(f.run(&unit(1.0, 20.0), &[0]).unwrap()[0].get("rate")), 0.0);
        assert!(stops("def formula(unit):\n    unit.rate = lookup(\"Rates\", \"bcm_per_hr\", equipment=\"DL1\")\n").starts_with("Rates has no column bcm_per_hr (did you mean `bcm_per_hour`?)"));
        assert!(stops("def formula(unit):\n    unit.rate = lookup(\"Haulage\", \"km\")\n").starts_with("no parameter table named Haulage is looked up here (it looks up Rates)"));
        // A key the rows don't filter on is no hindrance: the dozer's rate whatever else is given.
        let f = Formula::compile("def formula(unit):\n    unit.rate = lookup(\"Rates\", \"bcm_per_hour\", equipment=\"D11\", bench_height=99, seam=\"U\")\n", names(lookups())).unwrap();
        assert_eq!(number(f.run(&unit(1.0, 1.0), &[0]).unwrap()[0].get("rate")), 900.0);
    }

    /// `p.at` reads the script's own tables, in order, for the keys given: the first table's row before the second's.
    #[test]
    fn p_at_reads_the_own_tables_for_the_keys_given() {
        let site = Arc::new(ParameterTable::from_csv("when equipment,bcm_per_hour (bcm/h)\nDL1,2000\n").unwrap());
        let f = Formula::compile(
            "def formula(unit, p):\n    unit.rate = p.at(equipment=\"DL1\", bench_height=unit.thickness).bcm_per_hour\n",
            names(Lookups { own: vec![site, rates()], named: Vec::new() }),
        )
        .unwrap();
        assert_eq!(number(f.run(&unit(1.0, 12.0), &[0]).unwrap()[0].get("rate")), 2_000.0, "the first table read first");
    }
}
