//! What an allocation step reads and changes through (the Waste allocation plan, decisions 3 and 4): `column`, the block
//! column's units top down, and each `unit`, its fields, words and labels, its block and parameters. The column is the
//! engine's own: a split is the column's split, done in Rust, so the pieces add back to the unit exactly and the balance
//! holds; a field set is set on the unit. Nothing a script holds outlives the run.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use allocative::Allocative;
use starlark::any::ProvidesStaticType;
use starlark::starlark_simple_value;
use starlark::values::list::{AllocList, UnpackList};
use starlark::values::none::NoneType;
use starlark::values::tuple::UnpackTuple;
use starlark::values::{Heap, NoSerialize, StarlarkValue, Value, ValueLike};
use starlark::StarlarkPagablePanic;
use starlark_derive::starlark_value;
use zd_reserves::{Amount, Column, End, UnitId};

use crate::handles::{Event, Log, Params, Parts, Placed, Reads, from_starlark, note, to_starlark};
use crate::{Declared, Kind, ScriptValue, Values};

/// Where a unit ends once `final` writes it: in a ROM coal section, or in ROM waste. A unit neither (a small volume, the
/// underburden left, waste below the last coal) has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ends {
    Coal,
    Waste,
}

/// What an allocation step is run on: the column, its blocks' attributes and parameters by input unit (each unit's
/// `origin`), the column's own attributes and parameters (its levels above the seam), and where each unit ends.
pub struct AllocationInput {
    pub column: Column,
    /// The column's words (`Pit/3/B1`).
    pub name: String,
    pub blocks: Vec<Values>,
    pub parameters: Vec<Values>,
    pub column_block: Values,
    pub column_parameters: Values,
    pub ends: HashMap<UnitId, Ends>,
}

/// A split an allocation step made: the unit, how (`thickness 0.5, 4.5`, `shares 0.2, 0.3, 0.5`) and its pieces top
/// down, the unit's own id among them, none where a piece asked for nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitMade {
    pub unit: UnitId,
    pub how: String,
    pub pieces: Vec<Option<UnitId>>,
}

/// What an allocation step gave back: the column as it left it, the splits it made, what it placed onto other leaves (by
/// the unit that placed it), what it printed and the parameters it read (whose, a unit's or the column's, and the name).
#[derive(Debug, Clone)]
pub struct Allocated {
    pub column: Column,
    pub splits: Vec<SplitMade>,
    pub placed: Vec<(UnitId, Placed)>,
    pub said: Vec<String>,
    pub read: Vec<(Option<UnitId>, String)>,
}

/// The column in work, shared by every handle of one run.
pub(crate) struct State {
    pub column: Column,
    pub ends: HashMap<UnitId, Ends>,
    pub blocks: Vec<Values>,
    pub parameters: Vec<Values>,
    pub splits: Vec<SplitMade>,
    pub placed: Vec<(UnitId, Placed)>,
}

pub(crate) type Shared = Arc<Mutex<State>>;

/// What every handle of one run carries.
#[derive(Clone)]
pub(crate) struct Run {
    pub state: Shared,
    pub declared: Arc<Declared>,
    pub log: Option<Log>,
    pub reads: Reads,
    pub name: String,
    pub column_block: Values,
    pub column_parameters: Values,
}

impl State {
    fn at(&self, id: UnitId) -> anyhow::Result<usize> {
        self.column.position(id).ok_or_else(|| anyhow::anyhow!("a unit went missing from the column"))
    }

    /// A unit in words: `OB waste, bench 2`.
    fn words(&self, id: UnitId) -> String {
        let Some(at) = self.column.position(id) else { return "a unit".into() };
        let u = &self.column.units()[at];
        format!("{} {}{}", u.in_situ.seam, u.in_situ.material, u.bench.as_ref().map_or(String::new(), |b| format!(", bench {}", b.label)))
    }

