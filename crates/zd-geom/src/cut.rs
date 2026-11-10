use std::collections::HashMap;
use std::time::Instant;

use manifold_csg::Manifold;
use zd_core::{Aabb, MeshValidity, TriMesh};

use crate::{CapMethod, GeomError, Side, close_surface_to_halfspace};

#[derive(Debug, Clone)]
pub struct CutResult {
    /// Part of the solid below the design surface.
    pub below: TriMesh,
    /// Part of the solid above the design surface.
    pub above: TriMesh,
    pub below_validity: MeshValidity,
    pub above_validity: MeshValidity,
    pub input_volume: f64,
    pub below_volume: f64,
    pub above_volume: f64,
    /// False when the design surface does not cover the solid in plan; the uncovered part of
    /// the solid then reports as "above".
    pub design_covers_solid: bool,
    pub cap_method: CapMethod,
    pub halfspace_triangles: usize,
    /// What became of edges under [`TINY_EDGE`] the boolean left, below and above.
    pub tiny_edges: [TinyEdges; 2],
    pub timings: CutTimings,
}

/// Edges shorter than this (m) are a boolean's leftovers, not detail: two of its corners a nanometre apart where two
/// faces meet a third. The finest detail seen in real solids is hundredths of a millimetre (decision 0053).
pub const TINY_EDGE: f64 = 1e-6;

/// What became of a mesh's edges under [`TINY_EDGE`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TinyEdges {
    /// Corners joined into the corner at the other end of such an edge.
    pub joined: usize,
    /// Triangles that went with them: those left with no width, and pairs left folded flat on each other.
    pub triangles_dropped: usize,
    /// Edges under [`TINY_EDGE`] left as they were: joining their ends would have opened the solid, made faces meet
    /// three to an edge, or left nothing of it.
    pub left: usize,
}

impl TinyEdges {
    pub fn add(&mut self, other: TinyEdges) {
        self.joined += other.joined;
        self.triangles_dropped += other.triangles_dropped;
        self.left += other.left;
    }
}

/// A closed mesh with the two ends of each edge under [`TINY_EDGE`] joined into one corner, the first of them in the
/// mesh's order kept exactly as it is. The triangles on the edge go, having no width left, and so does a pair left
/// folded flat on each other, which encloses nothing. A mesh with no such edge comes back untouched.
///
/// The edges are joined all at once if that leaves the mesh closed, with two faces on every edge, and its volume
/// within what moving corners a micrometre can change. If not, they are tried one after another, each kept only if
/// the mesh is still sound with it, and the rest are counted as left.
pub fn join_tiny_edges(mesh: TriMesh) -> (TriMesh, TinyEdges) {
    let edges = tiny_edges(&mesh);
    if edges.is_empty() {
        return (mesh, TinyEdges::default());
    }
    let done = |joined: TriMesh, corners: usize, left: usize| {
        let triangles_dropped = mesh.triangles.len() - joined.triangles.len();
        (joined, TinyEdges { joined: corners, triangles_dropped, left })
    };
    if let Some((joined, corners)) = joined_at(&mesh, &edges) {
        return done(joined, corners, 0);
    }
    let mut kept: Vec<(u32, u32)> = Vec::new();
    let mut best = None;
    let mut left = 0;
    for &edge in &edges {
        kept.push(edge);
        match joined_at(&mesh, &kept) {
            Some(joined) => best = Some(joined),
            None => {
                kept.pop();
                left += 1;
            }
        }
    }
    match best {
        Some((joined, corners)) => done(joined, corners, left),
        None => (mesh, TinyEdges { left, ..TinyEdges::default() }),
    }
}

