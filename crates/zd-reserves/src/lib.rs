//! The reserves dataflow's column engine (decision 0070).
//!
//! A run works one block column at a time: the units of one case's strip and block, in the seam model's stack order.
//! Each unit carries every layer of labels at once (in situ, working section, ROM and what it became) and the input unit
//! it came from. A step changes a column only by sorting it, splitting a unit from its top or bottom, relabelling, and
//! grouping units to add them up by each field's declared rule. Nothing is taken out: a piece is set aside by a label,
//! so the column always balances to its inputs. The steps (small volumes to clean coal), the dataflow object and its
//! runner come in later workstreams.

mod column;
mod fields;

use std::fmt;

pub use column::{Amount, Became, BecameKind, Column, ColumnKey, End, Labels, Origin, Split, Unit, UnitId};
pub use fields::{AddUp, Field, FieldKind, Fields};

/// Why a column refused a change, said in full.
#[derive(Clone, Debug, PartialEq)]
pub enum ColumnError {
    DuplicateField(String),
    UnknownField(String),
    /// The fields need exactly one of this kind.
    NeedsOne { kind: String, found: usize },
    /// The volume or thickness must add up by sum.
    NotSummed(String),
    /// A mass average names a density that isn't a field, or is itself a mass average.
    NoDensity { field: String, density: String },
    /// An input unit with the wrong number of values.
    Values { name: String, expected: usize, got: usize },
    /// An input unit with a negative or missing volume or thickness.
    BadSize { unit: String, field: String, value: f64 },
    /// A split asked for nothing, or for a negative or missing amount.
    BadAmount { unit: UnitId, asked: f64 },
    /// A split asked for more than the unit holds.
    ShortOf { unit: UnitId, asked: f64, had: f64 },
}

impl fmt::Display for ColumnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateField(name) => write!(f, "the field {name} is declared twice"),
            Self::UnknownField(name) => write!(f, "there's no field {name}"),
            Self::NeedsOne { kind, found } => write!(f, "the fields need exactly one {kind} field; there are {found}"),
            Self::NotSummed(name) => write!(f, "{name} must add up by sum"),
            Self::NoDensity { field, density } => {
                write!(f, "{field} is averaged by mass with {density}, which isn't a field or is itself averaged by mass")
            }
            Self::Values { name, expected, got } => write!(f, "{name} has {got} values for {expected} fields"),
            Self::BadSize { unit, field, value } => write!(f, "{unit} has a {field} of {value}"),
            Self::BadAmount { unit, asked } => write!(f, "unit {unit}: a split of {asked} isn't a piece"),
            Self::ShortOf { unit, asked, had } => write!(f, "unit {unit} is short: {asked} asked of {had}"),
        }
    }
}

impl std::error::Error for ColumnError {}
