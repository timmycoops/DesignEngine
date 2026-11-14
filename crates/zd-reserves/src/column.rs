//! A block column and what a step can do to it (decision 0070, points 1 to 3 and 6): sort by the stack order, split a
//! unit from its top or bottom, relabel, group runs of units and add a group up by the fields' rules. There is no way to
//! take a unit out: a piece is set aside by saying what it became, so every summed field balances to the column's
//! inputs at every step.

use std::ops::Range;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ColumnError;
use crate::fields::{Fields, Weight};

/// A unit's id, unique in its column.
pub type UnitId = u32;

/// Which block a column is: one case's strip and block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ColumnKey {
    pub case: String,
    pub strip: String,
    pub block: String,
}

/// What a column's input unit was made from: the solid (its object id and name) the cuts made.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub object: String,
    pub name: String,
}

/// One layer of a unit's labels: in situ, working section or ROM.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Labels {
    pub material: String,
    pub seam: String,
    /// The section's or ROM piece's number in the column, once formed.
    pub id: Option<u32>,
}

impl Labels {
    pub fn new(material: &str, seam: &str) -> Self {
        Self { material: material.to_string(), seam: seam.to_string(), id: None }
    }
}

/// What a piece became, the fourth layer of labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BecameKind {
    /// Coal lost from a section (roof, floor, edge or other), set aside.
    Loss,
    /// Waste or coal taken into a section's ROM (roof, floor, edge, other or a parting).
    Dilution,
    /// A section too thin to mine, or one that couldn't supply its loss, set aside.
    ThinLoss,
    /// A section the ROM ash cut-off rejected, set aside.
    HighAshLoss,
    /// Below the smallest volume worth carrying, set aside.
    SmallVolume,
    /// Coal not worked (outside the working section), carried as waste.
    Discarded,
}

impl BecameKind {
    /// Whether a piece so labelled is out of the ROM: kept in the column and in its balance, never in the ROM.
    pub fn is_set_aside(self) -> bool {
        matches!(self, Self::Loss | Self::ThinLoss | Self::HighAshLoss | Self::SmallVolume)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Became {
    pub kind: BecameKind,
    /// Which (roof, floor, edge, other, parting), or a discarded coal's type.
    pub detail: String,
}

/// A piece of the column: where it sits, its four layers of labels, the input unit it came from and its values.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub id: UnitId,
    /// The input unit it came from, an index into the column's origins.
    pub origin: u32,
    /// Its place in the seam model's stack order, top down (decision 0070, point 2).
    pub stack: u32,
    pub in_situ: Labels,
    pub section: Option<Labels>,
    pub rom: Option<Labels>,
    pub became: Option<Became>,
    values: Vec<f64>,
}

impl Unit {
    pub fn value(&self, field: usize) -> f64 {
        self.values[field]
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// Sets a field. A summed field changed by hand no longer balances; the steps change only the others.
    pub fn set(&mut self, field: usize, value: f64) {
        self.values[field] = value;
    }

    pub fn is_set_aside(&self) -> bool {
        self.became.as_ref().is_some_and(|b| b.kind.is_set_aside())
    }
}

/// How much a split takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Amount {
    Volume(f64),
    Thickness(f64),
    Fraction(f64),
}

/// Which end of a unit a split takes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Top,
    Bottom,
}

/// Where a split's pieces now sit: the piece taken and what's left of the unit, `None` when the piece was all of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Split {
    pub piece: usize,
    pub rest: Option<usize>,
}

/// How far past what a unit holds a split may ask, relative, before it's short: rounding, not a shortfall.
const SPLIT_TOLERANCE: f64 = 1e-12;

/// The units of one block column, in stack order.
#[derive(Clone, Debug)]
pub struct Column {
    pub key: ColumnKey,
    fields: Arc<Fields>,
    units: Vec<Unit>,
    origins: Vec<Origin>,
    next_id: UnitId,
}

impl Column {
    pub fn new(key: ColumnKey, fields: Arc<Fields>) -> Self {
        Self { key, fields, units: Vec::new(), origins: Vec::new(), next_id: 0 }
    }

    pub fn fields(&self) -> &Fields {
        &self.fields
    }

    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    pub fn unit_mut(&mut self, at: usize) -> &mut Unit {
        &mut self.units[at]
    }

