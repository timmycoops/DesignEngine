//! The snap index on a big surface (decision 0044, 21 November): how long a 1 m lidar's worth of triangles takes to
//! index, and that a ray hits it where it should. Run the timing with `--ignored`.

use std::time::Instant;

use zd_core::TriMesh;
use zd_geom::SurfaceRayCaster;

/// A tilted plane z = 0.1 x over `n` by `n` cells of 1 m, two triangles a cell.
fn tilted_plane(n: u32) -> TriMesh {
    let side = n + 1;
    let vertices = (0..side).flat_map(|j| (0..side).map(move |i| [i as f64, j as f64, 0.1 * i as f64])).collect();
    let triangles = (0..n)
        .flat_map(|j| {
            (0..n).flat_map(move |i| {
                let a = j * side + i;
                [[a, a + 1, a + side + 1], [a, a + side + 1, a + side]]
            })
        })
        .collect();
    TriMesh::new(vertices, triangles)
}

/// A ray straight down onto the plane z = 0.1 x lands at z = 0.1 x: within 1e-9 m.
#[test]
fn a_ray_down_lands_on_the_plane() {
    let caster = SurfaceRayCaster::new(&tilted_plane(100)).unwrap();
    let hit = caster.cast([37.25, 61.5, 500.0], [0.0, 0.0, -1.0]).unwrap();
    assert!((hit[2] - 3.725).abs() < 1e-9 && (hit[0] - 37.25).abs() < 1e-9, "{hit:?}");
}

/// A surface indexed in pieces gives the same hits as one index, triangle for triangle: a wavy 120 by 120 m surface of
/// 28,800 triangles in pieces of about 1,000 (36 pieces), 2,000 oblique rays from above and below. Hits agree within
/// 1e-9 m and name the same triangle; so do casts that pass over every odd triangle.
#[test]
fn pieces_hit_what_one_index_hits() {
    let mut surface = tilted_plane(120);
    for p in &mut surface.vertices {
        p[2] += 3.0 * (p[0] / 7.0).sin() * (p[1] / 11.0).cos();
    }
    let whole = SurfaceRayCaster::in_pieces(&surface, usize::MAX).unwrap();
    let pieces = SurfaceRayCaster::in_pieces(&surface, 1_000).unwrap();
    let mut seed = 7_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 11) as f64 / (1_u64 << 53) as f64
    };
    let mut hits = 0;
    for k in 0..2_000 {
        let up = k % 2 == 0;
        let from = [next() * 130.0 - 5.0, next() * 130.0 - 5.0, if up { -30.0 } else { 50.0 }];
        let direction = [next() * 0.6 - 0.3, next() * 0.6 - 0.3, if up { 1.0 } else { -1.0 }];
        for keep in [&(|_: u32| true) as &dyn Fn(u32) -> bool, &|t: u32| t % 2 == 0] {
            let (a, b) = (whole.cast_where(from, direction, keep), pieces.cast_where(from, direction, keep));
            match (a, b) {
                (None, None) => {}
                (Some((ta, pa)), Some((tb, pb))) => {
                    hits += 1;
                    assert_eq!(ta, tb, "ray {k}");
                    assert!((0..3).all(|i| (pa[i] - pb[i]).abs() < 1e-9), "ray {k}: {pa:?} against {pb:?}");
                }
                _ => panic!("ray {k}: {a:?} against {b:?}"),
            }
        }
    }
    assert!(hits > 1_000, "most rays hit: {hits}");
}

/// Large mine scale: about 85 M triangles, as the 1 m lidar.
#[test]
#[ignore]
fn timing_index_a_lidar() {
    let n = 6_520;
    let surface = tilted_plane(n);
    for (what, most) in [("one index", usize::MAX), ("in pieces", zd_geom::PIECE_TRIANGLES)] {
        let t = Instant::now();
        let caster = SurfaceRayCaster::in_pieces(&surface, most).unwrap();
        let built = t.elapsed().as_secs_f64();
        let t = Instant::now();
        for k in 0..1_000 {
            let x = (k * 6_517 % (n as usize / 2)) as f64 + 0.5;
            assert!(caster.cast([x, x * 0.7, 1e3], [0.1, 0.05, -1.0]).is_some(), "{x}");
        }
        println!("snap index {what}: {} triangles in {built:.1} s, a ray {:.3} ms", surface.triangles.len(), t.elapsed().as_secs_f64());
    }
}
