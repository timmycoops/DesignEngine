//! Solids cut at bench levels (decision 0071): each solid split by level planes at RLs, bottom up, one piece a bench it
//! reaches into, with what lies below the lowest level and above the highest kept as a piece each. A level is a flat
//! plane, so each cut is Manifold's split by a plane, not a boolean with a closed surface. Flaps thinner than the flap
//! thickness, where a level lies just off a nearly flat face of the solid (a berm or a seam's roof a few millimetres from
//! it), are taken off as the seam cut takes them (decision 0064) and their volume said. The pieces and the flaps add up
//! to the solid.
//!
//! Worked about a whole-metre origin near the solid in plan, so mine grid coordinates keep their precision; RLs stay as
//! they are.

use std::time::Instant;

use manifold_csg::Manifold;
use zd_core::TriMesh;

use crate::cut::{from_manifold, to_manifold};
use crate::layers::without_flaps;
use crate::{GeomError, TinyEdges, join_tiny_edges};

/// A piece with less volume than this (m³) is nothing: a level only touching the solid.
const NOTHING: f64 = 1e-6;

/// The most levels one set of benches may have: 0 to 500 every 10 m is 51.
pub const MAX_LEVELS: usize = 100_000;

/// A solid's piece between two levels, or below the lowest or above the highest.
#[derive(Debug, Clone)]
pub struct BenchPiece {
    /// The level under it; `None` below the lowest level.
    pub floor: Option<f64>,
    /// The level over it; `None` above the highest level.
    pub top: Option<f64>,
    pub mesh: TriMesh,
    pub volume: f64,
    /// Separate parts in it: more than one where the level crosses the solid in two places.
    pub parts: usize,
    pub tiny_edges: TinyEdges,
}

/// One solid cut at bench levels.
#[derive(Debug, Clone)]
pub struct BenchCut {
    /// A piece for each bench the solid reaches into, bottom up.
    pub pieces: Vec<BenchPiece>,
    /// How many pieces had flaps taken off, and their volume (m³).
    pub flapped: usize,
    pub flaps: f64,
    pub input_volume: f64,
    /// Levels that passed through the solid, and so cut it.
    pub levels_cut: usize,
    pub ms: f64,
}

impl BenchCut {
    /// The pieces and the flaps together, less the solid (m³): what the cut lost or gained.
    pub fn difference(&self) -> f64 {
        self.pieces.iter().map(|p| p.volume).sum::<f64>() + self.flaps - self.input_volume
    }
}

/// The levels from `from` to `to` every `every` metres, lowest first: `from`, `from + every`, … up to `to` or the last
/// level under it.
pub fn bench_levels(from: f64, to: f64, every: f64) -> Result<Vec<f64>, GeomError> {
    if !(from.is_finite() && to.is_finite()) {
        return Err(GeomError::InvalidInput(format!("levels from {from} to {to} aren't both numbers")));
    }
    if !(every > 0.0 && every.is_finite()) {
        return Err(GeomError::InvalidInput(format!("benches every {every} m aren't more than nothing")));
    }
    if to < from {
        return Err(GeomError::InvalidInput(format!("levels from RL {from} to RL {to} run downward: the from RL is the lower")));
    }
    // A hair over, so a to RL on the levels by its figures is one of them despite rounding.
    let steps = ((to - from) / every + 1e-9).floor();
    if steps >= MAX_LEVELS as f64 {
        return Err(GeomError::InvalidInput(format!("levels from RL {from} to RL {to} every {every} m are more than {MAX_LEVELS}")));
    }
    Ok((0..=steps as usize).map(|k| from + k as f64 * every).collect())
}

/// Cuts a closed solid at each of `levels` (RLs, lowest first) that passes through it: one piece a bench, bottom up,
/// each with its flaps thinner than `flap_thickness` taken off (none with 0).
pub fn cut_solid_by_levels(solid: &TriMesh, levels: &[f64], flap_thickness: f64) -> Result<BenchCut, GeomError> {
    if levels.iter().any(|z| !z.is_finite()) || levels.windows(2).any(|w| w[1] <= w[0]) {
        return Err(GeomError::InvalidInput("bench levels must be numbers, each higher than the one before".into()));
    }
    if !(flap_thickness >= 0.0 && flap_thickness.is_finite()) {
        return Err(GeomError::InvalidInput("the flap thickness can't be less than nothing".into()));
    }
    let validity = solid.validity();
    if !validity.is_watertight() {
        return Err(GeomError::NotWatertight { what: "solid", validity });
    }
    let started = Instant::now();
    let bounds = solid.bounds().ok_or_else(|| GeomError::InvalidInput("empty solid".into()))?;
    let centre = bounds.centre();
    let origin = [centre[0].round(), centre[1].round(), 0.0];
    let input_volume = solid.signed_volume();
    let mut rest = to_manifold(solid, [-origin[0], -origin[1], 0.0], "solid")?;

    // The levels at or under the solid's lowest point don't cut it; the first above it is where cutting starts.
    let (low, high) = (bounds.min[2], bounds.max[2]);
    let mut k = levels.partition_point(|&z| z <= low);
    let (mut pieces, mut flapped, mut flaps, mut levels_cut) = (Vec::new(), 0, 0.0, 0);
    let mut take = |m: Manifold, floor: Option<f64>, top: Option<f64>| -> Result<(), GeomError> {
        let (kept, flap) = without_flaps(m, flap_thickness)?;
        if flap > 0.0 {
            flapped += 1;
            flaps += flap;
        }
        if let Some(piece) = piece_of(&kept, floor, top, origin)? {
            pieces.push(piece);
        }
        Ok(())
    };
    while k < levels.len() && levels[k] < high {
        let (above, below) = rest.split_by_plane([0.0, 0.0, 1.0], levels[k]);
        let floor = k.checked_sub(1).map(|j| levels[j]);
        take(below, floor, Some(levels[k]))?;
        rest = above;
        levels_cut += 1;
        k += 1;
    }
    take(rest, k.checked_sub(1).map(|j| levels[j]), levels.get(k).copied())?;
    Ok(BenchCut { pieces, flapped, flaps, input_volume, levels_cut, ms: started.elapsed().as_secs_f64() * 1000.0 })
}

/// A split's part as a piece, or nothing if it has no volume.
fn piece_of(m: &Manifold, floor: Option<f64>, top: Option<f64>, origin: [f64; 3]) -> Result<Option<BenchPiece>, GeomError> {
    m.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    if m.is_empty() || m.volume() < NOTHING {
        return Ok(None);
    }
    let parts = m.decompose().len();
    let (mesh, tiny_edges) = join_tiny_edges(from_manifold(m, origin));
    let volume = mesh.signed_volume();
    Ok(Some(BenchPiece { floor, top, mesh, volume, parts, tiny_edges }))
}
