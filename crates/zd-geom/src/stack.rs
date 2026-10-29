//! Checks on a seam stack, and a stack suggested from the grids alone (decision 0007).
//!
//! The checks never change a grid. Each problem comes back as a [`StackIssue`] with the nodes it
//! affects, the largest overlap and where that is, so it can be fixed in the source model.

use std::collections::BTreeMap;

use zd_core::{Grid, IssueKind, StackIssue};

use crate::GeomError;

/// One horizon as the checks see it.
#[derive(Debug, Clone, Copy)]
pub struct StackHorizon<'a> {
    pub code: &'a str,
    /// Index of the parent in the same slice.
    pub parent: Option<usize>,
    pub working: bool,
    pub roof: &'a Grid,
    pub floor: &'a Grid,
}

/// Nodes affected by one issue, the largest overlap and the node where it is (the first node
/// where the issue has no distance).
#[derive(Debug, Default)]
struct Tally {
    nodes: usize,
    worst: f64,
    at: Option<usize>,
}

impl Tally {
    fn add(&mut self, node: usize, overlap: f64) {
        self.nodes += 1;
        if self.at.is_none() || overlap > self.worst {
            self.worst = overlap;
            self.at = Some(node);
        }
    }
}

/// Checks a stack, top first, on one lattice:
///
/// - every horizon: the floor above the roof, and roof and floor not live at the same nodes;
/// - every daughter: where it holds more than `tolerance` of coal, any part above its parent's roof
///   or below its parent's floor;
/// - the working section, top down: a roof above the floor of the next live horizon up.
///
/// Overlaps up to `tolerance` are listed but do not block; unpaired nodes always block.
pub fn check_seam_stack(horizons: &[StackHorizon<'_>], tolerance: f64) -> Result<Vec<StackIssue>, GeomError> {
    let Some(first) = horizons.first() else { return Ok(Vec::new()) };
    if tolerance.is_nan() || tolerance < 0.0 {
        return Err(GeomError::InvalidInput("the stack tolerance must be zero or more".into()));
    }
    let lattice = first.roof;
    for h in horizons {
        if !h.roof.same_lattice(lattice) || !h.floor.same_lattice(lattice) {
            return Err(GeomError::InvalidInput(format!("horizon '{}' is on a different lattice from '{}'", h.code, first.code)));
        }
        if h.parent.is_some_and(|p| p >= horizons.len()) {
            return Err(GeomError::InvalidInput(format!("horizon '{}' names a parent outside the stack", h.code)));
        }
    }
    let nodes = lattice.z.len();

    // Crossings in the working section, by (lower, upper) horizon.
    let working: Vec<usize> = (0..horizons.len()).filter(|&h| horizons[h].working).collect();
    let mut crossings: BTreeMap<(usize, usize), Tally> = BTreeMap::new();
    for i in 0..nodes {
        let mut above: Option<(usize, f64)> = None;
        for &w in &working {
            let (r, f) = (horizons[w].roof.z[i], horizons[w].floor.z[i]);
            if r.is_nan() || f.is_nan() {
                continue;
            }
            if let Some((upper, upper_floor)) = above
                && r > upper_floor
            {
                crossings.entry((w, upper)).or_default().add(i, r - upper_floor);
            }
            above = Some((w, f));
        }
    }

    let mut crossings = crossings.into_iter().peekable();
    let at = |node: Option<usize>| node.map(|i| lattice.node_xy(i % lattice.cols, i / lattice.cols));
    let mut issues = Vec::new();
    let mut push = |kind: IssueKind, horizon: usize, other: Option<usize>, tally: Tally, blocking: bool| {
        if tally.nodes > 0 {
            issues.push(StackIssue {
                kind,
                horizon: horizons[horizon].code.to_string(),
                other: other.map(|o| horizons[o].code.to_string()),
                nodes: tally.nodes,
                worst: tally.worst,
                at: at(tally.at),
                blocking,
            });
        }
    };
    for (index, h) in horizons.iter().enumerate() {
        let (mut inverted, mut unpaired, mut outside) = (Tally::default(), Tally::default(), Tally::default());
        let parent = h.parent.map(|p| &horizons[p]);
        for i in 0..nodes {
            let (r, f) = (h.roof.z[i], h.floor.z[i]);
            match (r.is_nan(), f.is_nan()) {
                (false, false) => {}
                (true, true) => continue,
                _ => {
                    unpaired.add(i, 0.0);
                    continue;
                }
            }
            if f > r {
                inverted.add(i, f - r);
            }
            if let Some(p) = parent
                && r - f > tolerance
            {
                let over = (r - p.roof.z[i]).max(p.floor.z[i] - f);
                if over > 0.0 {
                    outside.add(i, over);
                }
            }
        }
        let beyond = |t: &Tally| t.worst > tolerance;
        let (inverted_blocks, outside_blocks) = (beyond(&inverted), beyond(&outside));
        push(IssueKind::FloorAboveRoof, index, None, inverted, inverted_blocks);
        push(IssueKind::Unpaired, index, None, unpaired, true);
        push(IssueKind::OutsideParent, index, h.parent, outside, outside_blocks);
        while let Some(((_, upper), tally)) = crossings.next_if(|((lower, _), _)| *lower == index) {
            let blocks = beyond(&tally);
            push(IssueKind::Crossing, index, Some(upper), tally, blocks);
        }
    }
    Ok(issues)
}

/// A horizon's place in a suggested stack: its index in the input and its parent's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuggestedHorizon {
    pub index: usize,
    pub parent: Option<usize>,
}