    pub fn len(&self) -> usize {
        self.units.len()
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    pub fn origins(&self) -> &[Origin] {
        &self.origins
    }

    /// Where the unit with this id sits now.
    pub fn position(&self, id: UnitId) -> Option<usize> {
        self.units.iter().position(|u| u.id == id)
    }

    /// Adds an input unit at the foot of the column: one value a field, its volume and thickness not negative.
    pub fn push_input(&mut self, origin: Origin, stack: u32, in_situ: Labels, values: Vec<f64>) -> Result<UnitId, ColumnError> {
        if values.len() != self.fields.len() {
            return Err(ColumnError::Values { name: origin.name, expected: self.fields.len(), got: values.len() });
        }
        for field in [self.fields.volume(), self.fields.thickness()] {
            let v = values[field];
            if !(v >= 0.0 && v.is_finite()) {
                let name = self.fields.list()[field].name.clone();
                return Err(ColumnError::BadSize { unit: origin.name, field: name, value: v });
            }
        }
        let id = self.take_id();
        self.origins.push(origin);
        let origin = (self.origins.len() - 1) as u32;
        self.units.push(Unit { id, origin, stack, in_situ, section: None, rom: None, became: None, values });
        Ok(id)
    }

    fn take_id(&mut self) -> UnitId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Puts the units in the seam model's stack order, top down. Stable: the pieces of one place keep their order.
    pub fn sort(&mut self) {
        self.units.sort_by_key(|u| u.stack);
    }

    /// Splits a piece off the top or bottom of the unit at `at`. The piece is a new unit with the unit's labels and
    /// origin, placed above the rest when taken from the top and below it when from the bottom; the rest keeps the
    /// unit's id. Summed fields are shared out so the two add up exactly to the unit; every other value is kept. Asking
    /// for more than the unit holds is a shortfall, refused with how much was asked and how much there was.
    pub fn split(&mut self, at: usize, amount: Amount, end: End) -> Result<Split, ColumnError> {
        let (volume, thickness) = (self.fields.volume(), self.fields.thickness());
        let unit = &self.units[at];
        let (asked, had, field) = match amount {
            Amount::Volume(v) => (v, unit.values[volume], volume),
            Amount::Thickness(t) => (t, unit.values[thickness], thickness),
            Amount::Fraction(f) => (f, 1.0, usize::MAX),
        };
        if !(asked > 0.0 && asked.is_finite()) {
            return Err(ColumnError::BadAmount { unit: unit.id, asked });
        }
        if asked > had * (1.0 + SPLIT_TOLERANCE) {
            return Err(ColumnError::ShortOf { unit: unit.id, asked, had });
        }
        if asked >= had {
            return Ok(Split { piece: at, rest: None });
        }
        let fraction = asked / had;
        let mut piece = unit.clone();
        piece.id = self.take_id();
        let rest = &mut self.units[at];
        for i in 0..self.fields.len() {
            if !self.fields.is_summed(i) {
                continue;
            }
            let share = if i == field { asked } else { rest.values[i] * fraction };
            piece.values[i] = share;
            rest.values[i] -= share;
        }
        Ok(match end {
            End::Top => {
                self.units.insert(at, piece);
                Split { piece: at, rest: Some(at + 1) }
            }
            End::Bottom => {
                self.units.insert(at + 1, piece);
                Split { piece: at + 1, rest: Some(at) }
            }
        })
    }

    /// Labels the unit at `at` as what it became. A piece set aside stays in the column and in its balance.
    pub fn label_became(&mut self, at: usize, became: Became) {
        self.units[at].became = Some(became);
    }

    /// The runs of neighbouring units `same` holds between, top down, covering the column.
    pub fn runs(&self, same: impl Fn(&Unit, &Unit) -> bool) -> Vec<Range<usize>> {
        let mut runs = Vec::new();
        let mut start = 0;
        for i in 1..=self.units.len() {
            if i == self.units.len() || !same(&self.units[i - 1], &self.units[i]) {
                if i > start {
                    runs.push(start..i);
                }
                start = i;
            }
        }
        runs
    }

    /// The units at these places added up by each field's rule.
    pub fn add_up(&self, at: impl IntoIterator<Item = usize>) -> Vec<f64> {
        let units: Vec<&Unit> = at.into_iter().map(|i| &self.units[i]).collect();
        add_up(&self.fields, &units)
    }

    /// Every unit, set aside or not, added up: its summed fields are the inputs' at every step.
    pub fn balance(&self) -> Vec<f64> {
        self.add_up(0..self.units.len())
    }
}

/// Units added up by each field's rule. A sum adds; a volume or mass average weighs by volume or volume × density,
/// leaving out units of no weight; the largest and smallest are of all. Nothing missing is hidden: a missing value
/// (NaN) among units that weigh gives a missing answer. No units, or no weight, gives NaN for an average.
pub(crate) fn add_up(fields: &Fields, units: &[&Unit]) -> Vec<f64> {
    let volume = fields.volume();
    (0..fields.len())
        .map(|i| match fields.weight(i) {
            Weight::Sum => units.iter().map(|u| u.values[i]).sum(),
            Weight::Volume => weighted(units, i, |u| u.values[volume]),
            Weight::Mass(d) => weighted(units, i, |u| u.values[volume] * u.values[d]),
            Weight::Max => extreme(units, i, f64::max),
            Weight::Min => extreme(units, i, f64::min),
        })
        .collect()
}

fn weighted(units: &[&Unit], i: usize, weight: impl Fn(&Unit) -> f64) -> f64 {
    let (mut sum, mut total) = (0.0, 0.0);
    for u in units {
        let w = weight(u);
        if w != 0.0 {
            sum += w * u.values[i];
            total += w;
        }
    }
    if total == 0.0 { f64::NAN } else { sum / total }
}

fn extreme(units: &[&Unit], i: usize, pick: fn(f64, f64) -> f64) -> f64 {
    let mut values = units.iter().map(|u| u.values[i]);
    let Some(first) = values.next() else { return f64::NAN };
    values.fold(first, |a, b| if a.is_nan() || b.is_nan() { f64::NAN } else { pick(a, b) })
}
