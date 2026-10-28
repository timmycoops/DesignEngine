//! Synthetic pit surfaces at mine-grid coordinates: an undulating topography, a seam dipping
//! about 2.3 degrees east that thins and pinches out in places, and a design surface that weaves
//! through the seam so a cut crosses roof and floor many times.

use zd_core::Grid;

/// South-west corner of the synthetic pit (MGA-scale easting and northing).
pub const ORIGIN: [f64; 2] = [650_000.0, 7_450_000.0];

pub struct SynthPit {
    pub topo: Grid,
    pub roof: Grid,
    pub floor: Grid,
}

/// Topography, seam roof and seam floor on one `cells` x `cells` lattice of `cell_size` metres.
pub fn synth_pit(cells: usize, cell_size: f64) -> SynthPit {
    let lattice = |f: fn(f64, f64) -> f64| {
        Grid::from_fn(ORIGIN, [cell_size, cell_size], cells + 1, cells + 1, move |x, y| f(x - ORIGIN[0], y - ORIGIN[1]))
    };
    SynthPit { topo: lattice(topo_rl), roof: lattice(roof_rl), floor: lattice(|x, y| roof_rl(x, y) - thickness(x, y)) }
}

/// A design surface overhanging the seam by two of its own cells on every side. Its cell is
/// 1.37 x the seam cell so its vertices never line up with the seam lattice.
pub fn synth_design(cells: usize, cell_size: f64) -> Grid {
    let design_cell = 1.37 * cell_size;
    let extent = cells as f64 * cell_size;
    let n = (extent / design_cell).ceil() as usize + 5;
    let origin = [ORIGIN[0] - 2.0 * design_cell, ORIGIN[1] - 2.0 * design_cell];
    Grid::from_fn(origin, [design_cell, design_cell], n, n, |x, y| design_rl(x - ORIGIN[0], y - ORIGIN[1]))
}

fn topo_rl(x: f64, y: f64) -> f64 {
    200.0 + 8.0 * (x / 700.0).sin() + 6.0 * (y / 900.0).cos() + 3.0 * ((x + y) / 300.0).sin()
}

fn roof_rl(x: f64, y: f64) -> f64 {
    170.0 - 0.04 * x + 2.0 * (y / 500.0).sin() + 1.5 * (x / 350.0).cos()
}

/// Ranges from -1 m (pinched out) to 5 m.
fn thickness(x: f64, y: f64) -> f64 {
    2.0 + 3.0 * (x / 800.0).sin() * (y / 650.0).cos()
}

fn design_rl(x: f64, y: f64) -> f64 {
    roof_rl(x, y) - 0.5 * thickness(x, y).max(0.0) + 6.0 * (x / 400.0 + y / 550.0).sin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seam_sits_below_topography_and_design_overhangs_it() {
        let pit = synth_pit(20, 50.0);
        for i in 0..pit.roof.z.len() {
            assert!(pit.roof.z[i] < pit.topo.z[i]);
        }
        let design = synth_design(20, 50.0);
        let [x0, y0] = design.origin;
        let far = design.node_xy(design.cols - 1, design.rows - 1);
        assert!(x0 < ORIGIN[0] && y0 < ORIGIN[1]);
        assert!(far[0] > ORIGIN[0] + 1000.0 && far[1] > ORIGIN[1] + 1000.0);
    }
}
