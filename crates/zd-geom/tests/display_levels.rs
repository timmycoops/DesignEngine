//! Coarser versions of a drawn chunk (decision 0097, workstream 1), each with a known answer.

use std::collections::HashSet;
use std::time::Instant;

use zd_geom::{FEWEST_TRIANGLES, display_levels};

/// A chunk of `cols` by `rows` nodes a metre apart from column `first`, its heights by `z`: positions and indices.
fn lattice(first: usize, cols: usize, rows: usize, z: impl Fn(f64, f64) -> f64) -> (Vec<[f32; 3]>, Vec<u32>) {
    let mut positions = Vec::with_capacity(cols * rows);
    for r in 0..rows {
        for c in first..first + cols {
            positions.push([c as f32, r as f32, z(c as f64, r as f64) as f32]);
        }
    }
    let mut indices = Vec::with_capacity((cols - 1) * (rows - 1) * 6);
    for r in 0..rows - 1 {
        for c in 0..cols - 1 {
            let at = |c: usize, r: usize| (r * cols + c) as u32;
            indices.extend([at(c, r), at(c + 1, r), at(c + 1, r + 1), at(c, r), at(c + 1, r + 1), at(c, r + 1)]);
        }
    }
    (positions, indices)
}

fn rolling(x: f64, y: f64) -> f64 {
    5.0 * (x / 30.0).sin() * (y / 40.0).cos() + 2.0 * ((x + y) / 17.0).sin()
}

/// The distance from `p` to the triangle `a b c`, worked out directly.
fn to_triangle(p: [f64; 3], a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let sub = |u: [f64; 3], v: [f64; 3]| [u[0] - v[0], u[1] - v[1], u[2] - v[2]];
    let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let (ab, ac, ap) = (sub(b, a), sub(c, a), sub(p, a));
    let (d1, d2) = (dot(ab, ap), dot(ac, ap));
    let closest = if d1 <= 0.0 && d2 <= 0.0 {
        a
    } else {
        let bp = sub(p, b);
        let (d3, d4) = (dot(ab, bp), dot(ac, bp));
        let cp = sub(p, c);
        let (d5, d6) = (dot(ab, cp), dot(ac, cp));
        let (vc, vb, va) = (d1 * d4 - d3 * d2, d5 * d2 - d1 * d6, d3 * d6 - d5 * d4);
        let along = |o: [f64; 3], e: [f64; 3], t: f64| [o[0] + t * e[0], o[1] + t * e[1], o[2] + t * e[2]];
        if d3 >= 0.0 && d4 <= d3 {
            b
        } else if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            along(a, ab, d1 / (d1 - d3))
        } else if d6 >= 0.0 && d5 <= d6 {
            c
        } else if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            along(a, ac, d2 / (d2 - d6))
        } else if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
            along(b, sub(c, b), (d4 - d3) / ((d4 - d3) + (d5 - d6)))
        } else {
            let denom = 1.0 / (va + vb + vc);
            let (v, w) = (vb * denom, vc * denom);
            [a[0] + ab[0] * v + ac[0] * w, a[1] + ab[1] * v + ac[1] * w, a[2] + ab[2] * v + ac[2] * w]
        }
    };
    let d = sub(p, closest);
    dot(d, d).sqrt()
}

/// A flat chunk of 99,458 triangles: every version lies on it (error 0), and the last keeps only its 892 border
/// vertices, the 890 triangles a polygon of 892 corners takes.
#[test]
fn a_flat_chunk_keeps_only_its_border() {
    let (positions, indices) = lattice(0, 224, 224, |_, _| 100.0);
    assert_eq!(indices.len() / 3, 2 * 223 * 223);
    let made = display_levels(&positions, &indices, None);
    assert!(!made.levels.is_empty());
    for level in &made.levels {
        assert!(level.error <= 1e-9, "{}", level.error);
    }
    // A polygon of n corners with i vertices inside takes n - 2 + 2i triangles: the last version has almost none inside.
    let last = made.levels.last().unwrap();
    let on_border = |v: u32| {
        let p = positions[v as usize];
        p[0] == 0.0 || p[0] == 223.0 || p[1] == 0.0 || p[1] == 223.0
    };
    let inside: HashSet<u32> = last.indices.iter().copied().filter(|&v| !on_border(v)).collect();
    assert!(inside.len() <= 4, "{} inside vertices left", inside.len());
    assert_eq!(last.indices.len() / 3, 4 * 223 - 2 + 2 * inside.len());
}

