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
    /// A value placed onto another leaf of the block (Waste allocation plan, decision 5): the schedule field, the value
    /// and the leaf's levels as named.
    Placed { field: String, value: f64, onto: Vec<(String, String)> },
}

/// A value a formula placed onto another leaf of its block (`unit.place_onto(...)`, Waste allocation plan, decision 5):
/// a schedule field, the value, and the levels below the column it named (`seam`, `bench`), each in words.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub field: String,
    pub value: f64,
    pub onto: Vec<(String, String)>,
}

pub(crate) type Log = Arc<Mutex<Vec<Event>>>;

pub(crate) fn note(log: &Option<Log>, event: Event) {
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
    pub rom_coal: Option<Values>,
    #[allocative(skip)]
    pub block: Values,
    #[allocative(skip)]
    pub above: Option<Values>,
    #[allocative(skip)]
    pub declared: Arc<Declared>,
    #[allocative(skip)]
    pub sets: Mutex<Vec<(String, ScriptValue)>>,
    #[allocative(skip)]
    pub placed: Mutex<Vec<Placed>>,
    #[allocative(skip)]
    pub log: Option<Log>,
}
starlark_simple_value!(Unit);

/// `unit.place_onto("Waste\\Drill and Blast\\Volume", x, seam="B", bench="2")` (Waste allocation plan, decision 5): the
/// value written on the leaf of the unit's block those levels name, not its own, as Spry's drill and blast and coal
/// exposed are; a level not named is the unit's own. Only a schedule field (named with a folder) is placed; the engine
/// checks the levels and that the leaf is there.
#[starlark::starlark_module]
fn unit_methods(builder: &mut starlark::environment::MethodsBuilder) {
    fn place_onto<'v>(this: &Unit, #[starlark(require = pos)] field: &str, #[starlark(require = pos)] value: Value<'v>, #[starlark(kwargs)] leaf: starlark::values::dict::UnpackDictEntries<String, Value<'v>>) -> anyhow::Result<starlark::values::none::NoneType> {
        let Some(placed) = placed(&this.declared, field, value, &leaf.entries)? else { return Ok(starlark::values::none::NoneType) };
        note(&this.log, Event::Placed { field: placed.field.clone(), value: placed.value, onto: placed.onto.clone() });
        this.placed.lock().expect("placed").push(placed);
        Ok(starlark::values::none::NoneType)
    }
}

/// A value to place onto another leaf, checked: a schedule field of numbers, the leaf's levels in words. None placed for
/// a value of None.
pub(crate) fn placed(declared: &Declared, field: &str, value: Value<'_>, leaf: &[(String, Value<'_>)]) -> anyhow::Result<Option<Placed>> {
    let Some(decl) = declared.fields.iter().find(|f| f.name == field) else {
        anyhow::bail!("`{field}` isn't a field of this dataflow{}", crate::check::did_you_mean(field, declared.fields.iter().map(|f| f.name.as_str())));
    };
    if !field.contains('\\') {
        anyhow::bail!("`{field}` isn't a schedule field: only a field named with a folder (`Waste\\\\Drill and Blast\\\\Volume`) is placed onto another leaf");
    }
    let value = match from_starlark(value, decl.kind).map_err(|why| anyhow::anyhow!("`{field}` {why}"))? {
        ScriptValue::Number(x) => x,
        ScriptValue::None => return Ok(None),
        ScriptValue::Text(_) => anyhow::bail!("`{field}` holds words: words aren't placed onto another leaf, only numbers that add up"),
    };
    let mut onto = Vec::with_capacity(leaf.len());
    for (level, word) in leaf {
        let word = if let Some(s) = word.unpack_str() {
            s.to_string()
        } else if let Some(i) = word.unpack_i32() {
            i.to_string()
        } else if let Some(x) = word.downcast_ref::<starlark::values::float::StarlarkFloat>() {
            format!("{}", x.0)
        } else {
            anyhow::bail!("`{level}=` names a leaf in words or a number, not {}", word.to_repr());
        };
        onto.push((level.clone(), word));
    }
    Ok(Some(Placed { field: field.to_string(), value, onto }))
}

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
    fn get_methods() -> Option<&'static starlark::environment::Methods> {
        starlark::methods_static!(UNIT_METHODS = unit_methods);
        Some(UNIT_METHODS.methods())
    }

    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        let part = match attribute {
            "section" => Some(self.part("section", self.section.as_ref())),
            "column" => Some(self.part("column", Some(&self.column))),
            "rom_coal" => {
                return match &self.rom_coal {
                    Some(v) => Some(heap.alloc(Parts { kind: "rom_coal", values: Some(v.clone()), log: self.log.clone() })),
                    None => Some(Value::new_none()),
                };
            }
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
        matches!(attribute, "section" | "column" | "block" | "above" | "rom_coal") || self.read(attribute).is_some()
    }

    fn dir_attr(&self) -> Vec<String> {
        let mut names: Vec<String> = self.declared.fields.iter().map(|f| f.name.clone()).collect();
        names.extend(self.declared.labels.iter().cloned());
        names.extend(["section", "column", "block", "above", "rom_coal"].map(String::from));
        names
    }

    /// `unit["ash"]`: a field or label by a name worked out as the formula runs (`name[:-len("_default")]`), as
    /// `unit.ash` reads it; a name the dataflow doesn't have is refused.
    fn at(&self, index: Value<'v>, heap: Heap<'v>) -> starlark::Result<Value<'v>> {
        let Some(name) = index.unpack_str() else {
            return Err(starlark::Error::new_other(anyhow::anyhow!("a unit's field is named in words, not {}", index.to_repr())));
        };
        match self.get_attr(name, heap) {
            Some(v) if !matches!(name, "section" | "column" | "block" | "above" | "rom_coal") => Ok(v),
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
    pub kind: &'static str,
    #[allocative(skip)]
    pub values: Option<Values>,
    #[allocative(skip)]
    pub log: Option<Log>,
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

    /// `unit.rom_coal["mad"]`: a value by a name worked out as the formula runs, as `unit["mad"]` reads the unit's. An AI
    /// writing from the reference reached for it (decision 0077, 7e), so it works the same on every part.
    fn at(&self, index: Value<'v>, heap: Heap<'v>) -> starlark::Result<Value<'v>> {
        match index.unpack_str() {
            Some(name) => Ok(self.get_attr(name, heap).unwrap_or_else(Value::new_none)),
            None => Err(starlark::Error::new_other(anyhow::anyhow!("a value of the {} is named in words, not {}", self.kind, index.to_repr()))),
        }
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
    /// The tables `p.at(...)` reads (Schedule quantities plan, decision 1).
    #[allocative(skip)]
    pub lookups: Option<Arc<crate::Lookups>>,
    /// Where an allocation step notes each parameter it read, and whose they were (a unit's, or the column's): what the
    /// notebook traces to its table and row (Waste allocation plan, decision 8).
    #[allocative(skip)]
    pub reads: Option<(Reads, Option<u32>)>,
}
starlark_simple_value!(Params);

