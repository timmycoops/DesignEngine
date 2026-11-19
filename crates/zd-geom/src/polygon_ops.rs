//! Polygons in plan (String tools plan, workstream 6; decision 0083): the union, overlap or difference of two sets of
//! closed strings, as closed strings. The areas are worked on Manifold's CrossSection about a local origin, so mine grid
//! eastings and northings keep their digits; every point that comes back is then put back on the strings it came from.
//! A vertex that was already there comes back exactly, with its height; a new one, where two edges cross, lies on the
//! first set's edge where it can, and takes the height along it. Heights never change the areas, which are in plan.

use std::collections::HashMap;
use std::time::Instant;

use manifold_csg::CrossSection;
use zd_core::Polyline;

use crate::GeomError;

/// A point from the area work this close in plan to a vertex or an edge of the strings it came from is on it (m). The
/// area work rounds its points far finer than this.
pub const ON_A_POLYGON: f64 = 1e-6;

/// What is made of the two sets of closed strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolygonOp {
    /// Everywhere inside either set.
    Union,
    /// Everywhere inside both.
    Overlap,
    /// Everywhere inside the first set and not the second.
    Difference,
}

/// One closed string made: which part it belongs to (largest first) and whether it's a hole in that part.
#[derive(Debug, Clone, PartialEq)]
pub struct PolygonRing {
    pub line: Polyline,
    pub part: usize,
    pub hole: bool,
}

/// The closed strings made, outsides anticlockwise and holes clockwise, each part's outside before its holes; their
/// area in plan; how many vertices came back as they were, how many are new, and how many lay on no string's edge and
/// took the height of the nearest (none, where the area work is sound); and the time taken.
#[derive(Debug, Clone, PartialEq)]
pub struct PolygonBoolean {
    pub rings: Vec<PolygonRing>,
    pub parts: usize,
    pub holes: usize,
    pub area: f64,
    pub kept: usize,
    pub new: usize,
    pub off: usize,
    pub ms: f64,
}

/// The union, overlap or difference of two sets of closed strings in plan. Each set is the area inside any of its
/// strings. `second` may be empty for a union, which then joins the first set's strings that touch or overlap into one.
/// Refused if a string isn't closed, has fewer than three vertices or closes round no area. An overlap or difference
/// that leaves nothing comes back with no strings.
pub fn polygon_boolean(first: &[Polyline], second: &[Polyline], op: PolygonOp) -> Result<PolygonBoolean, GeomError> {
    let start = Instant::now();
    if first.is_empty() {
        return Err(GeomError::InvalidInput("no strings in the first set".into()));
    }
    if second.is_empty() && op != PolygonOp::Union {
        return Err(GeomError::InvalidInput("an overlap or a difference needs a second set of strings".into()));
    }
    for (k, ring) in first.iter().chain(second).enumerate() {
        if !ring.closed || ring.vertices.len() < 3 {
            return Err(GeomError::InvalidInput(format!("string {} isn't a closed string of three vertices or more", k + 1)));
        }
        if ring_area(&plan_ring(ring, [0.0, 0.0])) == 0.0 {
            return Err(GeomError::InvalidInput(format!("string {} closes round no area in plan", k + 1)));
        }
    }
    let v0 = first[0].vertices[0];
    let origin = [v0[0].round(), v0[1].round()];
    let area_of = |set: &[Polyline]| -> CrossSection {
        let sections: Vec<CrossSection> = set.iter().map(|r| section(r, origin)).collect();
        CrossSection::batch_union(&sections)
    };
    let a = area_of(first);
    let result = match op {
        PolygonOp::Union if second.is_empty() => a,
        PolygonOp::Union => a.union(&area_of(second)),
        PolygonOp::Overlap => a.intersection(&area_of(second)),
        PolygonOp::Difference => a.difference(&area_of(second)),
    };

    let sources: Vec<&Polyline> = first.iter().chain(second).collect();
    let index = SourceIndex::new(&sources);
    let mut parts: Vec<CrossSection> = result.decompose();
    parts.sort_by(|p, q| q.area().total_cmp(&p.area()));
    let mut made = PolygonBoolean { rings: Vec::new(), parts: parts.len(), holes: 0, area: 0.0, kept: 0, new: 0, off: 0, ms: 0.0 };
    for (k, part) in parts.iter().enumerate() {
        made.area += part.area();
        let mut rings = part.to_polygons();
        // The outside first, then the holes.
        rings.sort_by(|p, q| ring_area(q).total_cmp(&ring_area(p)));
        for ring in rings {
            let hole = ring_area(&ring) < 0.0;
            let world: Vec<[f64; 2]> = ring.iter().map(|p| [p[0] + origin[0], p[1] + origin[1]]).collect();
            let line = index.put_back(&world, &mut made);
            if line.vertices.len() < 3 {
                continue;
            }
            made.holes += hole as usize;
            made.rings.push(PolygonRing { line: Polyline { vertices: line.vertices, closed: true }, part: k, hole });
        }
    }
    made.ms = start.elapsed().as_secs_f64() * 1e3;
    Ok(made)
}