/// A rolling surface: each version's stated error is the farthest any vertex of the chunk lies from it, found again by
/// brute force over every triangle; versions get coarser as they go, each at most 0.8 of the one before.
#[test]
fn each_versions_error_is_the_farthest_a_vertex_lies_from_it() {
    let (positions, indices) = lattice(0, 60, 60, rolling);
    let made = display_levels(&positions, &indices, None);
    assert!(made.levels.len() >= 3, "{} versions", made.levels.len());
    let at = |v: u32| positions[v as usize].map(|c| c as f64);
    let mut before = indices.len() / 3;
    let mut error = 0.0;
    for level in &made.levels {
        let count = level.indices.len() / 3;
        assert!(count as f64 <= 0.8 * before as f64 && count >= FEWEST_TRIANGLES, "{count} after {before}");
        let brute = positions
            .iter()
            .map(|p| {
                let p = p.map(|c| c as f64);
                level.indices.as_chunks::<3>().0.iter().map(|t| to_triangle(p, at(t[0]), at(t[1]), at(t[2]))).fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max);
        assert!((level.error - brute).abs() <= 1e-9, "stated {} found {brute}", level.error);
        assert!(level.error >= error - 1e-9, "coarser but nearer: {} after {error}", level.error);
        (before, error) = (count, level.error);
    }
}

/// Two chunks side by side, sharing column 100: every version of each keeps every edge of the full chunk along that
/// column, one a metre, so drawn at any versions they meet with no crack. (A version may add a sliver standing on three
/// of them, which closes nothing and opens nothing.)
#[test]
fn chunks_side_by_side_meet_at_every_version() {
    let seam = 100.0_f32;
    for (first, cols) in [(0, 101), (100, 101)] {
        let (positions, indices) = lattice(first, cols, 120, rolling);
        let along = |idx: &[u32]| -> HashSet<(u32, u32)> {
            let mut edges = HashSet::new();
            for t in idx.as_chunks::<3>().0 {
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    if positions[a as usize][0] == seam && positions[b as usize][0] == seam {
                        edges.insert((a.min(b), a.max(b)));
                    }
                }
            }
            edges
        };
        let full = along(&indices);
        assert_eq!(full.len(), 119);
        let made = display_levels(&positions, &indices, None);
        assert!(made.levels.len() >= 3);
        for level in &made.levels {
            let kept = along(&level.indices);
            assert!(full.is_subset(&kept), "the chunk from column {first} lost {} of the seam's edges", full.difference(&kept).count());
        }
    }
}

/// Vertices named locked (a block's edge across its top) are in every version.
#[test]
fn locked_vertices_stay() {
    let (positions, indices) = lattice(0, 80, 80, rolling);
    let locked: Vec<bool> = positions.iter().map(|p| p[0] == p[1]).collect();
    let made = display_levels(&positions, &indices, Some(&locked));
    assert!(made.levels.len() >= 2);
    for level in &made.levels {
        let used: HashSet<u32> = level.indices.iter().copied().collect();
        assert!((0..positions.len() as u32).filter(|&v| locked[v as usize]).all(|v| used.contains(&v)));
    }
}

/// Timing, release build: lidar-like ground (a 1 m lattice, rolling, with up to 30 cm of roughness) in chunks of about
/// 100,000 triangles, as the shell bins them, all on their own threads.
#[test]
#[ignore]
fn timing_display_levels() {
    let rough = |x: f64, y: f64| {
        let h = ((x * 12.9898 + y * 78.233).sin() * 43758.5453).fract();
        rolling(x, y) + 0.3 * (h - 0.5)
    };
    let chunks: Vec<(Vec<[f32; 3]>, Vec<u32>)> = (0..40).map(|k| lattice(k * 223, 224, 224, rough)).collect();
    let triangles: usize = chunks.iter().map(|c| c.1.len() / 3).sum();
    let start = Instant::now();
    let made: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = chunks.iter().map(|(p, i)| s.spawn(move || display_levels(p, i, None))).collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let wall = start.elapsed().as_secs_f64() * 1000.0;
    let one = &made[0];
    let counts: Vec<String> = one.levels.iter().map(|l| format!("{} ({:.3} m)", l.indices.len() / 3, l.error)).collect();
    let mean = made.iter().map(|m| m.ms).sum::<f64>() / made.len() as f64;
    println!("{} chunks, {triangles} triangles: {wall:.0} ms on all cores, {mean:.0} ms a chunk on its own thread", made.len());
    println!("one chunk's versions: {}", counts.join(", "));
}
