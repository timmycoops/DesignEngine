//! Coarser versions of a drawn chunk (decision 0097): what the view draws far off, so a big lidar or a layer of blocks
//! isn't drawn triangle for triangle from across the lease. Only the drawing uses them; every pick, snap, cut and volume
//! stays on the full f64 mesh.
//!
//! Each version halves the one before, by meshoptimizer's edge collapses, with the chunk's border locked, so two chunks
//! side by side meet with no crack whatever version each is drawn at. Every version indexes the chunk's own vertices, so
//! only an index list is added. Each knows its error: the farthest any of the chunk's vertices lies from it, measured.

use std::time::Instant;

use meshopt::{SimplifyOptions, simplify_with_locks_decoder};
use parry3d_f64::math::Vector;
use parry3d_f64::query::PointQuery;
use parry3d_f64::shape::TriMesh as ParryTriMesh;

/// A version stops being made when it's this small, in triangles: below it, a chunk costs nothing to draw.
pub const FEWEST_TRIANGLES: usize = 256;

/// A version is kept only if it has at most this share of the triangles of the one before; one that can't halve again
/// (its border locked, or nothing more to take without tearing) ends the list.
const MOST_KEPT: f64 = 0.8;

/// One coarser version of a chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayLevel {
    /// Its triangles, as indices into the chunk's vertices, three a triangle.
    pub indices: Vec<u32>,
    /// The farthest any of the chunk's vertices lies from this version (m): how far it can look wrong.
    pub error: f64,
}

/// What [`display_levels`] made, and how long it took.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayLevels {
    /// Coarser and coarser, the full chunk not among them.
    pub levels: Vec<DisplayLevel>,
    pub ms: f64,
}

/// The coarser versions of a chunk drawn from `positions` (local coordinates, as the view has them) and `indices`
/// (three a triangle). Each halves the one before, down to [`FEWEST_TRIANGLES`]. The chunk's border never moves, and
/// nor does any vertex `locked` names (`locked[v]`, one a vertex, or none): a solid's edges where its walls meet its top.
pub fn display_levels(positions: &[[f32; 3]], indices: &[u32], locked: Option<&[bool]>) -> DisplayLevels {
    let start = Instant::now();
    let mut levels: Vec<DisplayLevel> = Vec::new();
    let full = indices.len() / 3;
    if full <= FEWEST_TRIANGLES || positions.is_empty() {
        return DisplayLevels { levels, ms: ms(start) };
    }
    // The border found here, from the edges one triangle uses, and locked with what the caller locks: meshoptimizer's own
    // border lock let a border vertex go on a rolling chunk, which would open a crack against the chunk beside it.
    let mut locks = on_border(positions.len(), indices);
    if let Some(named) = locked.filter(|l| l.len() == positions.len()) {
        for (lock, &named) in locks.iter_mut().zip(named) {
            *lock |= named;
        }
    }
    let locks = &locks;
    let options = SimplifyOptions::LockBorder | SimplifyOptions::ErrorAbsolute;
    let mut before = full;
    let mut from: &[u32] = indices;
    loop {
        let target = before / 2;
        if target < FEWEST_TRIANGLES {
            break;
        }
        let made = simplify_with_locks_decoder(from, positions, locks, target * 3, f32::MAX, options, None);
        let count = made.len() / 3;
        if count == 0 || count as f64 > MOST_KEPT * before as f64 {
            break;
        }
        let error = farthest_from(positions, &made);
        levels.push(DisplayLevel { indices: made, error });
        before = count;
        from = &levels.last().expect("just pushed").indices;
    }
    DisplayLevels { levels, ms: ms(start) }
}

/// Which of `count` vertices are on the border of the triangles `indices` makes: an end of an edge only one triangle uses.
fn on_border(count: usize, indices: &[u32]) -> Vec<bool> {
    let mut uses: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::with_capacity(indices.len());
    for t in indices.as_chunks::<3>().0 {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *uses.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let mut border = vec![false; count];
    for ((a, b), n) in uses {
        if n == 1 {
            border[a as usize] = true;
            border[b as usize] = true;
        }
    }
    border
}

/// The farthest any of `positions` lies from the triangles `indices` makes of them (m). The full chunk's surface runs
/// straight between its vertices, so this is how far the version can be from it at a vertex.
fn farthest_from(positions: &[[f32; 3]], indices: &[u32]) -> f64 {
    let vertices: Vec<Vector> = positions.iter().map(|p| Vector::new(p[0] as f64, p[1] as f64, p[2] as f64)).collect();
    let triangles: Vec<[u32; 3]> = indices.as_chunks::<3>().0.to_vec();
    // A vertex the version keeps lies on it: only those it dropped are measured.
    let mut kept = vec![false; positions.len()];
    for &v in indices {
        kept[v as usize] = true;
    }
    let Ok(mesh) = ParryTriMesh::new(vertices.clone(), triangles) else {
        return f64::INFINITY;
    };
    vertices.iter().zip(&kept).filter(|(_, k)| !**k).map(|(p, _)| mesh.distance_to_local_point(*p, true)).fold(0.0, f64::max)
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