    /// A field, word or label of a unit as a script reads it; none for a name the dataflow hasn't.
    fn read(&self, id: UnitId, name: &str, declared: &Declared) -> Option<ScriptValue> {
        let u = &self.column.units()[self.column.position(id)?];
        if let Ok(i) = self.column.fields().at(name) {
            let x = u.value(i);
            return Some(if x.is_nan() { ScriptValue::None } else { ScriptValue::Number(x) });
        }
        if let Some(f) = declared.fields.iter().find(|f| f.name == name) {
            return Some(match (f.kind, u.word(name)) {
                (Kind::Text, Some(w)) => ScriptValue::Text(w.to_string()),
                _ => ScriptValue::None,
            });
        }
        let text = |s: Option<String>| s.map_or(ScriptValue::None, ScriptValue::Text);
        match name {
            "seam" => Some(ScriptValue::Text(u.in_situ.seam.clone())),
            "material" => Some(ScriptValue::Text(u.in_situ.material.clone())),
            "bench" => Some(text(u.bench.as_ref().map(|b| b.label.clone()))),
            "became" => Some(text(u.became.as_ref().map(|b| b.kind.word().to_string()))),
            _ => declared.is_label(name).then_some(ScriptValue::None),
        }
    }
}

impl Run {
    fn unit(&self, id: UnitId) -> AllocUnit {
        AllocUnit { id, run: self.clone() }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("allocation column")
    }

    /// Units as a Starlark list, a piece asked for nothing as None.
    fn list<'v>(&self, ids: impl IntoIterator<Item = Option<UnitId>>, heap: Heap<'v>) -> Value<'v> {
        let items: Vec<Value<'v>> = ids.into_iter().map(|id| id.map_or_else(Value::new_none, |id| heap.alloc(self.unit(id)))).collect();
        heap.alloc(AllocList(items))
    }

    /// The units a script handed over (a list of units), by id; a thing that isn't one of this column's units is refused.
    fn ids_of(&self, units: &[Value<'_>], what: &str) -> anyhow::Result<Vec<UnitId>> {
        units.iter().map(|v| self.id_of(*v, what)).collect()
    }

    fn id_of(&self, v: Value<'_>, what: &str) -> anyhow::Result<UnitId> {
        match v.downcast_ref::<AllocUnit>() {
            Some(u) => Ok(u.id),
            None => anyhow::bail!("{what} takes units of the column, not {} ({})", v.get_type(), v.to_repr()),
        }
    }
}

/// The block column an allocation step works on, its units top down.
#[derive(ProvidesStaticType, NoSerialize, StarlarkPagablePanic, Allocative)]
pub(crate) struct ColumnHandle {
    #[allocative(skip)]
    pub run: Run,
}
starlark_simple_value!(ColumnHandle);

impl fmt::Debug for ColumnHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "column({})", self.run.name)
    }
}

impl fmt::Display for ColumnHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "column({}, {} units)", self.run.name, self.run.lock().column.len())
    }
}

#[starlark_value(type = "column")]
impl<'v> StarlarkValue<'v> for ColumnHandle {
    fn get_methods() -> Option<&'static starlark::environment::Methods> {
        starlark::methods_static!(COLUMN_METHODS = column_methods);
        Some(COLUMN_METHODS.methods())
    }

    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        match attribute {
            "units" => {
                let ids: Vec<Option<UnitId>> = self.run.lock().column.units().iter().map(|u| Some(u.id)).collect();
                Some(self.run.list(ids, heap))
            }
            "name" => Some(heap.alloc_str(&self.run.name).to_value()),
            "block" => Some(heap.alloc(Parts { kind: "block", values: Some(self.run.column_block.clone()), log: self.run.log.clone() })),
            _ => None,
        }
    }

    fn has_attr(&self, attribute: &str, _heap: Heap<'v>) -> bool {
        matches!(attribute, "units" | "name" | "block")
    }

    fn dir_attr(&self) -> Vec<String> {
        ["units", "name", "block"].map(String::from).to_vec()
    }
}