/// The mesh with the ends of each of `edges` joined, and how many corners went, or `None` if that leaves it unsound.
fn joined_at(mesh: &TriMesh, edges: &[(u32, u32)]) -> Option<(TriMesh, usize)> {
    // Each corner goes to the lowest-numbered corner it's joined to, through any run of edges.
    let mut to: Vec<u32> = (0..mesh.vertices.len() as u32).collect();
    fn root(to: &mut [u32], mut v: u32) -> u32 {
        while to[v as usize] != v {
            to[v as usize] = to[to[v as usize] as usize];
            v = to[v as usize];
        }
        v
    }
    let mut corners = 0;
    for &(a, b) in edges {
        let (ra, rb) = (root(&mut to, a), root(&mut to, b));
        if ra != rb {
            to[ra.max(rb) as usize] = ra.min(rb);
            corners += 1;
        }
    }
    let mut triangles: Vec<[u32; 3]> = mesh.triangles.iter().map(|t| t.map(|v| root(&mut to, v))).filter(|t| t[0] != t[1] && t[1] != t[2] && t[0] != t[2]).collect();

    // Two triangles on the same three corners, wound opposite ways, are a fold with nothing in it: both go.
    let mut keys: Vec<([u32; 3], usize)> = triangles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut key = *t;
            key.sort_unstable();
            (key, i)
        })
        .collect();
    keys.sort_unstable();
    let even = |t: [u32; 3]| {
        let low = (0..3).min_by_key(|&k| t[k]).expect("three corners");
        t[(low + 1) % 3] < t[(low + 2) % 3]
    };
    let mut fold = vec![false; triangles.len()];
    for run in keys.chunk_by(|x, y| x.0 == y.0) {
        if run.len() == 2 && even(triangles[run[0].1]) != even(triangles[run[1].1]) {
            fold[run[0].1] = true;
            fold[run[1].1] = true;
        }
    }
    let mut index = 0;
    triangles.retain(|_| {
        index += 1;
        !fold[index - 1]
    });
    if triangles.is_empty() {
        return None;
    }

    // The corners still used, in their own order.
    let mut renumber = vec![u32::MAX; mesh.vertices.len()];
    let mut used = vec![false; mesh.vertices.len()];
    triangles.iter().flatten().for_each(|&v| used[v as usize] = true);
    let mut vertices = Vec::new();
    for (v, p) in mesh.vertices.iter().enumerate().filter(|(v, _)| used[*v]) {
        renumber[v] = vertices.len() as u32;
        vertices.push(*p);
    }
    triangles.iter_mut().flatten().for_each(|v| *v = renumber[*v as usize]);
    let joined = TriMesh::new(vertices, triangles);
    let sound = joined.validity().is_watertight() && (joined.signed_volume() - mesh.signed_volume()).abs() <= TINY_EDGE * mesh.area();
    sound.then_some((joined, corners))
}

/// Edges shorter than [`TINY_EDGE`], each once, lower corner first, in order.
fn tiny_edges(mesh: &TriMesh) -> Vec<(u32, u32)> {
    let limit = TINY_EDGE * TINY_EDGE;
    let mut edges = Vec::new();
    for &[a, b, c] in &mesh.triangles {
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let (p, q) = (mesh.vertices[u as usize], mesh.vertices[v as usize]);
            if (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2) < limit {
                edges.push((u.min(v), u.max(v)));
            }
        }
    }
    edges.sort_unstable();
    edges.dedup();
    edges
}

/// Milliseconds per stage of a cut.
#[derive(Debug, Clone, Copy, Default)]
pub struct CutTimings {
    pub close_ms: f64,
    pub to_manifold_ms: f64,
    pub boolean_ms: f64,
    pub from_manifold_ms: f64,
}

impl CutTimings {
    pub fn total_ms(&self) -> f64 {
        self.close_ms + self.to_manifold_ms + self.boolean_ms + self.from_manifold_ms
    }
}

/// Cuts a closed solid by an open design surface. The surface is closed into the half-space
/// below it, capped just beyond the solid's bounding box, and Manifold splits the solid into
/// the parts inside (below) and outside (above) that half-space.
///
/// Both meshes are moved to a local origin near the solid for the boolean and moved back after,
/// so Manifold's tolerances are not spent on mine-grid offsets. Only the design near the solid is
/// used; [`cut_solid_by_whole_surface`] uses all of it.
pub fn cut_solid_by_surface(solid: &TriMesh, design: &TriMesh) -> Result<CutResult, GeomError> {
    cut(solid, design, true)
}

/// As [`cut_solid_by_surface`], with the whole design in the half-space. Manifold's tolerance
/// scales with the size of what it is given, so the two can differ below a millimetre where the
/// solid's faces lie on the design (a relimited solid cut by its own topography); a caller can try
/// this one when a part of the other fails its check.
pub fn cut_solid_by_whole_surface(solid: &TriMesh, design: &TriMesh) -> Result<CutResult, GeomError> {
    cut(solid, design, false)
}

