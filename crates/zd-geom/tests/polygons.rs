//! Polygons from lines (decision 0062): closed areas of lines in plan, with answers worked by hand. Areas to 0.001 m².

use zd_core::Polyline;
use zd_geom::polygons_from_lines;

const E: f64 = 650_000.0;
const N: f64 = 7_450_000.0;

/// A line at mine grid coordinates, `x` and `y` metres from the origin, at RL 10.
fn line(points: &[(f64, f64)]) -> Polyline {
    Polyline { vertices: points.iter().map(|&(x, y)| [E + x, N + y, 10.0]).collect(), closed: false }
}

/// Three lines across and three up, 100 m apart, each running `past` metres beyond the outer ones.
fn two_by_two(past: f64) -> Vec<Polyline> {
    let mut lines = Vec::new();
    for k in 0..3 {
        let at = 100.0 * k as f64;
        lines.push(line(&[(-past, at), (200.0 + past, at)]));
        lines.push(line(&[(at, -past), (at, 200.0 + past)]));
    }
    lines
}

fn areas(made: &zd_geom::LinePolygons) -> Vec<f64> {
    made.polygons.iter().map(|p| p.area).collect()
}

#[test]
fn a_two_by_two_grid_of_lines_closes_four_squares() {
    let made = polygons_from_lines(&two_by_two(0.0), 0.001).unwrap();
    assert_eq!(made.polygons.len(), 4);
    for area in areas(&made) {
        assert!((area - 10_000.0).abs() < 0.001, "{area}");
    }
    assert!(made.short.is_empty() && made.unused.is_empty() && made.joined.is_empty() && made.islands.is_empty());
    assert_eq!(made.run_past, 0);
    // West to east, then south to north.
    let centres: Vec<[f64; 2]> = made.polygons.iter().map(|p| [p.centroid[0] - E, p.centroid[1] - N]).collect();
    for (got, want) in centres.iter().zip([[50.0, 50.0], [50.0, 150.0], [150.0, 50.0], [150.0, 150.0]]) {
        assert!((got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6, "{got:?}");
    }
    // Anticlockwise, at the lines' height.
    for polygon in &made.polygons {
        assert!(polygon.vertices.iter().all(|v| v[2] == 10.0));
        let n = polygon.vertices.len();
        let twice: f64 = (0..n).map(|k| {
            let (a, b) = (polygon.vertices[k], polygon.vertices[(k + 1) % n]);
            (a[0] - E) * (b[1] - N) - (b[0] - E) * (a[1] - N)
        }).sum();
        assert!(twice > 0.0);
    }
}

#[test]
fn lines_running_past_close_the_same_squares_and_are_counted() {
    let made = polygons_from_lines(&two_by_two(20.0), 0.001).unwrap();
    assert_eq!(made.polygons.len(), 4);
    assert!(areas(&made).iter().all(|a| (a - 10_000.0).abs() < 0.001));
    assert_eq!(made.run_past, 12, "both ends of all six lines");
    assert!((made.run_past_length - 240.0).abs() < 1e-6);
    assert!(made.short.is_empty());
}

#[test]
fn an_end_five_millimetres_short_is_joined_at_a_centimetre_and_listed_at_a_millimetre() {
    let mut lines = two_by_two(0.0);
    // The middle line up stops 5 mm below the top line.
    lines[3] = line(&[(100.0, 0.0), (100.0, 199.995)]);
    let joined = polygons_from_lines(&lines, 0.01).unwrap();
    assert_eq!(joined.polygons.len(), 4);
    assert_eq!(joined.joined.len(), 1);
    assert!((joined.joined[0].gap - 0.005).abs() < 1e-9);
    assert!(joined.short.is_empty());

    let open = polygons_from_lines(&lines, 0.001).unwrap();
    let mut got = areas(&open);
    got.sort_by(f64::total_cmp);
    assert_eq!(got.len(), 3, "the top two squares are one");
    for (area, want) in got.iter().zip([10_000.0, 10_000.0, 20_000.0]) {
        assert!((area - want).abs() < 0.001, "{area}");
    }
    assert_eq!(open.short.len(), 1);
    let end = &open.short[0];
    assert!((end.at[0] - E - 100.0).abs() < 1e-9 && (end.at[1] - N - 199.995).abs() < 1e-6);
    assert!((end.gap - 0.005).abs() < 1e-6);
    assert!((open.polygons[end.inside].area - 20_000.0).abs() < 0.001);
}

#[test]
fn a_line_crossing_nothing_is_unused() {
    let mut lines = two_by_two(0.0);
    lines.push(line(&[(500.0, 0.0), (500.0, 200.0)]));
    let made = polygons_from_lines(&lines, 0.001).unwrap();
    assert_eq!(made.polygons.len(), 4);
    assert_eq!(made.unused, vec![6]);
    assert_eq!(made.run_past, 2);
}

#[test]
fn a_closed_string_split_by_a_line_across_it() {
    let square = Polyline { vertices: [(0.0, 0.0), (200.0, 0.0), (200.0, 200.0), (0.0, 200.0)].iter().map(|&(x, y)| [E + x, N + y, 0.0]).collect(), closed: true };
    let across = line(&[(100.0, -50.0), (100.0, 250.0)]);
    let made = polygons_from_lines(&[square, across], 0.001).unwrap();
    assert_eq!(made.polygons.len(), 2);
    assert!(areas(&made).iter().all(|a| (a - 20_000.0).abs() < 0.001));
    assert_eq!(made.run_past, 2);
}

#[test]
fn a_closing_line_drawn_along_another_adds_nothing() {
    let mut lines = two_by_two(0.0);
    // Along the bottom line from 50 m to 150 m.
    lines.push(line(&[(50.0, 0.0), (150.0, 0.0)]));
    let made = polygons_from_lines(&lines, 0.001).unwrap();
    assert_eq!(made.polygons.len(), 4);
    assert!(areas(&made).iter().all(|a| (a - 10_000.0).abs() < 0.001));
}

#[test]
fn a_ring_inside_a_polygon_not_touching_it_is_an_island() {
    let ring = |x0: f64, size: f64| Polyline {
        vertices: [(x0, x0), (x0 + size, x0), (x0 + size, x0 + size), (x0, x0 + size)].iter().map(|&(x, y)| [E + x, N + y, 0.0]).collect(),
        closed: true,
    };
    let made = polygons_from_lines(&[ring(0.0, 300.0), ring(100.0, 50.0)], 0.001).unwrap();
    assert_eq!(made.polygons.len(), 2);
    assert_eq!(made.islands.len(), 1);
    assert!((made.polygons[made.islands[0].1].area - 90_000.0).abs() < 0.001);
}

/// Tim's layout: two bent closing lines along strike with 2,000 vertices each, and 15 lines across them every 100 m
/// running 50 m past. The closing lines run 150 m past the first and last lines across, so those ends close nothing:
/// 14 blocks, between neighbouring lines across.
#[test]
fn block_lines_every_100_m_between_two_bent_lines() {
    let bent = |y0: f64| -> Polyline {
        line(&(0..2000).map(|k| {
            let x = -100.0 + 1700.0 * k as f64 / 1999.0;
            (x, y0 + 8.0 * (x / 230.0).sin())
        }).collect::<Vec<_>>())
    };
    let mut lines = vec![bent(0.0), bent(360.0)];
    for k in 0..15 {
        let x = 50.0 + 100.0 * k as f64;
        lines.push(line(&[(x, -50.0), (x, 410.0)]));
    }
    let started = std::time::Instant::now();
    let made = polygons_from_lines(&lines, 0.001).unwrap();
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    println!("{} segments in, {} crossings, {} polygons, {ms:.1} ms", made.segments, made.crossings, made.polygons.len());
    assert_eq!(made.polygons.len(), 14);
    // The bends are the same on both lines, so each block is 100 m by 360 m.
    for polygon in &made.polygons {
        assert!((polygon.area - 36_000.0).abs() < 0.01, "{}", polygon.area);
    }
    assert!(made.short.is_empty() && made.unused.is_empty());
    assert_eq!(made.run_past, 34, "both ends of the 15 lines across, and both of the two bent lines");
    assert!(ms < 500.0);
}
