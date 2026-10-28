use zd_core::Grid;

/// An undulating topography of `cells` x `cells` at 5 m, for the transfer check. Placed at the
/// same mine-grid origin as the bench's synthetic pit.
pub fn synthetic_topography(cells: usize) -> Grid {
    let origin = [650_000.0, 7_450_000.0];
    Grid::from_fn(origin, [5.0, 5.0], cells + 1, cells + 1, move |x, y| {
        let (x, y) = (x - origin[0], y - origin[1]);
        200.0 + 8.0 * (x / 700.0).sin() + 6.0 * (y / 900.0).cos() + 3.0 * ((x + y) / 300.0).sin()
    })
}