/// `column.waste()`, `column.coal()`, `column.after(unit)`, `column.before(unit)`, `column.group_while(...)` and
/// `column.total(...)`: the column read in Rust.
#[starlark::starlark_module]
fn column_methods(builder: &mut starlark::environment::MethodsBuilder) {
    /// The units that end in ROM waste, top down: waste, partings left out of a section, losses and discarded coal.
    fn waste<'v>(this: &ColumnHandle, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let ids = ending(&this.run, Ends::Waste);
        Ok(this.run.list(ids, heap))
    }

    /// The units that end in a ROM coal section, top down: its coal, partings and dilution.
    fn coal<'v>(this: &ColumnHandle, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let ids = ending(&this.run, Ends::Coal);
        Ok(this.run.list(ids, heap))
    }

    /// The unit below this one in the column, None for the last.
    fn after<'v>(this: &ColumnHandle, #[starlark(require = pos)] unit: Value<'v>, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let id = this.run.id_of(unit, "after")?;
        let next = {
            let s = this.run.lock();
            let at = s.at(id)?;
            s.column.units().get(at + 1).map(|u| u.id)
        };
        Ok(next.map_or_else(Value::new_none, |id| heap.alloc(this.run.unit(id))))
    }

    /// The unit above this one in the column, None for the first.
    fn before<'v>(this: &ColumnHandle, #[starlark(require = pos)] unit: Value<'v>, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let id = this.run.id_of(unit, "before")?;
        let above = {
            let s = this.run.lock();
            let at = s.at(id)?;
            at.checked_sub(1).map(|a| s.column.units()[a].id)
        };
        Ok(above.map_or_else(Value::new_none, |id| heap.alloc(this.run.unit(id))))
    }

    /// Runs of neighbouring units, in the order given (every unit, top down, by default), that have the same value for
    /// each name (a field, a word or a label): `column.group_while("process", "zone", units=column.waste())`, a list of
    /// lists. Units given apart in the column but next to each other in the list are one run if their values agree.
    fn group_while<'v>(
        this: &ColumnHandle,
        #[starlark(args)] names: UnpackTuple<String>,
        #[starlark(require = named, default = NoneOr::None)] units: starlark::values::none::NoneOr<UnpackList<Value<'v>>>,
        heap: Heap<'v>,
    ) -> anyhow::Result<Value<'v>> {
        if names.items.is_empty() {
            anyhow::bail!("group_while takes the names to group by: column.group_while(\"process\", \"zone\")");
        }
        let ids = match units {
            starlark::values::none::NoneOr::Other(list) => this.run.ids_of(&list.items, "group_while's units")?,
            starlark::values::none::NoneOr::None => this.run.lock().column.units().iter().map(|u| u.id).collect(),
        };
        let keys: Vec<Vec<Option<ScriptValue>>> = {
            let s = this.run.lock();
            ids.iter().map(|&id| names.items.iter().map(|n| s.read(id, n, &this.run.declared)).collect()).collect()
        };
        if let Some(n) = names.items.iter().enumerate().find(|(k, _)| keys.first().is_some_and(|first| first[*k].is_none())).map(|(_, n)| n) {
            anyhow::bail!("`{n}` isn't a field, word or label of this dataflow{}", crate::check::did_you_mean(n, this.run.declared.fields.iter().map(|f| f.name.as_str())));
        }
        let mut groups: Vec<Vec<Option<UnitId>>> = Vec::new();
        for (k, &id) in ids.iter().enumerate() {
            match groups.last_mut() {
                Some(g) if k > 0 && keys[k] == keys[k - 1] => g.push(Some(id)),
                _ => groups.push(vec![Some(id)]),
            }
        }
        let lists: Vec<Value<'v>> = groups.into_iter().map(|g| this.run.list(g, heap)).collect();
        Ok(heap.alloc(AllocList(lists)))
    }

    /// The runs of neighbouring units that end in ROM waste, top down, each a list of its units in order: the waste
    /// intervals between the coal sections, a unit or several (two waste horizons, a discarded thin coal between them, a
    /// loss mined with the waste). `ends="coal"` gives the ROM coal sections' runs the same way.
    fn intervals<'v>(this: &ColumnHandle, #[starlark(require = named, default = "waste")] ends: &str, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let want = match ends {
            "waste" => Ends::Waste,
            "coal" => Ends::Coal,
            other => anyhow::bail!("intervals ends in \"waste\" (the default) or \"coal\", not {other:?}"),
        };
        let runs: Vec<Vec<Option<UnitId>>> = {
            let s = this.run.lock();
            let mut runs: Vec<Vec<Option<UnitId>>> = Vec::new();
            let mut current: Vec<Option<UnitId>> = Vec::new();
            for u in s.column.units() {
                if s.ends.get(&u.id) == Some(&want) {
                    current.push(Some(u.id));
                } else if !current.is_empty() {
                    runs.push(std::mem::take(&mut current));
                }
            }
            if !current.is_empty() {
                runs.push(current);
            }
            runs
        };
        let lists: Vec<Value<'v>> = runs.into_iter().map(|r| this.run.list(r, heap)).collect();
        Ok(heap.alloc(AllocList(lists)))
    }

    /// A thickness taken off the top of these units, across as many as it needs, the one it ends in split: the pieces
    /// taken, top down. Each piece is the column's own split, so the pieces and what's left add back exactly. More than
    /// the units hold is refused, saying how thick they are.
    fn take_from_top<'v>(this: &ColumnHandle, #[starlark(require = pos)] units: UnpackList<Value<'v>>, #[starlark(require = pos)] thickness: Value<'v>, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let ids = this.run.ids_of(&units.items, "take_from_top's units")?;
        let taken = take_thickness(&this.run, &ids, number(thickness, "take_from_top")?, End::Top, "take_from_top")?;
        note(&this.run.log, Event::Read { what: format!("take_from_top {} m", figure(number(thickness, "take_from_top")?)), value: ScriptValue::Text(format!("{} pieces", taken.len())) });
        Ok(this.run.list(taken.into_iter().map(Some), heap))
    }

    /// A thickness taken off the bottom of these units, across as many as it needs, as `take_from_top` from the top: a
    /// dragline system's zones from the bottom up, a wedge over the coal.
    fn take_from_bottom<'v>(this: &ColumnHandle, #[starlark(require = pos)] units: UnpackList<Value<'v>>, #[starlark(require = pos)] thickness: Value<'v>, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let ids = this.run.ids_of(&units.items, "take_from_bottom's units")?;
        let taken = take_thickness(&this.run, &ids, number(thickness, "take_from_bottom")?, End::Bottom, "take_from_bottom")?;
        note(&this.run.log, Event::Read { what: format!("take_from_bottom {} m", figure(number(thickness, "take_from_bottom")?)), value: ScriptValue::Text(format!("{} pieces", taken.len())) });
        Ok(this.run.list(taken.into_iter().map(Some), heap))
    }

    /// A number field over units (every unit by default) added up by its own rule: a volume summed, a quality averaged
    /// by its weight. None where no unit has a value.
    fn total<'v>(
        this: &ColumnHandle,
        #[starlark(require = pos)] field: &str,
        #[starlark(require = named, default = NoneOr::None)] units: starlark::values::none::NoneOr<UnpackList<Value<'v>>>,
    ) -> anyhow::Result<starlark::values::none::NoneOr<f64>> {
        let s = this.run.lock();
        let Ok(i) = s.column.fields().at(field) else {
            anyhow::bail!("`{field}` isn't a number field of this dataflow{}", crate::check::did_you_mean(field, s.column.fields().list().iter().map(|f| f.name.as_str())));
        };
        let at: Vec<usize> = match units {
            starlark::values::none::NoneOr::Other(list) => {
                let ids = this.run.ids_of(&list.items, "total's units")?;
                ids.iter().map(|&id| s.at(id)).collect::<anyhow::Result<_>>()?
            }
            starlark::values::none::NoneOr::None => (0..s.column.len()).collect(),
        };
        let x = s.column.add_up(at)[i];
        note(&this.run.log, Event::Read { what: format!("column.total({field})"), value: if x.is_nan() { ScriptValue::None } else { ScriptValue::Number(x) } });
        Ok(if x.is_nan() { starlark::values::none::NoneOr::None } else { starlark::values::none::NoneOr::Other(x) })
    }
}