fn cut(solid: &TriMesh, design: &TriMesh, crop: bool) -> Result<CutResult, GeomError> {
    let validity = solid.validity();
    if !validity.is_watertight() {
        return Err(GeomError::NotWatertight { what: "solid", validity });
    }
    let solid_bounds = solid.bounds().ok_or_else(|| GeomError::InvalidInput("empty solid".into()))?;
    let design_bounds = design.bounds().ok_or_else(|| GeomError::InvalidInput("empty design surface".into()))?;

    let centre = solid_bounds.centre();
    let origin = [centre[0].round(), centre[1].round(), 0.0];
    let to_local = [-origin[0], -origin[1], -origin[2]];
    let height = solid_bounds.size()[2].max(design_bounds.size()[2]);
    let margin = 1.0 + 0.01 * height;
    let cap_z = solid_bounds.min[2].min(design_bounds.min[2]) - margin;

    // Only the design over the solid matters: the triangles whose plan extent comes within `reach`
    // of the solid's. The cropped design's edge lies beyond that reach, so the closed half-space is
    // the same over the solid, and a topography of millions of triangles costs what its few hundred
    // over the solid cost. A design with nothing near the solid is used whole (all of it is above).
    let reach = margin + 0.01 * solid_bounds.size()[0].hypot(solid_bounds.size()[1]);
    let near = if crop { design_near(design, &solid_bounds, reach) } else { TriMesh::default() };
    let design = if near.triangles.is_empty() { design } else { &near };

    let mut timings = CutTimings::default();
    let t = Instant::now();
    let halfspace = close_surface_to_halfspace(&design.translated(to_local), Side::Below, cap_z - origin[2])?;
    timings.close_ms = ms(t);

    let t = Instant::now();
    let solid_m = to_manifold(solid, to_local, "solid")?;
    let halfspace_m = to_manifold(&halfspace.mesh, [0.0; 3], "closed half-space")?;
    timings.to_manifold_ms = ms(t);

    let t = Instant::now();
    let (below_m, above_m) = solid_m.split(&halfspace_m);
    below_m.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    above_m.status().map_err(|e| GeomError::Manifold(e.to_string()))?;
    timings.boolean_ms = ms(t);

    let t = Instant::now();
    let (below, tiny_below) = join_tiny_edges(from_manifold(&below_m, origin));
    let (above, tiny_above) = join_tiny_edges(from_manifold(&above_m, origin));
    timings.from_manifold_ms = ms(t);

    Ok(CutResult {
        below_validity: below.validity(),
        above_validity: above.validity(),
        input_volume: solid.signed_volume(),
        below_volume: below.signed_volume(),
        above_volume: above.signed_volume(),
        below,
        above,
        design_covers_solid: design_bounds.contains_plan(&solid_bounds),
        cap_method: halfspace.cap_method,
        halfspace_triangles: halfspace.mesh.triangle_count(),
        tiny_edges: [tiny_below, tiny_above],
        timings,
    })
}

/// Hands a mesh to Manifold, moved by `offset` on the way.
pub(crate) fn to_manifold(mesh: &TriMesh, offset: [f64; 3], what: &str) -> Result<Manifold, GeomError> {
    let verts: Vec<f64> = mesh
        .vertices
        .iter()
        .flat_map(|p| [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]])
        .collect();
    let tris: Vec<u64> = mesh.triangles.iter().flatten().map(|&i| u64::from(i)).collect();
    Manifold::from_mesh_f64(&verts, 3, &tris).map_err(|e| GeomError::Manifold(format!("{what}: {e}")))
}

/// Reads a mesh back from Manifold, moved by `offset` on the way.
pub(crate) fn from_manifold(m: &Manifold, offset: [f64; 3]) -> TriMesh {
    let (props, n_props, tris) = m.to_mesh_f64();
    let vertices = props
        .chunks_exact(n_props)
        .map(|p| [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]])
        .collect();
    let triangles = tris.as_chunks::<3>().0.iter().map(|t| t.map(|i| i as u32)).collect();
    TriMesh::new(vertices, triangles)
}

