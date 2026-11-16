//! What a formula reads and writes through (decision 0077): `unit`, its fields with any set earlier in the run, and
//! read-only `unit.section`, `unit.column`, `unit.block` and `unit.above`; and `p`, the parameters for the unit. Each
//! owns the values it hands out (shared, not copied, between units), so nothing a script holds can outlive the data.

use std::fmt;
use std::sync::{Arc, Mutex};

use allocative::Allocative;
use starlark::any::ProvidesStaticType;
use starlark::starlark_simple_value;
use starlark::StarlarkPagablePanic;
use starlark::values::{Heap, NoSerialize, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

use crate::{Declared, Kind, ScriptValue, Values};

/// What a formula did, kept only when it's traced: each value read and each field set, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A value read, as the script wrote it (`unit.ash_ad`, `p.rom_ash_cutoff`, `unit.section.volume`).
    Read { what: String, value: ScriptValue },
    /// A field set.
    Set { field: String, value: ScriptValue },
}

pub(crate) type Log = Arc<Mutex<Vec<Event>>>;

fn note(log: &Option<Log>, event: Event) {
    if let Some(log) = log {
        log.lock().expect("trace log").push(event);
    }
}

pub(crate) fn to_starlark<'v>(value: &ScriptValue, heap: Heap<'v>) -> Value<'v> {
    match value {
        ScriptValue::None => Value::new_none(),
        ScriptValue::Number(x) => heap.alloc(*x),
        ScriptValue::Text(s) => heap.alloc_str(s).to_value(),
    }
}

pub(crate) fn find<'a>(values: &'a Values, name: &str) -> Option<&'a ScriptValue> {
    values.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

/// A unit of the column. Reads its sets first, then its fields; a field the dataflow declares but the unit has no
/// value for reads as None; a name the dataflow doesn't declare isn't there.
#[derive(Debug, ProvidesStaticType, NoSerialize, StarlarkPagablePanic, Allocative)]
pub(crate) struct Unit {
    #[allocative(skip)]
    pub fields: Values,
    #[allocative(skip)]
    pub section: Option<Values>,
    #[allocative(skip)]
    pub column: Values,
    #[allocative(skip)]
    pub block: Values,
    #[allocative(skip)]
    pub above: Option<Values>,
    #[allocative(skip)]
    pub declared: Arc<Declared>,
    #[allocative(skip)]
    pub sets: Mutex<Vec<(String, ScriptValue)>>,
    #[allocative(skip)]
    pub log: Option<Log>,
}
starlark_simple_value!(Unit);

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words: Vec<String> = ["seam", "material", "bench"]
            .iter()
            .filter_map(|k| match find(&self.fields, k) {
                Some(ScriptValue::Text(s)) => Some(format!("{k} {s}")),
                Some(ScriptValue::Number(x)) => Some(format!("{k} {x}")),
                _ => None,
            })
            .collect();
        write!(f, "unit({})", words.join(", "))
    }
}

impl Unit {
    fn read(&self, name: &str) -> Option<ScriptValue> {
        if let Some((_, v)) = self.sets.lock().expect("sets").iter().rev().find(|(k, _)| k == name) {
            return Some(v.clone());
        }
        if let Some(v) = find(&self.fields, name) {
            return Some(v.clone());
        }
        (self.declared.is_field(name) || self.declared.is_label(name)).then_some(ScriptValue::None)
    }

    fn part(&self, kind: &'static str, values: Option<&Values>) -> Parts {
        Parts { kind, values: values.cloned(), log: self.log.clone() }
    }
}