use starlark::values::none::NoneOr;

/// `thickness` taken off the top or bottom of these units in the column's order, whole units first and the one it ends
/// in split there; the pieces taken, top down. Each split is noted as `split_thickness` notes one.
fn take_thickness(run: &Run, ids: &[UnitId], asked: f64, end: End, call: &str) -> anyhow::Result<Vec<UnitId>> {
    let mut s = run.lock();
    let thickness = s.column.fields().thickness();
    let mut order: Vec<(usize, UnitId)> = ids.iter().map(|&id| s.at(id).map(|at| (at, id))).collect::<anyhow::Result<_>>()?;
    order.sort_unstable();
    order.dedup();
    let held: f64 = order.iter().map(|&(at, _)| s.column.units()[at].value(thickness)).filter(|t| t.is_finite()).sum();
    if asked < 0.0 {
        anyhow::bail!("{call} can't take {asked} m");
    }
    if asked > held * (1.0 + 1e-12) + 1e-12 {
        anyhow::bail!("{call} asks {} m of units {} m thick", figure(asked), figure(held));
    }
    if end == End::Bottom {
        order.reverse();
    }
    let mut left = asked;
    let mut taken: Vec<UnitId> = Vec::new();
    for (_, id) in order {
        if left <= 0.0 {
            break;
        }
        let at = s.at(id)?;
        let has = s.column.units()[at].value(thickness);
        let has = if has.is_finite() { has } else { 0.0 };
        if has <= left * (1.0 + 1e-12) {
            taken.push(id);
            left -= has;
            continue;
        }
        let split = s.column.split(at, Amount::Thickness(left), end).map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let piece = s.column.units()[split.piece].id;
        let pieces = match end {
            End::Top => vec![Some(piece), Some(id)],
            End::Bottom => vec![Some(id), Some(piece)],
        };
        let side = if end == End::Top { "top" } else { "bottom" };
        settle(&mut s, id, format!("thickness {} off the {side}", figure(left)), &pieces);
        taken.push(piece);
        left = 0.0;
    }
    let mut placed: Vec<(usize, UnitId)> = taken.iter().map(|&id| s.at(id).map(|at| (at, id))).collect::<anyhow::Result<_>>()?;
    placed.sort_unstable();
    Ok(placed.into_iter().map(|(_, id)| id).collect())
}

