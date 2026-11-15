//! The fields a column's units carry, each declared once with how it adds up (decision 0070, point 4).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::ColumnError;

/// What a field holds. The engine needs only the volume and the thickness; the rest say what a step may do with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// The unit's volume (m³), the volume of record. Exactly one, adding up by sum.
    Volume,
    /// The unit's true thickness (m). Exactly one, adding up by sum down the column.
    Thickness,
    Area,
    /// A block's length in plan (m), as the interrogation measured it: what edge loss and dilution run along.
    Length,
    Density,
    Quality,
    /// A moisture (%): air dried, in situ or ROM, each averaged by the tonnes on its own basis.
    Moisture,
    MoistureFactor,
    /// A setting copied onto the units (a threshold, the ash cut-off), so a group's rule is its parts' aggregate.
    Setting,
    Other,
}

/// How a field adds up over a group of units, and so whether a split shares it out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "by")]
pub enum AddUp {
    /// Added; a split shares it out by the piece's fraction (volume, thickness, tonnes).
    Sum,
    /// Averaged by volume (density on its basis).
    Volume,
    /// Averaged by mass, volume × the named density field (raw qualities by tonnes).
    Mass { density: String },
    /// The largest (a threshold: the strictest of a group's plies).
    Max,
    /// The smallest (the ash cut-off).
    Min,
}

/// One field: its name, what it is and how it adds up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    pub add_up: AddUp,
}

impl Field {
    pub fn new(name: &str, kind: FieldKind, add_up: AddUp) -> Self {
        Self { name: name.to_string(), kind, add_up }
    }
}

/// How a field adds up, its density resolved to a place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Weight {
    Sum,
    Volume,
    Mass(usize),
    Max,
    Min,
}

/// The fields of a dataflow, in order, checked once: names unique, one volume and one thickness both summed, every
/// density a mass average names a field of its own that isn't itself averaged by mass.
#[derive(Clone, Debug)]
pub struct Fields {
    list: Vec<Field>,
    index: HashMap<String, usize>,
    weights: Vec<Weight>,
    volume: usize,
    thickness: usize,
}

impl Fields {
    pub fn new(list: Vec<Field>) -> Result<Self, ColumnError> {
        let mut index = HashMap::new();
        for (i, f) in list.iter().enumerate() {
            if index.insert(f.name.clone(), i).is_some() {
                return Err(ColumnError::DuplicateField(f.name.clone()));
            }
        }
        let only = |kind: FieldKind, word: &str| -> Result<usize, ColumnError> {
            let found: Vec<usize> = (0..list.len()).filter(|&i| list[i].kind == kind).collect();
            match found[..] {
                [i] if list[i].add_up == AddUp::Sum => Ok(i),
                [i] => Err(ColumnError::NotSummed(list[i].name.clone())),
                _ => Err(ColumnError::NeedsOne { kind: word.to_string(), found: found.len() }),
            }
        };
        let volume = only(FieldKind::Volume, "volume")?;
        let thickness = only(FieldKind::Thickness, "thickness")?;
        let weights = list
            .iter()
            .map(|f| match &f.add_up {
                AddUp::Sum => Ok(Weight::Sum),
                AddUp::Volume => Ok(Weight::Volume),
                AddUp::Max => Ok(Weight::Max),
                AddUp::Min => Ok(Weight::Min),
                AddUp::Mass { density } => match index.get(density) {
                    Some(&d) if !matches!(list[d].add_up, AddUp::Mass { .. }) => Ok(Weight::Mass(d)),
                    _ => Err(ColumnError::NoDensity { field: f.name.clone(), density: density.clone() }),
                },
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { list, index, weights, volume, thickness })
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn list(&self) -> &[Field] {
        &self.list
    }

    /// A field's place, by name.
    pub fn at(&self, name: &str) -> Result<usize, ColumnError> {
        self.index.get(name).copied().ok_or_else(|| ColumnError::UnknownField(name.to_string()))
    }

    pub fn volume(&self) -> usize {
        self.volume
    }

    pub fn thickness(&self) -> usize {
        self.thickness
    }

    pub(crate) fn weight(&self, i: usize) -> Weight {
        self.weights[i]
    }

    /// Whether a split shares the field out by the piece's fraction.
    pub fn is_summed(&self, i: usize) -> bool {
        self.weights[i] == Weight::Sum
    }
}
