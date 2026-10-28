/// Regular raster. Node `(col, row)` sits at `origin + (col * dx, row * dy)`; row 0 is the
/// southern edge. `z` is row-major (`row * cols + col`) with NaN for nulls.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub origin: [f64; 2],
    pub cell_size: [f64; 2],
    pub cols: usize,
    pub rows: usize,
    pub z: Vec<f64>,
}

impl Grid {
    pub fn new(origin: [f64; 2], cell_size: [f64; 2], cols: usize, rows: usize, z: Vec<f64>) -> Self {
        assert_eq!(z.len(), cols * rows, "grid z length must be cols * rows");
        assert!(cell_size[0] > 0.0 && cell_size[1] > 0.0, "grid cell size must be positive");
        Grid { origin, cell_size, cols, rows, z }
    }

    /// A grid whose node values come from `f(x, y)`.
    pub fn from_fn(
        origin: [f64; 2],
        cell_size: [f64; 2],
        cols: usize,
        rows: usize,
        f: impl Fn(f64, f64) -> f64,
    ) -> Self {
        let mut z = Vec::with_capacity(cols * rows);
        for row in 0..rows {
            let y = origin[1] + row as f64 * cell_size[1];
            for col in 0..cols {
                z.push(f(origin[0] + col as f64 * cell_size[0], y));
            }
        }
        Grid::new(origin, cell_size, cols, rows, z)
    }

    /// Same lattice as `self`, every node null.
    pub fn null_like(&self) -> Grid {
        Grid::new(self.origin, self.cell_size, self.cols, self.rows, vec![f64::NAN; self.z.len()])
    }

    pub fn index(&self, col: usize, row: usize) -> usize {
        row * self.cols + col
    }

    pub fn z_at(&self, col: usize, row: usize) -> f64 {
        self.z[self.index(col, row)]
    }

    pub fn node_xy(&self, col: usize, row: usize) -> [f64; 2] {
        [
            self.origin[0] + col as f64 * self.cell_size[0],
            self.origin[1] + row as f64 * self.cell_size[1],
        ]
    }

    /// True when both grids have identical node positions.
    pub fn same_lattice(&self, other: &Grid) -> bool {
        self.origin == other.origin
            && self.cell_size == other.cell_size
            && self.cols == other.cols
            && self.rows == other.rows
    }

    pub fn null_count(&self) -> usize {
        self.z.iter().filter(|z| z.is_nan()).count()
    }

    /// Lowest and highest live node value; `None` when every node is null.
    pub fn z_range(&self) -> Option<[f64; 2]> {
        self.z.iter().filter(|z| !z.is_nan()).fold(None, |range, &z| match range {
            None => Some([z, z]),
            Some([lo, hi]) => Some([lo.min(z), hi.max(z)]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_positions_and_indexing() {
        let g = Grid::from_fn([100.0, 200.0], [10.0, 5.0], 3, 2, |x, y| x + y);
        assert_eq!(g.node_xy(2, 1), [120.0, 205.0]);
        assert_eq!(g.z_at(2, 1), 325.0);
        assert_eq!(g.index(2, 1), 5);
        assert!(g.same_lattice(&g.null_like()));
        assert_eq!(g.null_like().null_count(), 6);
    }

    #[test]
    fn z_range_skips_nulls() {
        let mut g = Grid::from_fn([0.0, 0.0], [1.0, 1.0], 3, 2, |x, y| 100.0 + x - 2.0 * y);
        g.z[0] = f64::NAN;
        // Live nodes: 101, 102 (row 0) and 98, 99, 100 (row 1); node (0, 0) = 100 is null.
        assert_eq!(g.z_range(), Some([98.0, 102.0]));
        assert_eq!(g.null_like().z_range(), None);
    }
}