fn ending(run: &Run, ends: Ends) -> Vec<Option<UnitId>> {
    let s = run.lock();
    s.column.units().iter().filter(|u| s.ends.get(&u.id) == Some(&ends)).map(|u| Some(u.id)).collect()
}

/// A unit of the column, by its id: what it reads is the column's as it stands, so a split or a set earlier in the
/// script is seen.
#[derive(ProvidesStaticType, NoSerialize, StarlarkPagablePanic, Allocative)]
pub(crate) struct AllocUnit {
    pub id: UnitId,
    #[allocative(skip)]
    pub run: Run,
}
starlark_simple_value!(AllocUnit);

impl fmt::Debug for AllocUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unit({})", self.id)
    }
}

impl fmt::Display for AllocUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unit({})", self.run.lock().words(self.id))
    }
}

/// `unit.split_thickness(0.5, 4.5)`, `unit.split_shares([0.2, 0.3, 0.5])` and `unit.place_onto(...)`.
#[starlark::starlark_module]
fn alloc_unit_methods(builder: &mut starlark::environment::MethodsBuilder) {
    /// Pieces off the top of the unit of these thicknesses, then the rest: a list one longer than the thicknesses asked,
    /// a piece of 0 m (or the rest, when nothing is left) None. Each piece takes the unit's labels and words, its summed
    /// fields shared by thickness and the others kept; the pieces add back to the unit exactly. Asking for more than the
    /// unit holds is refused with how thick it is.
    fn split_thickness<'v>(this: &AllocUnit, #[starlark(args)] thicknesses: UnpackTuple<Value<'v>>, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let asked: Vec<f64> = thicknesses.items.iter().map(|v| number(*v, "split_thickness")).collect::<anyhow::Result<_>>()?;
        if asked.is_empty() {
            anyhow::bail!("split_thickness takes the thicknesses to take off the top: unit.split_thickness(0.5, 4.5)");
        }
        let pieces = {
            let mut s = this.run.lock();
            let thickness = s.column.fields().thickness();
            let at = s.at(this.id)?;
            let had = s.column.units()[at].value(thickness);
            let total: f64 = asked.iter().sum();
            if let Some(bad) = asked.iter().find(|t| **t < 0.0) {
                anyhow::bail!("split_thickness can't take {bad} m off {}", s.words(this.id));
            }
            if total > had * (1.0 + 1e-12) + 1e-12 {
                anyhow::bail!("split_thickness asks {} m of {}, which is {} m thick", total, s.words(this.id), had);
            }
            let mut pieces: Vec<Option<UnitId>> = Vec::with_capacity(asked.len() + 1);
            let mut rest = Some(at);
            for &t in &asked {
                match rest {
                    Some(r) if t > 0.0 => {
                        let split = s.column.split(r, Amount::Thickness(t), End::Top).map_err(|e| anyhow::anyhow!(e.to_string()))?;
                        pieces.push(Some(s.column.units()[split.piece].id));
                        rest = split.rest;
                    }
                    _ => pieces.push(None),
                }
            }
            pieces.push(rest.map(|r| s.column.units()[r].id));
            let how = format!("thickness {}", asked.iter().map(|t| figure(*t)).collect::<Vec<_>>().join(", "));
            settle(&mut s, this.id, how, &pieces);
            pieces
        };
        note(&this.run.log, Event::Read { what: format!("{} split by thickness", this), value: ScriptValue::Text(format!("{} pieces", pieces.iter().flatten().count())) });
        Ok(this.run.list(pieces, heap))
    }

    /// The unit split by volume shares that add up to 1, in order top down: a list a share, a share of 0 None. Each piece
    /// takes the unit's labels and words, its summed fields shared by the share and the others kept; the last piece is
    /// what's left, so the pieces add back to the unit exactly.
    fn split_shares<'v>(this: &AllocUnit, #[starlark(require = pos)] shares: UnpackList<Value<'v>>, heap: Heap<'v>) -> anyhow::Result<Value<'v>> {
        let shares: Vec<f64> = shares.items.iter().map(|v| number(*v, "split_shares")).collect::<anyhow::Result<_>>()?;
        if let Some(bad) = shares.iter().find(|x| **x < 0.0) {
            anyhow::bail!("split_shares takes shares of 0 or more, not {bad}");
        }
        let sum: f64 = shares.iter().sum();
        if (sum - 1.0).abs() > 1e-9 {
            anyhow::bail!("split_shares' shares add up to {sum}, not 1");
        }
        let pieces = {
            let mut s = this.run.lock();
            let mut at = s.at(this.id)?;
            let last = shares.iter().rposition(|x| *x > 0.0).expect("shares adding to 1 have one over 0");
            let mut left: f64 = shares[..=last].iter().sum();
            let mut pieces: Vec<Option<UnitId>> = Vec::with_capacity(shares.len());
            for (k, &share) in shares.iter().enumerate() {
                if share <= 0.0 {
                    pieces.push(None);
                } else if k == last {
                    pieces.push(Some(s.column.units()[at].id));
                } else {
                    let split = s.column.split(at, Amount::Fraction(share / left), End::Top).map_err(|e| anyhow::anyhow!(e.to_string()))?;
                    pieces.push(Some(s.column.units()[split.piece].id));
                    at = split.rest.expect("a share less than what's left leaves a rest");
                    left -= share;
                }
            }
            let how = format!("shares {}", shares.iter().map(|x| figure(*x)).collect::<Vec<_>>().join(", "));
            settle(&mut s, this.id, how, &pieces);
            pieces
        };
        note(&this.run.log, Event::Read { what: format!("{} split by shares", this), value: ScriptValue::Text(format!("{} pieces", pieces.iter().flatten().count())) });
        Ok(this.run.list(pieces, heap))
    }

    /// As a formula's `unit.place_onto` (decision 5): a schedule field's value written on the leaf of the block these
    /// levels name, not the unit's own; or, with `unit=top`, on the leaves another unit of the column is written on,
    /// shared by its benches as its own pieces are (a blast's drill and blast on its top unit's leaves).
    fn place_onto<'v>(this: &AllocUnit, #[starlark(require = pos)] field: &str, #[starlark(require = pos)] value: Value<'v>, #[starlark(kwargs)] leaf: starlark::values::dict::UnpackDictEntries<String, Value<'v>>) -> anyhow::Result<NoneType> {
        let onto_unit = match leaf.entries.iter().find(|(k, _)| k == "unit") {
            Some((_, v)) => Some(this.run.id_of(*v, "place_onto's unit=")?),
            None => None,
        };
        let levels: Vec<(String, Value<'v>)> = leaf.entries.iter().filter(|(k, _)| k != "unit").cloned().collect();
        if onto_unit.is_some() && !levels.is_empty() {
            anyhow::bail!("place_onto goes on a unit's leaves (unit=) or on the levels named, not both");
        }
        let Some(mut placed) = crate::handles::placed(&this.run.declared, field, value, &levels)? else { return Ok(NoneType) };
        placed.unit = onto_unit;
        note(&this.run.log, Event::Placed { field: placed.field.clone(), value: placed.value, onto: placed.onto.clone() });
        this.run.lock().placed.push((this.id, placed));
        Ok(NoneType)
    }
}