/// The design's triangles that reach within `reach` of the solid's plan extent, with their vertices.
pub(crate) fn design_near(design: &TriMesh, solid: &Aabb, reach: f64) -> TriMesh {
    let lo = [solid.min[0] - reach, solid.min[1] - reach];
    let hi = [solid.max[0] + reach, solid.max[1] + reach];
    let mut renumber: HashMap<u32, u32> = HashMap::new();
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for t in &design.triangles {
        let corners = t.map(|i| design.vertices[i as usize]);
        if (0..2).any(|k| corners.iter().all(|p| p[k] < lo[k]) || corners.iter().all(|p| p[k] > hi[k])) {
            continue;
        }
        triangles.push(t.map(|i| {
            *renumber.entry(i).or_insert_with(|| {
                vertices.push(design.vertices[i as usize]);
                vertices.len() as u32 - 1
            })
        }));
    }
    TriMesh::new(vertices, triangles)
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1e3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_seam_solid, triangulate_grid};
    use zd_core::Grid;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// 10 m x 10 m seam, floor 100, roof 110: a 1000 m3 box at mine-grid coordinates.
    fn box_seam() -> TriMesh {
        let roof = Grid::from_fn([E, N], [1.0, 1.0], 11, 11, |_, _| 110.0);
        let floor = Grid::from_fn([E, N], [1.0, 1.0], 11, 11, |_, _| 100.0);
        build_seam_solid(&roof, &floor, 0.01).unwrap().mesh
    }

    /// Design grid overhanging the seam by 1 m on every side.
    fn design(f: impl Fn(f64, f64) -> f64 + 'static) -> TriMesh {
        triangulate_grid(&Grid::from_fn([E - 1.0, N - 1.0], [0.5, 0.5], 25, 25, move |x, y| f(x - E, y - N)))
    }

    fn assert_close(actual: f64, expected: f64, what: &str) {
        // Volume tolerance: 1e-6 m3 on a 1000 m3 solid (1 ppb).
        assert!((actual - expected).abs() < 1e-6, "{what}: {actual} vs {expected}");
    }

    #[test]
    fn horizontal_cut_splits_box_by_height() {
        let cut = cut_solid_by_surface(&box_seam(), &design(|_, _| 104.0)).unwrap();
        assert!(cut.design_covers_solid);
        assert!(cut.below_validity.is_watertight() && cut.above_validity.is_watertight());
        assert_close(cut.below_volume, 400.0, "below");
        assert_close(cut.above_volume, 600.0, "above");
        let b = cut.below.bounds().unwrap();
        assert!((b.max[2] - 104.0).abs() < 1e-9 && (b.min[0] - E).abs() < 1e-9);
    }

    #[test]
    fn dipping_cut_splits_box_by_mean_height() {
        // Design dips from 102 m at x = 0 to 108 m at x = 10: 500 m3 each side.
        let cut = cut_solid_by_surface(&box_seam(), &design(|x, _| 102.0 + 0.6 * x)).unwrap();
        assert!(cut.below_validity.is_watertight() && cut.above_validity.is_watertight());
        assert_close(cut.below_volume, 500.0, "below");
        assert_close(cut.above_volume, 500.0, "above");
        assert_close(cut.below_volume + cut.above_volume, cut.input_volume, "conservation");
    }

    #[test]
    fn design_above_the_seam_leaves_all_of_it_below() {
        let cut = cut_solid_by_surface(&box_seam(), &design(|_, _| 150.0)).unwrap();
        assert_close(cut.below_volume, 1000.0, "below");
        assert!(cut.above.triangles.is_empty());
    }

    #[test]
    fn a_large_design_is_cropped_to_the_solid_and_cuts_the_same() {
        // A 400 m square design at 1 m spacing (320,000 triangles) round the 10 m box: only the
        // triangles near the box go into the half-space, and the volumes are those of a design
        // covering the box alone.
        let large = triangulate_grid(&Grid::from_fn([E - 195.0, N - 195.0], [1.0, 1.0], 401, 401, |x, _| 102.0 + 0.6 * (x - E)));
        let cut = cut_solid_by_surface(&box_seam(), &large).unwrap();
        assert!(cut.design_covers_solid);
        assert!(cut.halfspace_triangles < 2_000, "{} triangles in the half-space", cut.halfspace_triangles);
        assert_close(cut.below_volume, 500.0, "below");
        assert_close(cut.above_volume, 500.0, "above");

        // A design wholly off to one side is used whole: nothing near the box, all of it above.
        let aside = triangulate_grid(&Grid::from_fn([E + 100.0, N], [1.0, 1.0], 11, 11, |_, _| 104.0));
        let cut = cut_solid_by_surface(&box_seam(), &aside).unwrap();
        assert!(!cut.design_covers_solid && cut.below.triangles.is_empty());
        assert_close(cut.above_volume, 1000.0, "above");
    }

    #[test]
    fn corners_a_nanometre_apart_are_joined_and_nothing_else_moves() {
        // A 10 m cube whose top has a corner written twice, 0.5 nm apart, with two triangles of no width between the
        // copies: closed, 1,000 m3. Joined, it's the cube's top with one corner there and two triangles fewer.
        let mut v: Vec<[f64; 3]> = (0..8).map(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]).collect();
        v.extend([[E + 5.0, N + 5.0, 110.0], [E + 5.0 + 5e-10, N + 5.0, 110.0]]);
        let mut t = vec![[0, 2, 3], [0, 3, 1], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        t.extend([[4, 5, 8], [5, 9, 8], [5, 7, 9], [7, 6, 9], [6, 8, 9], [6, 4, 8]]);
        let cube = TriMesh::new(v.clone(), t);
        assert!(cube.validity().is_watertight());
        let (joined, tiny) = join_tiny_edges(cube.clone());
        assert_eq!(tiny, TinyEdges { joined: 1, triangles_dropped: 2, left: 0 });
        assert!(joined.validity().is_watertight());
        assert_eq!(joined.vertices, v[..9], "every corner kept is exactly as it was");
        // Volume tolerance: 1e-9 m3 on 1,000 m3.
        assert!((joined.signed_volume() - 1000.0).abs() < 1e-9, "{}", joined.signed_volume());

        // A mesh with no such edge comes back as it was, to the bit and in its order.
        let plain = box_seam();
        let (same, tiny) = join_tiny_edges(plain.clone());
        assert_eq!(tiny, TinyEdges::default());
        assert_eq!((same.vertices, same.triangles), (plain.vertices, plain.triangles));
    }

    #[test]
    fn a_wedge_of_no_height_goes_whole_and_a_mesh_that_is_only_one_is_left() {
        // A cube beside a closed wedge 0.5 nm high (3e-9 m3): joining its apex to the corner under it leaves its top
        // and bottom folded flat on each other, enclosing nothing, so both go and the cube is left as it was.
        let mut v: Vec<[f64; 3]> = (0..8).map(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 10.0, 100.0 + (i >> 2) as f64 * 10.0]).collect();
        let mut t = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        let cube = TriMesh::new(v.clone(), t.clone());
        v.extend([[E + 12.0, N, 100.0], [E + 18.0, N, 100.0], [E + 12.0, N + 6.0, 100.0], [E + 12.0, N, 100.0 + 5e-10]]);
        t.extend([[8, 10, 9], [8, 9, 11], [9, 10, 11], [10, 8, 11]]);
        let mesh = TriMesh::new(v.clone(), t);
        assert!(mesh.validity().is_watertight());
        let (joined, tiny) = join_tiny_edges(mesh);
        assert_eq!(tiny, TinyEdges { joined: 1, triangles_dropped: 4, left: 0 });
        assert_eq!((joined.vertices, joined.triangles), (cube.vertices, cube.triangles));

        // The wedge alone would leave nothing: it comes back as it was, its edge counted as left.
        let wedge = TriMesh::new(v[8..].to_vec(), vec![[0, 2, 1], [0, 1, 3], [1, 2, 3], [2, 0, 3]]);
        let (same, tiny) = join_tiny_edges(wedge.clone());
        assert_eq!(tiny, TinyEdges { joined: 0, triangles_dropped: 0, left: 1 });
        assert_eq!((same.vertices, same.triangles), (wedge.vertices, wedge.triangles));
    }

    #[test]
    fn open_solid_is_refused() {
        let mut open = box_seam();
        open.triangles.pop();
        assert!(matches!(
            cut_solid_by_surface(&open, &design(|_, _| 104.0)),
            Err(GeomError::NotWatertight { .. })
        ));
    }
}