#[starlark_value(type = "unit")]
impl<'v> StarlarkValue<'v> for Unit {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        let part = match attribute {
            "section" => Some(self.part("section", self.section.as_ref())),
            "column" => Some(self.part("column", Some(&self.column))),
            "block" => Some(self.part("block", Some(&self.block))),
            "above" => {
                return match &self.above {
                    Some(v) => Some(heap.alloc(Parts { kind: "above", values: Some(v.clone()), log: self.log.clone() })),
                    None => Some(Value::new_none()),
                };
            }
            _ => None,
        };
        if let Some(p) = part {
            return Some(heap.alloc(p));
        }
        let value = self.read(attribute)?;
        note(&self.log, Event::Read { what: format!("unit.{attribute}"), value: value.clone() });
        Some(to_starlark(&value, heap))
    }

    fn has_attr(&self, attribute: &str, _heap: Heap<'v>) -> bool {
        matches!(attribute, "section" | "column" | "block" | "above") || self.read(attribute).is_some()
    }

    fn dir_attr(&self) -> Vec<String> {
        let mut names: Vec<String> = self.declared.fields.iter().map(|f| f.name.clone()).collect();
        names.extend(self.declared.labels.iter().cloned());
        names.extend(["section", "column", "block", "above"].map(String::from));
        names
    }

    /// `unit["ash"]`: a field or label by a name worked out as the formula runs (`name[:-len("_default")]`), as
    /// `unit.ash` reads it; a name the dataflow doesn't have is refused.
    fn at(&self, index: Value<'v>, heap: Heap<'v>) -> starlark::Result<Value<'v>> {
        let Some(name) = index.unpack_str() else {
            return Err(starlark::Error::new_other(anyhow::anyhow!("a unit's field is named in words, not {}", index.to_repr())));
        };
        match self.get_attr(name, heap) {
            Some(v) if !matches!(name, "section" | "column" | "block" | "above") => Ok(v),
            _ => Err(starlark::Error::new_other(anyhow::anyhow!("`{name}` isn't a field of this dataflow{}", crate::check::did_you_mean(name, self.declared.fields.iter().map(|f| f.name.as_str()))))),
        }
    }

    /// `unit["ash"] = 12`: sets a field named as the formula runs, as `unit.ash = 12` does.
    fn set_at(&self, index: Value<'v>, new_value: Value<'v>) -> starlark::Result<()> {
        let Some(name) = index.unpack_str() else {
            return Err(starlark::Error::new_other(anyhow::anyhow!("a unit's field is named in words, not {}", index.to_repr())));
        };
        self.set_attr(name, new_value)
    }

    fn set_attr(&self, attribute: &str, new_value: Value<'v>) -> starlark::Result<()> {
        let Some(field) = self.declared.fields.iter().find(|f| f.name == attribute) else {
            let why = if self.declared.is_label(attribute) {
                format!("`{attribute}` is a label of the unit, not a field a formula sets")
            } else {
                format!("`{attribute}` isn't a field of this dataflow{}", crate::check::did_you_mean(attribute, self.declared.fields.iter().map(|f| f.name.as_str())))
            };
            return Err(starlark::Error::new_other(anyhow::anyhow!(why)));
        };
        let value = from_starlark(new_value, field.kind).map_err(|why| starlark::Error::new_other(anyhow::anyhow!("`{attribute}` {why}")))?;
        note(&self.log, Event::Set { field: attribute.to_string(), value: value.clone() });
        self.sets.lock().expect("sets").push((attribute.to_string(), value));
        Ok(())
    }
}

/// A value from Starlark as a field of a kind takes it: a number (whole or not) for a number field, words for a text
/// field, None for either.
pub(crate) fn from_starlark(v: Value<'_>, kind: Kind) -> Result<ScriptValue, String> {
    if v.is_none() {
        return Ok(ScriptValue::None);
    }
    match kind {
        Kind::Number => {
            if let Some(i) = v.unpack_i32() {
                return Ok(ScriptValue::Number(f64::from(i)));
            }
            match v.downcast_ref::<starlark::values::float::StarlarkFloat>() {
                Some(x) if x.0.is_finite() => Ok(ScriptValue::Number(x.0)),
                Some(x) => Err(format!("holds numbers, and {} isn't one (a division by zero?)", x.0)),
                None => Err(format!("holds numbers, not {} ({})", v.get_type(), v.to_repr())),
            }
        }
        Kind::Text => match v.unpack_str() {
            Some(s) => Ok(ScriptValue::Text(s.to_string())),
            None => Err(format!("holds words, not {} ({})", v.get_type(), v.to_repr())),
        },
    }
}

/// Read-only values: a unit's working section or column added up, its block's attributes, the unit above.
#[derive(Debug, ProvidesStaticType, NoSerialize, StarlarkPagablePanic, Allocative)]
pub(crate) struct Parts {
    kind: &'static str,
    #[allocative(skip)]
    values: Option<Values>,
    #[allocative(skip)]
    log: Option<Log>,
}
starlark_simple_value!(Parts);

impl fmt::Display for Parts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind)
    }
}

#[starlark_value(type = "values")]
impl<'v> StarlarkValue<'v> for Parts {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        // A unit with no working section, or a block without the attribute, reads None, never fails.
        let value = self.values.as_ref().and_then(|v| find(v, attribute)).cloned().unwrap_or(ScriptValue::None);
        note(&self.log, Event::Read { what: format!("unit.{}.{attribute}", self.kind), value: value.clone() });
        Some(to_starlark(&value, heap))
    }

    fn has_attr(&self, attribute: &str, _heap: Heap<'v>) -> bool {
        self.values.as_ref().is_some_and(|v| find(v, attribute).is_some())
    }

    fn dir_attr(&self) -> Vec<String> {
        self.values.as_ref().map(|v| v.iter().map(|(k, _)| k.clone()).collect()).unwrap_or_default()
    }
}

/// The parameters for a unit: each value from the first row of the dataflow's tables that fits the unit and holds it.
/// One no row gives isn't there, so reading it stops the block (0070); `getattr(p, "name", default)` reads one that
/// may be missing.
#[derive(Debug, ProvidesStaticType, NoSerialize, StarlarkPagablePanic, Allocative)]
pub(crate) struct Params {
    #[allocative(skip)]
    pub values: Values,
    #[allocative(skip)]
    pub log: Option<Log>,
}
starlark_simple_value!(Params);

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parameters")
    }
}

#[starlark_value(type = "parameters")]
impl<'v> StarlarkValue<'v> for Params {
    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        let value = find(&self.values, attribute)?.clone();
        note(&self.log, Event::Read { what: format!("p.{attribute}"), value: value.clone() });
        Some(to_starlark(&value, heap))
    }

    fn has_attr(&self, attribute: &str, _heap: Heap<'v>) -> bool {
        find(&self.values, attribute).is_some()
    }

    fn dir_attr(&self) -> Vec<String> {
        self.values.iter().map(|(k, _)| k.clone()).collect()
    }
}
