//! A face laid on the grid it was built to (decision 0064), worked by hand at mine grid coordinates.

use zd_core::{Grid, TriMesh};
use zd_geom::{boundary_loops, drape_on_grids, triangulate_grid};

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// A grid of 10 m cells from (E - 50, N - 50), 21 by 11 nodes, at RL 100 + `rise(x)` with x east of E.
fn grid(rise: impl Fn(f64) -> f64) -> TriMesh {
    triangulate_grid(&Grid::from_fn([E - 50.0, N - 50.0], [10.0, 10.0], 21, 11, |x, _| 100.0 + rise(x - E)))
}

/// A band 40 m east and 10 m north from (E, N), two triangles, its corners on grid nodes at the grid's heights.
fn band(rise: impl Fn(f64) -> f64) -> TriMesh {
    let at = |x: f64, y: f64| [E + x, N + y, 100.0 + rise(x)];
    TriMesh::new(vec![at(0.0, 0.0), at(40.0, 0.0), at(40.0, 10.0), at(0.0, 10.0)], vec![[0, 1, 2], [0, 2, 3]])
}

/// The height of the grid's own triangles (cells split south-west to north-east) at a plan point.
fn on_grid(rise: &impl Fn(f64) -> f64, x: f64, y: f64) -> f64 {
    let (u, v) = ((x - E + 50.0) / 10.0, (y - N + 50.0) / 10.0);
    let (c, r) = (u.floor().min(19.0), v.floor().min(9.0));
    let (s, t) = (u - c, v - r);
    let z = |cc: f64| 100.0 + rise(-50.0 + 10.0 * cc);
    let (sw, se) = (z(c), z(c + 1.0));
    // Heights vary only east-west, so the north-east corner is the south-east one's and the north-west the south-west's.
    if s >= t { sw + s * (se - sw) } else { sw + t * 0.0 + s * (se - sw) }
}

/// RL 100 + 0.0005 x² is bent at every grid line. The band's straight edges and diagonal span four cells, so between
/// its corners it's a chord, furthest off the grid at x = 20 m: 0.0005 × (0 + 1600) / 2 - 0.0005 × 400 = 0.2 m.
/// Draped, every piece lies in one grid triangle on its plane, the band is one sheet with the same area in plan, and it
/// moved 0.2 m at most.
#[test]
fn a_band_across_a_bent_grid_is_laid_on_it() {
    let rise = |x: f64| 0.0005 * x * x;
    let (draped, log) = drape_on_grids(&band(rise), &[&grid(rise)]);
    assert_eq!(log.draped, 2, "{log:?}");
    assert!((log.furthest - 0.2).abs() < 1e-9, "{log:?}");
    assert_eq!((log.left, log.too_far), (0, 0));
    for t in &draped.triangles {
        let p = t.map(|i| draped.vertices[i as usize]);
        let middle = [0, 1, 2].map(|k| (p[0][k] + p[1][k] + p[2][k]) / 3.0);
        assert!((middle[2] - on_grid(&rise, middle[0], middle[1])).abs() < 1e-9, "a piece off the grid at {middle:?}");
        for v in p {
            assert!((v[2] - on_grid(&rise, v[0], v[1])).abs() < 1e-9, "a corner off the grid at {v:?}");
        }
    }
    let plan: f64 = draped.triangles.iter().map(|t| {
        let [a, b, c] = t.map(|i| draped.vertices[i as usize]);
        0.5 * ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]))
    }).sum();
    assert!((plan - 400.0).abs() < 1e-6, "{plan}");
    assert_eq!(boundary_loops(&draped).unwrap().len(), 1, "one sheet");
}

/// On a grid that's one plane the band already lies on it: nothing changes.
#[test]
fn a_band_already_on_its_grid_is_left_as_it_is() {
    let rise = |x: f64| 0.01 * x;
    let before = band(rise);
    let (draped, log) = drape_on_grids(&before, &[&grid(rise)]);
    assert_eq!(log.draped, 0, "{log:?}");
    assert_eq!(draped.triangles.len(), before.triangles.len());
}

/// RL 100 + 0.002 x²: the chord is 0.8 m off at x = 20 m, more than half a metre, so the band isn't following the grid
/// and is left as it is.
#[test]
fn a_band_far_off_its_grid_is_left_and_counted() {
    let rise = |x: f64| 0.002 * x * x;
    let (draped, log) = drape_on_grids(&band(rise), &[&grid(rise)]);
    assert_eq!((log.draped, log.too_far), (0, 2), "{log:?}");
    assert_eq!(draped.triangles.len(), 2);
}