/// Share of a daughter's coal nodes that must sit inside a horizon for it to be suggested as the
/// parent. Lenient on purpose: the suggestion is a starting point, and the check lists every node
/// outside the parent.
const NESTED_SHARE: f64 = 0.99;

/// Suggests a stack from roof and floor grids alone, whatever the horizons are called: a horizon is a
/// daughter of the thinnest horizon that holds it (at nearly every node where it has more than
/// `tolerance` of coal, it lies within that horizon's roof and floor, and that horizon holds more coal
/// overall). Horizons come out top first: each parent before its daughters, and horizons with the
/// same parent ordered by how far one roof lies above the other (the median where both are live).
pub fn suggest_seam_tree(horizons: &[(&Grid, &Grid)], tolerance: f64) -> Result<Vec<SuggestedHorizon>, GeomError> {
    let Some(&(lattice, _)) = horizons.first() else { return Ok(Vec::new()) };
    if horizons.iter().any(|(r, f)| !r.same_lattice(lattice) || !f.same_lattice(lattice)) {
        return Err(GeomError::InvalidInput("every roof and floor must share one lattice".into()));
    }
    let nodes = lattice.z.len();
    let coal: Vec<Vec<usize>> =
        horizons.iter().map(|(r, f)| (0..nodes).filter(|&i| r.z[i] - f.z[i] > tolerance).collect()).collect();
    let total: Vec<f64> =
        horizons.iter().map(|(r, f)| (0..nodes).map(|i| r.z[i] - f.z[i]).filter(|t| *t > 0.0).sum()).collect();

    let holds = |a: usize, b: usize| {
        if a == b || coal[b].is_empty() || total[a] <= total[b] {
            return false;
        }
        let ((ra, fa), (rb, fb)) = (horizons[a], horizons[b]);
        let inside = coal[b]
            .iter()
            .filter(|&&i| ra.z[i] - fa.z[i] > tolerance && rb.z[i] <= ra.z[i] + tolerance && fb.z[i] >= fa.z[i] - tolerance)
            .count();
        inside as f64 >= NESTED_SHARE * coal[b].len() as f64
    };
    let parent: Vec<Option<usize>> = (0..horizons.len())
        .map(|b| (0..horizons.len()).filter(|&a| holds(a, b)).min_by(|&x, &y| total[x].total_cmp(&total[y])))
        .collect();

    // Two horizons are compared on the same nodes: a seam that holds coal only up-dip must not rank
    // above one that lies over it everywhere.
    let above = |a: usize, b: usize| {
        let (ra, rb) = (horizons[a].0, horizons[b].0);
        let mut gap: Vec<f64> = (0..nodes).map(|i| ra.z[i] - rb.z[i]).filter(|d| !d.is_nan()).collect();
        median(&mut gap).is_some_and(|d| d > 0.0)
    };
    let children = |of: Option<usize>| {
        // Insertion sort: the comparison need not be a total order on odd data, and a few horizons
        // per parent make the cost nothing.
        let mut list: Vec<usize> = Vec::new();
        for h in (0..horizons.len()).filter(|&h| parent[h] == of) {
            let at = list.iter().position(|&other| above(h, other)).unwrap_or(list.len());
            list.insert(at, h);
        }
        list
    };
    // Parents are always thicker than their daughters, so the tree has no cycles.
    let mut order = Vec::with_capacity(horizons.len());
    let mut pending: Vec<usize> = children(None).into_iter().rev().collect();
    while let Some(h) = pending.pop() {
        order.push(SuggestedHorizon { index: h, parent: parent[h] });
        pending.extend(children(Some(h)).into_iter().rev());
    }
    Ok(order)
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mid = values.len() / 2;
    let (_, m, _) = values.select_nth_unstable_by(mid, f64::total_cmp);
    Some(*m)
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 600_000.0;
    const N: f64 = 7_500_000.0;

    /// A 21 x 11 lattice at 10 m in mine grid coordinates; `f` takes local x, y in metres.
    fn grid(f: impl Fn(f64, f64) -> f64) -> Grid {
        Grid::from_fn([E, N], [10.0, 10.0], 21, 11, move |x, y| f(x - E, y - N))
    }

    fn horizon<'a>(code: &'a str, parent: Option<usize>, working: bool, roof: &'a Grid, floor: &'a Grid) -> StackHorizon<'a> {
        StackHorizon { code, parent, working, roof, floor }
    }

    /// A seam `a` from 100 down to 90, split by a parting from 96 to 94 into `au` (100..96) and
    /// `al` (94..90). All planes, so every check has an exact answer.
    struct Split {
        roofs: Vec<Grid>,
        floors: Vec<Grid>,
    }

    fn split() -> Split {
        Split {
            roofs: vec![grid(|_, _| 100.0), grid(|_, _| 100.0), grid(|_, _| 94.0)],
            floors: vec![grid(|_, _| 90.0), grid(|_, _| 96.0), grid(|_, _| 90.0)],
        }
    }

    fn stack(s: &Split) -> Vec<StackHorizon<'_>> {
        vec![
            horizon("a", None, false, &s.roofs[0], &s.floors[0]),
            horizon("au", Some(0), true, &s.roofs[1], &s.floors[1]),
            horizon("al", Some(0), true, &s.roofs[2], &s.floors[2]),
        ]
    }

    #[test]
    fn a_consistent_split_has_no_issues() {
        let s = split();
        assert_eq!(check_seam_stack(&stack(&s), 0.001).unwrap(), vec![]);
    }

    #[test]
    fn a_floor_above_its_roof_is_flagged_where_and_by_how_much() {
        let mut s = split();
        // au's floor rises 0.08 m per metre east, above its roof (100) where 96 + 0.08x > 100, x > 50:
        // columns 6..=20 (x = 60..200), 15 columns by 11 rows.
        s.floors[1] = grid(|x, _| 96.0 + 0.08 * x);
        let issues = check_seam_stack(&stack(&s), 0.001).unwrap();
        let inverted = issues.iter().find(|i| i.kind == IssueKind::FloorAboveRoof).unwrap();
        assert_eq!((inverted.horizon.as_str(), inverted.nodes), ("au", 15 * 11));
        // Worst at x = 200: 96 + 16 - 100 = 12 m, first met on row 0.
        assert!((inverted.worst - 12.0).abs() < 1e-9);
        assert_eq!(inverted.at, Some([E + 200.0, N]));
        assert!(inverted.blocking);
    }

    #[test]
    fn a_crossing_between_plies_is_flagged_with_the_one_above() {
        let mut s = split();
        // al's roof rises to 97 west of x = 50 (columns 0..=4), above au's floor at 96: 1 m over.
        s.roofs[2] = grid(|x, _| if x < 50.0 { 97.0 } else { 94.0 });
        let issues = check_seam_stack(&stack(&s), 0.001).unwrap();
        let crossing = issues.iter().find(|i| i.kind == IssueKind::Crossing).unwrap();
        assert_eq!((crossing.horizon.as_str(), crossing.other.as_deref(), crossing.nodes), ("al", Some("au"), 5 * 11));
        assert!((crossing.worst - 1.0).abs() < 1e-12);
        assert_eq!(crossing.at, Some([E, N]));
        assert!(crossing.blocking);
        // al stays inside its parent a (97 < 100), so the crossing is the only issue.
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn overlaps_within_the_tolerance_are_listed_but_do_not_block() {
        let mut s = split();
        s.roofs[2] = grid(|_, _| 96.0005);
        let issues = check_seam_stack(&stack(&s), 0.001).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!((issues[0].kind, issues[0].nodes, issues[0].blocking), (IssueKind::Crossing, 21 * 11, false));
        assert!((issues[0].worst - 0.0005).abs() < 1e-9);
    }

    #[test]
    fn a_daughter_outside_its_parent_is_flagged() {
        let mut s = split();
        // a's floor rises to 92 on the top row (y = 100): al (94..90) reaches 2 m below it there.
        s.floors[0] = grid(|_, y| if y > 95.0 { 92.0 } else { 90.0 });
        let issues = check_seam_stack(&stack(&s), 0.001).unwrap();
        assert_eq!(issues.len(), 1);
        let outside = &issues[0];
        assert_eq!((outside.kind, outside.horizon.as_str(), outside.other.as_deref()), (IssueKind::OutsideParent, "al", Some("a")));
        assert_eq!(outside.nodes, 21);
        assert!((outside.worst - 2.0).abs() < 1e-12);
        assert_eq!(outside.at, Some([E, N + 100.0]));
    }

    #[test]
    fn a_daughter_without_coal_may_sit_anywhere() {
        let mut s = split();
        // au pinches out at 120 m, far above a: zero thickness is not outside the parent.
        s.roofs[1] = grid(|_, _| 120.0);
        s.floors[1] = grid(|_, _| 120.0);
        let mut horizons = stack(&s);
        horizons[1].working = false;
        assert_eq!(check_seam_stack(&horizons, 0.001).unwrap(), vec![]);
    }

    #[test]
    fn unpaired_nulls_always_block() {
        let mut s = split();
        s.floors[2].z[7] = f64::NAN;
        let issues = check_seam_stack(&stack(&s), 0.001).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!((issues[0].kind, issues[0].nodes, issues[0].blocking), (IssueKind::Unpaired, 1, true));
        assert_eq!(issues[0].at, Some([E + 70.0, N]));
    }

    #[test]
    fn crossings_skip_horizons_that_are_null_at_a_node() {
        let mut s = split();
        // au is null on column 0, so al is compared with the next live horizon up there: none in the
        // working section, so no crossing even with al's roof at 99.
        s.roofs[1].z[0] = f64::NAN;
        s.floors[1].z[0] = f64::NAN;
        s.roofs[2].z[0] = 99.0;
        let issues = check_seam_stack(&stack(&s), 0.001).unwrap();
        assert!(issues.iter().all(|i| i.kind != IssueKind::Crossing), "{issues:?}");
    }

    #[test]
    fn mixed_lattices_are_refused() {
        let s = split();
        let other = Grid::from_fn([E, N], [5.0, 5.0], 41, 21, |_, _| 0.0);
        let mut horizons = stack(&s);
        horizons[2].floor = &other;
        assert!(check_seam_stack(&horizons, 0.001).is_err());
    }

    #[test]
    fn the_tree_is_suggested_from_geometry_alone() {
        // Given bottom-up and named nothing in particular: l2 below everything, then the split seam.
        let s = split();
        let (l2_roof, l2_floor) = (grid(|_, _| 80.0), grid(|_, _| 78.0));
        let input = [(&l2_roof, &l2_floor), (&s.roofs[2], &s.floors[2]), (&s.roofs[0], &s.floors[0]), (&s.roofs[1], &s.floors[1])];
        let tree = suggest_seam_tree(&input, 0.001).unwrap();
        // a (index 2) first, then au (3) and al (1) as its daughters, then l2 (0).
        assert_eq!(
            tree,
            [
                SuggestedHorizon { index: 2, parent: None },
                SuggestedHorizon { index: 3, parent: Some(2) },
                SuggestedHorizon { index: 1, parent: Some(2) },
                SuggestedHorizon { index: 0, parent: None },
            ]
        );
    }

    #[test]
    fn a_seam_with_coal_only_up_dip_still_sorts_below_the_one_over_it() {
        // c dips: roof 100 + 0.2x (100..140), 10 m thick. l2 lies 5 m below c everywhere but holds
        // coal only from x = 180 m, where its roof is 121..125: above the median of c's roof (120).
        let (c_r, c_f) = (grid(|x, _| 100.0 + 0.2 * x), grid(|x, _| 90.0 + 0.2 * x));
        let l2_r = grid(|x, _| 85.0 + 0.2 * x);
        let l2_f = grid(|x, _| if x >= 180.0 { 84.0 + 0.2 * x } else { 85.0 + 0.2 * x });
        let tree = suggest_seam_tree(&[(&l2_r, &l2_f), (&c_r, &c_f)], 0.001).unwrap();
        assert_eq!(tree.iter().map(|s| s.index).collect::<Vec<_>>(), [1, 0]);
    }

    #[test]
    fn nested_splits_take_the_thinnest_holder_as_parent() {
        // c (100..80) holds u (100..92) which holds uu (100..97); uu's parent is u, not c.
        let (c_r, c_f, u_r, u_f, uu_r, uu_f) =
            (grid(|_, _| 100.0), grid(|_, _| 80.0), grid(|_, _| 100.0), grid(|_, _| 92.0), grid(|_, _| 100.0), grid(|_, _| 97.0));
        let tree = suggest_seam_tree(&[(&uu_r, &uu_f), (&c_r, &c_f), (&u_r, &u_f)], 0.001).unwrap();
        let parent_of = |i| tree.iter().find(|s| s.index == i).unwrap().parent;
        assert_eq!((parent_of(0), parent_of(2), parent_of(1)), (Some(2), Some(1), None));
        assert_eq!(tree.iter().map(|s| s.index).collect::<Vec<_>>(), [1, 2, 0]);
    }
}