/// A closed string's plan ring about an origin.
fn plan_ring(ring: &Polyline, origin: [f64; 2]) -> Vec<[f64; 2]> {
    ring.vertices.iter().map(|p| [p[0] - origin[0], p[1] - origin[1]]).collect()
}

/// A closed string's area in plan about an origin, turned anticlockwise so it counts as inside.
fn section(ring: &Polyline, origin: [f64; 2]) -> CrossSection {
    let mut ring = plan_ring(ring, origin);
    if ring_area(&ring) < 0.0 {
        ring.reverse();
    }
    CrossSection::from_polygons(&[ring])
}

/// A ring's signed area (anticlockwise positive).
fn ring_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    let o = ring.first().copied().unwrap_or([0.0, 0.0]);
    (0..n)
        .map(|k| {
            let (p, q) = (ring[k], ring[(k + 1) % n]);
            (p[0] - o[0]) * (q[1] - o[1]) - (q[0] - o[0]) * (p[1] - o[1])
        })
        .sum::<f64>()
        / 2.0
}

fn distance2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

/// The place on segment `a`–`b` nearest `p` in plan, as the fraction along it.
fn along(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let span = dx * dx + dy * dy;
    if span == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / span).clamp(0.0, 1.0) }
}

fn plan(p: [f64; 3]) -> [f64; 2] {
    [p[0], p[1]]
}

/// The strings' vertices and edges in bins in plan, to put each point from the area work back on what it came from. The
/// strings are in order, the first set's first, and an earlier string wins a tie.
struct SourceIndex<'a> {
    sources: &'a [&'a Polyline],
    lo: [f64; 2],
    cell: f64,
    /// Each bin's edges, as (string, vertex the edge starts at), in order.
    bins: HashMap<(i64, i64), Vec<(u32, u32)>>,
}

