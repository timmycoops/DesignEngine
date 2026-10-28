use std::time::Instant;

use manifold_csg::Manifold;
use zd_core::{MeshValidity, TriMesh};

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
    pub timings: CutTimings,
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
/// so Manifold's tolerances are not spent on mine-grid offsets.
pub fn cut_solid_by_surface(solid: &TriMesh, design: &TriMesh) -> Result<CutResult, GeomError> {
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
    let below = from_manifold(&below_m, origin);
    let above = from_manifold(&above_m, origin);
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
        timings,
    })
}

/// Hands a mesh to Manifold, moved by `offset` on the way.
fn to_manifold(mesh: &TriMesh, offset: [f64; 3], what: &str) -> Result<Manifold, GeomError> {
    let verts: Vec<f64> = mesh
        .vertices
        .iter()
        .flat_map(|p| [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]])
        .collect();
    let tris: Vec<u64> = mesh.triangles.iter().flatten().map(|&i| u64::from(i)).collect();
    Manifold::from_mesh_f64(&verts, 3, &tris).map_err(|e| GeomError::Manifold(format!("{what}: {e}")))
}

/// Reads a mesh back from Manifold, moved by `offset` on the way.
fn from_manifold(m: &Manifold, offset: [f64; 3]) -> TriMesh {
    let (props, n_props, tris) = m.to_mesh_f64();
    let vertices = props
        .chunks_exact(n_props)
        .map(|p| [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]])
        .collect();
    let triangles = tris.as_chunks::<3>().0.iter().map(|t| t.map(|i| i as u32)).collect();
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
    fn open_solid_is_refused() {
        let mut open = box_seam();
        open.triangles.pop();
        assert!(matches!(
            cut_solid_by_surface(&open, &design(|_, _| 104.0)),
            Err(GeomError::NotWatertight { .. })
        ));
    }
}