/// The pieces of a split take the unit's ending, and the split is noted.
fn settle(s: &mut State, unit: UnitId, how: String, pieces: &[Option<UnitId>]) {
    if let Some(&ends) = s.ends.get(&unit) {
        for id in pieces.iter().flatten() {
            s.ends.insert(*id, ends);
        }
    }
    s.splits.push(SplitMade { unit, how, pieces: pieces.to_vec() });
}

fn figure(x: f64) -> String {
    let s = format!("{x:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn number(v: Value<'_>, what: &str) -> anyhow::Result<f64> {
    match from_starlark(v, Kind::Number) {
        Ok(ScriptValue::Number(x)) => Ok(x),
        Ok(_) => anyhow::bail!("{what} takes numbers, not None"),
        Err(why) => anyhow::bail!("{what} {why}"),
    }
}

impl AllocUnit {
    fn read(&self, name: &str) -> Option<ScriptValue> {
        self.run.lock().read(self.id, name, &self.run.declared)
    }

    fn origin(&self) -> Option<usize> {
        let s = self.run.lock();
        s.column.position(self.id).map(|at| s.column.units()[at].origin as usize)
    }
}

#[starlark_value(type = "unit")]
impl<'v> StarlarkValue<'v> for AllocUnit {
    fn get_methods() -> Option<&'static starlark::environment::Methods> {
        starlark::methods_static!(ALLOC_UNIT_METHODS = alloc_unit_methods);
        Some(ALLOC_UNIT_METHODS.methods())
    }

    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        Ok(other.downcast_ref::<AllocUnit>().is_some_and(|o| o.id == self.id && Arc::ptr_eq(&o.run.state, &self.run.state)))
    }

    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        match attribute {
            "block" => {
                let values = self.origin().and_then(|o| self.run.lock().blocks.get(o).cloned());
                return Some(heap.alloc(Parts { kind: "block", values, log: self.run.log.clone() }));
            }
            "p" => {
                let values = self.origin().and_then(|o| self.run.lock().parameters.get(o).cloned()).unwrap_or_default();
                return Some(heap.alloc(Params { values, log: self.run.log.clone(), lookups: Some(self.run.declared.lookups.clone()), reads: Some((self.run.reads.clone(), Some(self.id))) }));
            }
            _ => {}
        }
        let value = self.read(attribute)?;
        note(&self.run.log, Event::Read { what: format!("{self}.{attribute}"), value: value.clone() });
        Some(to_starlark(&value, heap))
    }

    fn has_attr(&self, attribute: &str, _heap: Heap<'v>) -> bool {
        matches!(attribute, "block" | "p") || self.read(attribute).is_some()
    }

    fn dir_attr(&self) -> Vec<String> {
        let mut names: Vec<String> = self.run.declared.fields.iter().map(|f| f.name.clone()).collect();
        names.extend(self.run.declared.labels.iter().cloned());
        names.extend(["block", "p"].map(String::from));
        names
    }

    fn at(&self, index: Value<'v>, heap: Heap<'v>) -> starlark::Result<Value<'v>> {
        let Some(name) = index.unpack_str() else {
            return Err(starlark::Error::new_other(anyhow::anyhow!("a unit's field is named in words, not {}", index.to_repr())));
        };
        match self.get_attr(name, heap) {
            Some(v) if !matches!(name, "block" | "p") => Ok(v),
            _ => Err(starlark::Error::new_other(anyhow::anyhow!("`{name}` isn't a field of this dataflow{}", crate::check::did_you_mean(name, self.run.declared.fields.iter().map(|f| f.name.as_str()))))),
        }
    }

    fn set_at(&self, index: Value<'v>, new_value: Value<'v>) -> starlark::Result<()> {
        let Some(name) = index.unpack_str() else {
            return Err(starlark::Error::new_other(anyhow::anyhow!("a unit's field is named in words, not {}", index.to_repr())));
        };
        self.set_attr(name, new_value)
    }

    /// Sets a field or word on the unit in the column. The volume is of record: a split shares it, nothing sets it.
    fn set_attr(&self, attribute: &str, new_value: Value<'v>) -> starlark::Result<()> {
        let fail = |why: String| Err(starlark::Error::new_other(anyhow::anyhow!(why)));
        let Some(field) = self.run.declared.fields.iter().find(|f| f.name == attribute) else {
            return fail(if self.run.declared.is_label(attribute) {
                format!("`{attribute}` is a label of the unit, not a field a script sets")
            } else {
                format!("`{attribute}` isn't a field of this dataflow{}", crate::check::did_you_mean(attribute, self.run.declared.fields.iter().map(|f| f.name.as_str())))
            });
        };
        let value = match from_starlark(new_value, field.kind) {
            Ok(v) => v,
            Err(why) => return fail(format!("`{attribute}` {why}")),
        };
        let words = self.to_string();
        {
            let mut s = self.run.lock();
            let Some(at) = s.column.position(self.id) else { return fail("a unit went missing from the column".into()) };
            match &value {
                ScriptValue::Text(w) => s.column.unit_mut(at).set_word(attribute, Some(w.clone())),
                ScriptValue::None if field.kind == Kind::Text => s.column.unit_mut(at).set_word(attribute, None),
                v => {
                    let i = s.column.fields().at(attribute).map_err(|e| starlark::Error::new_other(anyhow::anyhow!(e.to_string())))?;
                    if i == s.column.fields().volume() {
                        return fail(format!("it sets {attribute} on {words}: the volume is of record; split the unit to share it"));
                    }
                    s.column.unit_mut(at).set(i, if let ScriptValue::Number(x) = v { *x } else { f64::NAN });
                }
            }
        }
        note(&self.run.log, Event::Set { field: format!("{words}.{attribute}"), value });
        Ok(())
    }
}