/// The parameters an allocation step read: whose (a unit's id, or none for the column's own) and the name.
pub(crate) type Reads = Arc<Mutex<Vec<(Option<u32>, String)>>>;

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parameters")
    }
}

#[starlark_value(type = "parameters")]
impl<'v> StarlarkValue<'v> for Params {
    fn get_methods() -> Option<&'static starlark::environment::Methods> {
        starlark::methods_static!(PARAMS_METHODS = params_methods);
        Some(PARAMS_METHODS.methods())
    }

    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        let value = find(&self.values, attribute)?.clone();
        note(&self.log, Event::Read { what: format!("p.{attribute}"), value: value.clone() });
        if let Some((reads, whose)) = &self.reads {
            let mut reads = reads.lock().expect("reads");
            let read = (*whose, attribute.to_string());
            if !reads.contains(&read) {
                reads.push(read);
            }
        }
        Some(to_starlark(&value, heap))
    }

    fn has_attr(&self, attribute: &str, _heap: Heap<'v>) -> bool {
        find(&self.values, attribute).is_some()
    }

    fn dir_attr(&self) -> Vec<String> {
        self.values.iter().map(|(k, _)| k.clone()).collect()
    }
}

/// `p.at(seam="U", thickness=x)` (Schedule quantities plan, decision 1): the parameters from the script's own tables, read
/// in order, for the keys given and only those, as `lookup` reads one table by name.
#[starlark::starlark_module]
fn params_methods(builder: &mut starlark::environment::MethodsBuilder) {
    fn at<'v>(this: &Params, #[starlark(kwargs)] keys: starlark::values::dict::UnpackDictEntries<String, Value<'v>>) -> anyhow::Result<Params> {
        let keys = crate::lookups::keys_of(&keys.entries)?;
        let lookups = this.lookups.clone().unwrap_or_default();
        let refs: Vec<&zd_core::ParameterTable> = lookups.own.iter().map(|t| t.as_ref()).collect();
        let mut values: Vec<(String, ScriptValue)> = Vec::new();
        for t in &lookups.own {
            for c in &t.columns {
                if values.iter().any(|(n, _)| n.eq_ignore_ascii_case(&c.name)) {
                    continue;
                }
                if let Some(found) = zd_core::ParameterTable::lookup_in(&refs, &keys, &c.name) {
                    values.push((c.name.clone(), crate::lookups::script_value(found.value)));
                }
            }
        }
        if let Some(log) = &this.log {
            log.lock().expect("trace log").push(Event::Read { what: format!("p.at({})", crate::lookups::keys_words(&keys)), value: ScriptValue::Text(format!("{} values", values.len())) });
        }
        Ok(Params { values: Arc::new(values), log: this.log.clone(), lookups: this.lookups.clone(), reads: None })
    }
}
