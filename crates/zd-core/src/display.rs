//! How objects look in the viewport: colour by RL or the layer's colour, and how see-through they are
//! (decision 0043). Display state, as hiding is: saved with the project, never a revision, never undone,
//! and nothing a command works out depends on it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{LayerId, ObjectId};

/// How a surface, grid or seam horizon is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Colouring {
    /// In bands by RL, at the contour interval.
    #[default]
    ByRl,
    /// One colour, the layer's.
    Layer,
}

/// How one object looks. The default, colour by RL and solid, isn't stored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Look {
    #[serde(default)]
    pub colouring: Colouring,
    /// 1 is solid, 0 is not seen at all.
    #[serde(default = "solid")]
    pub opacity: f64,
}

impl Default for Look {
    fn default() -> Self {
        Look { colouring: Colouring::ByRl, opacity: 1.0 }
    }
}

fn solid() -> f64 {
    1.0
}

fn yes() -> bool {
    true
}

/// The bands of colour by RL, for every object coloured that way.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Contours {
    /// Metres between band edges; `None` picks a round interval from the RL range.
    #[serde(default)]
    pub interval: Option<f64>,
    /// Thin lines at the band edges.
    #[serde(default = "yes")]
    pub lines: bool,
    /// One colour range across everything on show, so an RL is one colour on every surface. Off, each
    /// object takes the whole ramp over its own RLs.
    #[serde(default)]
    pub shared: bool,
}

impl Default for Contours {
    fn default() -> Self {
        Contours { interval: None, lines: true, shared: false }
    }
}

impl Contours {
    /// Whether these can be drawn: an interval, if set, is a positive number of metres.
    pub fn is_valid(&self) -> bool {
        self.interval.is_none_or(|i| i.is_finite() && i > 0.0)
    }
}

/// Every object's and layer's look, as saved. Only what differs from the default is kept.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Display {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objects: BTreeMap<ObjectId, Look>,
    /// A layer's opacity, which multiplies its objects'. A layer's solids draw as one mesh, so for them
    /// it's the only opacity there is.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layers: BTreeMap<LayerId, f64>,
    #[serde(default)]
    pub contours: Contours,
}

impl Display {
    pub fn look(&self, id: ObjectId) -> Look {
        self.objects.get(&id).copied().unwrap_or_default()
    }

    pub fn layer_opacity(&self, id: LayerId) -> f64 {
        self.layers.get(&id).copied().unwrap_or(1.0)
    }

    /// Sets one object's look; a default look is dropped rather than kept. Returns whether it changed.
    pub(crate) fn set_look(&mut self, id: ObjectId, look: Look) -> bool {
        if self.look(id) == look {
            return false;
        }
        if look == Look::default() {
            self.objects.remove(&id);
        } else {
            self.objects.insert(id, look);
        }
        true
    }

    pub(crate) fn set_layer_opacity(&mut self, id: LayerId, opacity: f64) -> bool {
        if self.layer_opacity(id) == opacity {
            return false;
        }
        if opacity == 1.0 {
            self.layers.remove(&id);
        } else {
            self.layers.insert(id, opacity);
        }
        true
    }
}

/// An opacity as asked for, held to 0 to 1. `None` for one that isn't a number.
pub(crate) fn opacity(asked: f64) -> Option<f64> {
    asked.is_finite().then(|| asked.clamp(0.0, 1.0))
}