impl<'a> SourceIndex<'a> {
    fn new(sources: &'a [&'a Polyline]) -> Self {
        let (mut lo, mut hi, mut edges) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2], 0usize);
        for ring in sources {
            edges += ring.vertices.len();
            for p in &ring.vertices {
                lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                hi = [hi[0].max(p[0]), hi[1].max(p[1])];
            }
        }
        let extent = (hi[0] - lo[0]).max(hi[1] - lo[1]);
        let cell = (extent / (edges as f64).sqrt().max(1.0)).max(1.0);
        let mut index = SourceIndex { sources, lo, cell, bins: HashMap::new() };
        for (r, ring) in sources.iter().enumerate() {
            let n = ring.vertices.len();
            for k in 0..n {
                let (a, b) = (plan(ring.vertices[k]), plan(ring.vertices[(k + 1) % n]));
                for key in index.cells([a[0].min(b[0]), a[1].min(b[1])], [a[0].max(b[0]), a[1].max(b[1])]) {
                    index.bins.entry(key).or_default().push((r as u32, k as u32));
                }
            }
        }
        index
    }

    /// The bins a box in plan covers, widened by the tolerance.
    fn cells(&self, lo: [f64; 2], hi: [f64; 2]) -> impl Iterator<Item = (i64, i64)> + use<> {
        let at = |v: f64, o: f64| ((v - o) / self.cell).floor() as i64;
        let (i0, i1) = (at(lo[0] - ON_A_POLYGON, self.lo[0]), at(hi[0] + ON_A_POLYGON, self.lo[0]));
        let (j0, j1) = (at(lo[1] - ON_A_POLYGON, self.lo[1]), at(hi[1] + ON_A_POLYGON, self.lo[1]));
        (i0..=i1).flat_map(move |i| (j0..=j1).map(move |j| (i, j)))
    }

    /// The edges near a box in plan, in the strings' order, each once.
    fn edges_near(&self, lo: [f64; 2], hi: [f64; 2]) -> Vec<(u32, u32)> {
        let mut found: Vec<(u32, u32)> = self.cells(lo, hi).filter_map(|key| self.bins.get(&key)).flatten().copied().collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    fn edge(&self, (r, k): (u32, u32)) -> ([f64; 3], [f64; 3]) {
        let ring = self.sources[r as usize];
        let n = ring.vertices.len();
        (ring.vertices[k as usize], ring.vertices[(k as usize + 1) % n])
    }

    /// A point from the area work back on the strings: a vertex there exactly, or the place on the first edge it lies
    /// on at the height along it, or failing both the nearest edge's (counted as off). Whether it was a vertex.
    fn place(&self, p: [f64; 2], made: &mut PolygonBoolean) -> ([f64; 3], bool) {
        let near = self.edges_near(p, p);
        let tol2 = ON_A_POLYGON * ON_A_POLYGON;
        // A vertex first: the earliest string's, then the nearest.
        let mut vertex: Option<(u32, f64, [f64; 3])> = None;
        for &e in &near {
            let (a, _) = self.edge(e);
            let d = distance2(plan(a), p);
            if d <= tol2 && vertex.is_none_or(|(r, best, _)| e.0 < r || (e.0 == r && d < best)) {
                vertex = Some((e.0, d, a));
            }
        }
        if let Some((_, _, v)) = vertex {
            made.kept += 1;
            return (v, true);
        }
        let on_edge = |e: (u32, u32)| {
            let (a, b) = self.edge(e);
            let t = along(plan(a), plan(b), p);
            let q = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] + t * (b[2] - a[2])];
            (distance2(plan(q), p), q)
        };
        let mut edge: Option<(u32, f64, [f64; 3])> = None;
        for &e in &near {
            let (d, q) = on_edge(e);
            if d <= tol2 && edge.is_none_or(|(r, best, _)| e.0 < r || (e.0 == r && d < best)) {
                edge = Some((e.0, d, q));
            }
        }
        if let Some((_, _, q)) = edge {
            made.new += 1;
            return (q, false);
        }
        // On no edge: the nearest of them all, its height taken at the point itself.
        made.off += 1;
        let mut best: Option<(f64, [f64; 3])> = None;
        for (r, ring) in self.sources.iter().enumerate() {
            for k in 0..ring.vertices.len() {
                let (d, q) = on_edge((r as u32, k as u32));
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, q));
                }
            }
        }
        let q = best.map_or([p[0], p[1], 0.0], |(_, q)| q);
        ([p[0], p[1], q[2]], false)
    }

    /// The vertices of the strings lying on a segment in plan between its ends, in order along it, each place once.
    /// The area work may leave out a vertex where its string runs straight on; this puts it back, with its height.
    fn on_segment(&self, a: [f64; 3], b: [f64; 3]) -> Vec<[f64; 3]> {
        let (pa, pb) = (plan(a), plan(b));
        let tol2 = ON_A_POLYGON * ON_A_POLYGON;
        let mut found: Vec<(f64, [f64; 3])> = Vec::new();
        for e in self.edges_near([pa[0].min(pb[0]), pa[1].min(pb[1])], [pa[0].max(pb[0]), pa[1].max(pb[1])]) {
            let (v, _) = self.edge(e);
            let pv = plan(v);
            if distance2(pv, pa) <= tol2 || distance2(pv, pb) <= tol2 {
                continue;
            }
            let t = along(pa, pb, pv);
            if t <= 0.0 || t >= 1.0 {
                continue;
            }
            let foot = [pa[0] + t * (pb[0] - pa[0]), pa[1] + t * (pb[1] - pa[1])];
            if distance2(foot, pv) <= tol2 && !found.iter().any(|(_, q)| distance2(plan(*q), pv) <= tol2) {
                found.push((t, v));
            }
        }
        found.sort_by(|x, y| x.0.total_cmp(&y.0));
        found.into_iter().map(|(_, v)| v).collect()
    }

    /// A ring from the area work put back on the strings, with any vertex it left out put back too.
    fn put_back(&self, ring: &[[f64; 2]], made: &mut PolygonBoolean) -> Polyline {
        let placed: Vec<[f64; 3]> = ring.iter().map(|&p| self.place(p, made).0).collect();
        let n = placed.len();
        let mut vertices: Vec<[f64; 3]> = Vec::with_capacity(n);
        for k in 0..n {
            let (a, b) = (placed[k], placed[(k + 1) % n]);
            if vertices.last().is_none_or(|l| distance2(plan(*l), plan(a)) > ON_A_POLYGON * ON_A_POLYGON) {
                vertices.push(a);
            }
            let between = self.on_segment(a, b);
            made.kept += between.len();
            vertices.extend(between);
        }
        while vertices.len() > 1 && distance2(plan(vertices[0]), plan(*vertices.last().expect("a vertex"))) <= ON_A_POLYGON * ON_A_POLYGON {
            vertices.pop();
        }
        Polyline { vertices, closed: true }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A rectangle from `x0`, `y0` to `x1`, `y1` m off (E, N), anticlockwise, each corner at the RL `rl` gives it.
    fn rect(x0: f64, y0: f64, x1: f64, y1: f64, rl: impl Fn(f64, f64) -> f64) -> Polyline {
        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        Polyline { vertices: corners.iter().map(|&[x, y]| [E + x, N + y, rl(x, y)]).collect(), closed: true }
    }

    fn area(l: &Polyline) -> f64 {
        ring_area(&plan_ring(l, [E, N]))
    }

    fn has(l: &Polyline, x: f64, y: f64) -> Option<[f64; 3]> {
        l.vertices.iter().copied().find(|p| (p[0] - (E + x)).abs() < 1e-6 && (p[1] - (N + y)).abs() < 1e-6)
    }

    #[test]
    fn two_100_m_squares_overlapping_by_half() {
        // Union 15,000 m², overlap 5,000 m², difference 5,000 m², each one closed string at RL 100.
        let a = rect(0.0, 0.0, 100.0, 100.0, |_, _| 100.0);
        let b = rect(50.0, 0.0, 150.0, 100.0, |_, _| 100.0);
        for (op, expected) in [(PolygonOp::Union, 15_000.0), (PolygonOp::Overlap, 5_000.0), (PolygonOp::Difference, 5_000.0)] {
            let made = polygon_boolean(std::slice::from_ref(&a), std::slice::from_ref(&b), op).unwrap();
            assert_eq!((made.rings.len(), made.parts, made.holes, made.off), (1, 1, 0, 0), "{op:?}");
            assert!((made.area - expected).abs() < 1e-6, "{op:?}: {}", made.area);
            assert!((area(&made.rings[0].line) - expected).abs() < 1e-6, "{op:?}: the string's own area");
            assert!(made.rings[0].line.vertices.iter().all(|p| p[2] == 100.0), "{op:?}");
        }
    }

    #[test]
    fn vertices_keep_their_heights_and_new_ones_take_the_first_sets() {
        // A 100 m square graded 10 % north from RL 100, and a block at RL 200 from 50 m to 150 m east, 20 m to 80 m north.
        // Their overlap: the block's two corners inside the square kept exactly at RL 200; the two new corners on the
        // square's east edge at RL 102 and 108, along it.
        let a = rect(0.0, 0.0, 100.0, 100.0, |_, y| 100.0 + 0.1 * y);
        let b = rect(50.0, 20.0, 150.0, 80.0, |_, _| 200.0);
        let made = polygon_boolean(std::slice::from_ref(&a), std::slice::from_ref(&b), PolygonOp::Overlap).unwrap();
        assert_eq!(made.rings.len(), 1);
        let line = &made.rings[0].line;
        assert_eq!(line.vertices.len(), 4);
        assert!((area(line) - 3_000.0).abs() < 1e-6);
        assert_eq!(has(line, 50.0, 20.0), Some(b.vertices[0]), "a vertex kept exactly");
        assert_eq!(has(line, 50.0, 80.0), Some(b.vertices[3]));
        let (p, q) = (has(line, 100.0, 20.0).unwrap(), has(line, 100.0, 80.0).unwrap());
        assert!((p[2] - 102.0).abs() < 1e-6 && (q[2] - 108.0).abs() < 1e-6, "{p:?} {q:?}");
        assert!((p[0] - (E + 100.0)).abs() < 1e-9, "on the square's edge: {}", p[0] - E);
        assert_eq!((made.kept, made.new, made.off), (2, 2, 0));

        // The square less the block: 7,000 m², its four corners kept and the notch's two new corners as above.
        let made = polygon_boolean(std::slice::from_ref(&a), std::slice::from_ref(&b), PolygonOp::Difference).unwrap();
        let line = &made.rings[0].line;
        assert!((area(line) - 7_000.0).abs() < 1e-6);
        assert_eq!(line.vertices.len(), 8);
        for v in &a.vertices {
            assert!(line.vertices.contains(v), "{v:?}");
        }
        assert!((has(line, 100.0, 20.0).unwrap()[2] - 102.0).abs() < 1e-6);

        // Together: 13,000 m², the block's far corners kept at RL 200.
        let made = polygon_boolean(std::slice::from_ref(&a), std::slice::from_ref(&b), PolygonOp::Union).unwrap();
        let line = &made.rings[0].line;
        assert!((area(line) - 13_000.0).abs() < 1e-6);
        assert_eq!(has(line, 150.0, 20.0), Some(b.vertices[1]));
        assert!((has(line, 100.0, 80.0).unwrap()[2] - 108.0).abs() < 1e-6);
    }

    #[test]
    fn a_hole_and_parts_apart() {
        // A 100 m square less a 20 m square inside it: one part, its outside and one hole, 10,000 and 400 m².
        let outer = rect(0.0, 0.0, 100.0, 100.0, |_, _| 100.0);
        let inner = rect(40.0, 40.0, 60.0, 60.0, |_, _| 100.0);
        let made = polygon_boolean(std::slice::from_ref(&outer), std::slice::from_ref(&inner), PolygonOp::Difference).unwrap();
        assert_eq!((made.parts, made.rings.len(), made.holes), (1, 2, 1));
        assert!(!made.rings[0].hole && made.rings[1].hole);
        assert!((area(&made.rings[0].line) - 10_000.0).abs() < 1e-6);
        assert!((area(&made.rings[1].line) + 400.0).abs() < 1e-6, "a hole runs clockwise");
        assert!((made.area - 9_600.0).abs() < 1e-6);

        // Two squares apart, and a third touching the first along an edge, joined: two parts, the larger first.
        let set = [rect(0.0, 0.0, 100.0, 100.0, |_, _| 100.0), rect(300.0, 0.0, 350.0, 50.0, |_, _| 100.0), rect(100.0, 0.0, 200.0, 100.0, |_, _| 100.0)];
        let made = polygon_boolean(&set, &[], PolygonOp::Union).unwrap();
        assert_eq!((made.parts, made.rings.len()), (2, 2));
        assert!((area(&made.rings[0].line) - 20_000.0).abs() < 1e-6 && (area(&made.rings[1].line) - 2_500.0).abs() < 1e-6);
        // The corners where the two touching squares met stay, on the straight edge.
        assert!(has(&made.rings[0].line, 100.0, 0.0).is_some() && has(&made.rings[0].line, 100.0, 100.0).is_some());

        // Squares apart overlap nowhere: nothing made.
        let made = polygon_boolean(&set[..1], &set[1..2], PolygonOp::Overlap).unwrap();
        assert!(made.rings.is_empty());
    }

    #[test]
    fn what_cant_be_worked_is_refused() {
        let a = rect(0.0, 0.0, 100.0, 100.0, |_, _| 100.0);
        let mut open = a.clone();
        open.closed = false;
        assert!(polygon_boolean(std::slice::from_ref(&a), std::slice::from_ref(&open), PolygonOp::Union).unwrap_err().to_string().contains("string 2"));
        assert!(polygon_boolean(std::slice::from_ref(&a), &[], PolygonOp::Overlap).is_err());
        let flat = Polyline { vertices: vec![[E, N, 0.0], [E + 10.0, N, 0.0], [E + 20.0, N, 0.0]], closed: true };
        assert!(polygon_boolean(&[flat], &[], PolygonOp::Union).unwrap_err().to_string().contains("no area"));
    }

    /// Timings on made-up data: 150 block polygons (15 strips of 10 blocks, each 100 m by 100 m with a vertex every
    /// 10 m) joined, then clipped to a 720-sided pit limit. `cargo test -p zd-geom --release timing_polygons -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timing_polygons() {
        let mut blocks = Vec::new();
        for s in 0..15 {
            for b in 0..10 {
                let (x0, y0) = (s as f64 * 100.0, b as f64 * 100.0);
                let mut vertices = Vec::new();
                for (from, to) in [([0.0, 0.0], [100.0, 0.0]), ([100.0, 0.0], [100.0, 100.0]), ([100.0, 100.0], [0.0, 100.0]), ([0.0, 100.0], [0.0, 0.0])] {
                    for k in 0..10 {
                        let t = k as f64 / 10.0;
                        let (x, y) = (x0 + from[0] + t * (to[0] - from[0]), y0 + from[1] + t * (to[1] - from[1]));
                        vertices.push([E + x, N + y, 100.0 + 0.01 * x]);
                    }
                }
                blocks.push(Polyline { vertices, closed: true });
            }
        }
        let limit = Polyline {
            vertices: (0..720).map(|k| {
                let a = k as f64 * std::f64::consts::TAU / 720.0;
                [E + 750.0 + 600.0 * a.cos(), N + 500.0 + 450.0 * a.sin(), 100.0]
            }).collect(),
            closed: true,
        };
        let vertices: usize = blocks.iter().map(|b| b.vertices.len()).sum();
        let joined = polygon_boolean(&blocks, &[], PolygonOp::Union).unwrap();
        println!("union of {} polygons ({vertices} vertices): {:.1} ms, {} strings, {} vertices out, {:.0} m²", blocks.len(), joined.ms, joined.rings.len(), joined.rings.iter().map(|r| r.line.vertices.len()).sum::<usize>(), joined.area);
        for (op, word) in [(PolygonOp::Overlap, "overlap with"), (PolygonOp::Difference, "less")] {
            let made = polygon_boolean(&blocks, std::slice::from_ref(&limit), op).unwrap();
            println!("{} polygons {word} a 720-sided limit: {:.1} ms, {} strings, {} vertices out, {:.0} m², off {}", blocks.len(), made.ms, made.rings.len(), made.rings.iter().map(|r| r.line.vertices.len()).sum::<usize>(), made.area, made.off);
        }
    }
}
