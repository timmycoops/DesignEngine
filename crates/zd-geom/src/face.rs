//! A whole face (Phase 3): a toe registered on its grid, then a list of steps up from it and a list
//! down from it, each a wall to a surface or RL or a berm. Built oversized on purpose: it runs from RL
//! to RL and past the pit, and the cut does the fitting (decision 0023).
//!
//! Each wall comes from `project_wall`, and the crest goes through `next_toe` before the next one.
//! Steps at one batter with no berm between them are one wall: a face projected to the topography and
//! on to an RL is the same plane carrying straight through, not a new wall from a rough line. The leg down is the same thing upside down: the sheet carries on below the toe on the far side, so
//! heights are mirrored and the side swapped, and a wall that "rises" there is going down. Plan winding
//! doesn't change, so the whole sheet stays wound upward.

use std::collections::HashMap;

use zd_core::{Polyline, TriMesh};

use crate::string::TriangleBins;
use crate::wall::{Stretch, triangles_of};
use crate::{GeomError, OffsetSide, drape_string, next_toe, project_wall};

/// What a wall rises to.
#[derive(Debug, Clone, Copy)]
pub enum FaceTarget<'a> {
    Surface(&'a TriMesh),
    Rl(f64),
}

/// One step of a face.
#[derive(Debug, Clone, Copy)]
pub enum FaceStep<'a> {
    /// A wall at a batter in degrees.
    Project { to: FaceTarget<'a>, batter: f64 },
    /// A berm of a width in metres, on whatever the last wall reached.
    Berm { width: f64 },
}

/// Which way from the toe a step goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leg {
    Up,
    Down,
}

/// One wall of a face.
#[derive(Debug, Clone)]
pub struct FaceBench {
    pub leg: Leg,
    /// Where it is in its leg's steps, from 0.
    pub step: usize,
    pub toe: Polyline,
    pub crest: Polyline,
}

/// What one step did.
#[derive(Debug, Clone)]
pub struct StepReport {
    pub leg: Leg,
    pub step: usize,
    pub walls: usize,
    pub squeezed_out: usize,
    /// The step has no height anywhere: its target is on or below the string all the way along.
    pub no_wall: bool,
    /// Stretches where the target is below the string.
    pub no_height: Vec<Stretch>,
    /// Stretches where the target has no data and the crest bridges the gap.
    pub no_target: Vec<Stretch>,
    /// Vertices of the next toe with no surface under them, which hold their elevations.
    pub off_surface: usize,
    /// Pieces of a berm line left out when an offset pinched into several.
    pub pieces_dropped: usize,
}

/// A whole face.
#[derive(Debug, Clone)]
pub struct Face {
    /// Walls and berms as one sheet, wound upward.
    pub surface: TriMesh,
    pub benches: Vec<FaceBench>,
    pub steps: Vec<StepReport>,
}

/// What registering a toe did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegisterLog {
    /// Vertices with no grid under them, which carry on at the grade beside them.
    pub off_grid: usize,
}

/// A string extended straight on past each end by `start` and `end` metres in plan. Closed strings
/// have no ends. The new ends take the end's elevation, as they're registered next.
pub fn run_on(line: &Polyline, [start, end]: [f64; 2]) -> Result<Polyline, GeomError> {
    if !(start.is_finite() && end.is_finite() && start >= 0.0 && end >= 0.0) {
        return Err(GeomError::InvalidInput("a run-on must be zero or more".into()));
    }
    let n = line.vertices.len();
    if line.closed || n < 2 {
        return Ok(line.clone());
    }
    let past = |from: [f64; 3], towards: [f64; 3], distance: f64| {
        let length = (from[0] - towards[0]).hypot(from[1] - towards[1]);
        (length > 0.0).then(|| [from[0] + (from[0] - towards[0]) / length * distance, from[1] + (from[1] - towards[1]) / length * distance, from[2]])
    };
    let mut vertices = Vec::with_capacity(n + 2);
    if start > 0.0 {
        vertices.extend(past(line.vertices[0], line.vertices[1], start));
    }
    vertices.extend_from_slice(&line.vertices);
    if end > 0.0 {
        vertices.extend(past(line.vertices[n - 1], line.vertices[n - 2], end));
    }
    Ok(Polyline { vertices, closed: false })
}

/// A toe laid on the grid it starts from, following the grid between vertices. Where there's no grid
/// under it, in a no-seam hole or past an edge, it carries on at the grade beside it: straight across a
/// hole, and at its last grade past an end.
pub fn register_toe(line: &Polyline, surface: &TriMesh) -> Result<(Polyline, RegisterLog), GeomError> {
    let (mut toe, _) = drape_string(line, surface, true)?;
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &toe.vertices {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let bins = TriangleBins::new(surface, lo, hi);
    let on: Vec<bool> = toe.vertices.iter().map(|p| bins.height(surface, [p[0], p[1]]).is_some()).collect();
    let off_grid = on.iter().filter(|o| !**o).count();
    if off_grid == toe.vertices.len() {
        return Err(GeomError::InvalidInput("the toe doesn't cross the grid it registers to".into()));
    }
    let along = plan_distances(&toe.vertices);
    let known: Vec<usize> = (0..on.len()).filter(|&k| on[k]).collect();
    for k in (0..on.len()).filter(|&k| !on[k]) {
        let after = known.partition_point(|&i| i < k);
        let z = match (after.checked_sub(1).map(|i| known[i]), known.get(after).copied()) {
            (Some(a), Some(b)) => {
                let (za, zb) = (toe.vertices[a][2], toe.vertices[b][2]);
                za + (zb - za) * (along[k] - along[a]) / (along[b] - along[a])
            }
            (Some(a), None) => toe.vertices[a][2] + last_grade(&toe.vertices, &along, &known[..after], true) * (along[k] - along[a]),
            (None, Some(b)) => toe.vertices[b][2] + last_grade(&toe.vertices, &along, &known[after..], false) * (along[k] - along[b]),
            (None, None) => unreachable!("some vertex is on the grid"),
        };
        toe.vertices[k][2] = z;
    }
    Ok((toe, RegisterLog { off_grid }))
}

/// Plan distance along a string at each vertex.
fn plan_distances(points: &[[f64; 3]]) -> Vec<f64> {
    let mut along = Vec::with_capacity(points.len());
    let mut total = 0.0;
    for (k, p) in points.iter().enumerate() {
        if k > 0 {
            total += (p[0] - points[k - 1][0]).hypot(p[1] - points[k - 1][1]);
        }
        along.push(total);
    }
    along
}

/// How far back from an end of the known vertices a grade is read (m).
const GRADE_OVER: f64 = 20.0;

/// The grade per metre along the string over the last stretch of known vertices.
fn last_grade(points: &[[f64; 3]], along: &[f64], known: &[usize], at_the_far_end: bool) -> f64 {
    let (Some(&first), Some(&last)) = (known.first(), known.last()) else { return 0.0 };
    let (near, far) = if at_the_far_end {
        (last, known.iter().rev().copied().find(|&k| along[last] - along[k] >= GRADE_OVER).unwrap_or(first))
    } else {
        (first, known.iter().copied().find(|&k| along[k] - along[first] >= GRADE_OVER).unwrap_or(last))
    };
    if near == far { 0.0 } else { (points[near][2] - points[far][2]) / (along[near] - along[far]) }
}

/// The whole face from a toe already on its grid. `register` is the surface the toe lies on.
pub fn project_face(toe: &Polyline, register: &TriMesh, side: OffsetSide, up: &[FaceStep<'_>], down: &[FaceStep<'_>], tolerance: f64) -> Result<Face, GeomError> {
    let mut face = FaceParts::default();
    run_leg(toe, register, side, up, tolerance, Leg::Up, &mut face)?;
    if !down.is_empty() {
        let (toe, surface) = (mirrored_line(toe), mirrored_mesh(register));
        let swapped = if side == OffsetSide::Left { OffsetSide::Right } else { OffsetSide::Left };
        let surfaces: Vec<Option<TriMesh>> = down
            .iter()
            .map(|step| match step {
                FaceStep::Project { to: FaceTarget::Surface(mesh), .. } => Some(mirrored_mesh(mesh)),
                _ => None,
            })
            .collect();
        let steps: Vec<FaceStep<'_>> = down
            .iter()
            .zip(&surfaces)
            .map(|(step, mirror)| match (step, mirror) {
                (FaceStep::Project { batter, .. }, Some(mesh)) => FaceStep::Project { to: FaceTarget::Surface(mesh), batter: *batter },
                (FaceStep::Project { to: FaceTarget::Rl(rl), batter }, _) => FaceStep::Project { to: FaceTarget::Rl(-rl), batter: *batter },
                (other, _) => *other,
            })
            .collect();
        let mut below = FaceParts::default();
        run_leg(&toe, &surface, swapped, &steps, tolerance, Leg::Down, &mut below)?;
        below.mirror();
        face.absorb(below);
    }
    Ok(Face { surface: face.joined(), benches: face.benches, steps: face.steps })
}

/// What a leg has made so far.
#[derive(Default)]
struct FaceParts {
    meshes: Vec<TriMesh>,
    benches: Vec<FaceBench>,
    steps: Vec<StepReport>,
}

impl FaceParts {
    fn mirror(&mut self) {
        for mesh in &mut self.meshes {
            *mesh = mirrored_mesh(mesh);
        }
        for bench in &mut self.benches {
            bench.toe = mirrored_line(&bench.toe);
            bench.crest = mirrored_line(&bench.crest);
        }
        for stretch in self.steps.iter_mut().flat_map(|s| s.no_height.iter_mut().chain(s.no_target.iter_mut())) {
            stretch.from[2] = -stretch.from[2];
            stretch.to[2] = -stretch.to[2];
        }
    }

    fn absorb(&mut self, other: FaceParts) {
        self.meshes.extend(other.meshes);
        self.benches.extend(other.benches);
        self.steps.extend(other.steps);
    }

    /// One mesh, sharing vertices within a micrometre.
    fn joined(&self) -> TriMesh {
        let mut out = TriMesh::default();
        let mut numbered: HashMap<[i64; 3], u32> = HashMap::new();
        for mesh in &self.meshes {
            let renumbered: Vec<u32> = mesh
                .vertices
                .iter()
                .map(|p| {
                    *numbered.entry(p.map(|c| (c * 1e6).round() as i64)).or_insert_with(|| {
                        out.vertices.push(*p);
                        out.vertices.len() as u32 - 1
                    })
                })
                .collect();
            for t in &mesh.triangles {
                let [a, b, c] = t.map(|v| renumbered[v as usize]);
                if a != b && b != c && c != a {
                    out.triangles.push([a, b, c]);
                }
            }
        }
        out
    }
}

/// How far past a string a level sheet reaches, in plan: well past any wall to it.
fn sheet_reach(line: &Polyline, rl: f64, batter: f64) -> f64 {
    let lowest = line.vertices.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    100.0 + 8.0 * (rl - lowest).abs() / batter.to_radians().tan()
}

/// A level sheet at an RL over a string's plan area, as a target for a wall.
fn level_sheet(line: &Polyline, rl: f64, batter: f64) -> TriMesh {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &line.vertices {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let reach = sheet_reach(line, rl, batter);
    let (x0, y0, x1, y1) = (lo[0] - reach, lo[1] - reach, hi[0] + reach, hi[1] + reach);
    TriMesh::new(vec![[x0, y0, rl], [x1, y0, rl], [x1, y1, rl], [x0, y1, rl]], vec![[0, 1, 2], [0, 2, 3]])
}

fn run_leg<'a>(toe: &Polyline, register: &'a TriMesh, side: OffsetSide, steps: &[FaceStep<'a>], tolerance: f64, leg: Leg, face: &mut FaceParts) -> Result<(), GeomError> {
    let mut from = toe.clone();
    // The crest waiting to become the next toe, and what it lies on.
    let mut pending: Option<Polyline> = None;
    let mut on = Resting::Surface(register);
    let fail = |k: usize, e: GeomError| GeomError::InvalidInput(format!("{} step {}: {e}", if leg == Leg::Up { "up" } else { "down" }, k + 1));
    let mut k = 0;
    while k < steps.len() {
        match steps[k] {
            FaceStep::Project { batter, .. } => {
                if let Some(crest) = pending.take() {
                    // A conditioned line weaves either side of its crest, so with no berm here it stands
                    // clear of it by the tolerance, which can only widen what's there.
                    let (toes, _) = next_toe(&crest, tolerance, side, tolerance, on.mesh()).map_err(|e| fail(k, e))?;
                    from = longest(toes);
                    face.meshes.push(band(&crest, &from).map_err(|e| fail(k, e))?);
                }
                // Steps at one batter with no berm between them are one wall carrying straight on: the
                // targets on the way only mark where its crest lines fall.
                let last = (k..steps.len()).take_while(|&j| matches!(steps[j], FaceStep::Project { batter: b, .. } if b == batter)).last().unwrap_or(k);
                for (j, &step) in steps.iter().enumerate().take(last).skip(k) {
                    let target = Resting::of(step, &from);
                    match project_wall(&from, side, batter, target.mesh()) {
                        Ok(wall) => {
                            face.steps.push(wall_report(leg, j, &wall));
                            face.benches.push(FaceBench { leg, step: j, toe: from.clone(), crest: wall.crest });
                        }
                        // The wall carries on through here whatever this target does.
                        Err(_) => face.steps.push(no_wall_report(leg, j)),
                    }
                }
                let target = Resting::of(steps[last], &from);
                match project_wall(&from, side, batter, target.mesh()) {
                    Ok(wall) => {
                        face.steps.push(wall_report(leg, last, &wall));
                        face.benches.push(FaceBench { leg, step: last, toe: from.clone(), crest: wall.crest.clone() });
                        face.meshes.push(wall.wall);
                        pending = Some(wall.crest);
                        on = target;
                    }
                    Err(GeomError::NoHeight) => {
                        // Nothing to build here: the sheet carries on from the same string.
                        face.steps.push(no_wall_report(leg, last));
                        pending = Some(from.clone());
                    }
                    Err(e) => return Err(fail(last, e)),
                }
                k = last + 1;
            }
            FaceStep::Berm { width } => {
                let crest = pending.take().unwrap_or_else(|| from.clone());
                let (toes, log) = next_toe(&crest, width, side, tolerance, on.mesh()).map_err(|e| fail(k, e))?;
                let dropped = toes.len().saturating_sub(1);
                from = longest(toes);
                hold_above_the_target(&mut from, &crest, on.mesh());
                face.meshes.push(band(&crest, &from).map_err(|e| fail(k, e))?);
                let mut report = no_wall_report(leg, k);
                (report.no_wall, report.off_surface, report.pieces_dropped) = (false, log.off_surface, dropped);
                face.steps.push(report);
                k += 1;
            }
        }
    }
    Ok(())
}

/// What a line lies on or a wall rises to.
enum Resting<'a> {
    Surface(&'a TriMesh),
    Level(TriMesh),
}

impl<'a> Resting<'a> {
    /// The target of a step, for a wall from `from`.
    fn of(step: FaceStep<'a>, from: &Polyline) -> Self {
        match step {
            FaceStep::Project { to: FaceTarget::Surface(mesh), .. } => Resting::Surface(mesh),
            FaceStep::Project { to: FaceTarget::Rl(rl), batter } => Resting::Level(level_sheet(from, rl, batter)),
            FaceStep::Berm { .. } => unreachable!("a berm has no target"),
        }
    }

    fn mesh(&self) -> &TriMesh {
        match self {
            Resting::Surface(mesh) => mesh,
            Resting::Level(sheet) => sheet,
        }
    }
}

fn wall_report(leg: Leg, step: usize, wall: &crate::WallStep) -> StepReport {
    StepReport {
        walls: wall.log.walls,
        squeezed_out: wall.log.squeezed_out,
        no_wall: false,
        no_height: wall.log.no_height.clone(),
        no_target: wall.log.no_target.clone(),
        ..no_wall_report(leg, step)
    }
}

fn no_wall_report(leg: Leg, step: usize) -> StepReport {
    StepReport { leg, step, walls: 0, squeezed_out: 0, no_wall: true, no_height: Vec::new(), no_target: Vec::new(), off_surface: 0, pieces_dropped: 0 }
}

fn longest(toes: Vec<Polyline>) -> Polyline {
    toes.into_iter().max_by(|a, b| a.plan_length().total_cmp(&b.plan_length())).expect("next_toe gives at least one string")
}

/// A crest that ran along its string where the target is below it stays up there: a toe beside that
/// stretch keeps the crest's elevation instead of dropping onto the target.
fn hold_above_the_target(toe: &mut Polyline, crest: &Polyline, surface: &TriMesh) {
    if surface.triangles.is_empty() {
        return;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in crest.vertices.iter().chain(&toe.vertices) {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let bins = TriangleBins::new(surface, lo, hi);
    let raised: Vec<bool> = crest.vertices.iter().map(|p| bins.height(surface, [p[0], p[1]]).is_some_and(|z| p[2] > z + 1e-6)).collect();
    if !raised.contains(&true) {
        return;
    }
    for p in &mut toe.vertices {
        let nearest = (0..crest.vertices.len()).min_by(|&a, &b| {
            let gap = |k: usize| (crest.vertices[k][0] - p[0]).hypot(crest.vertices[k][1] - p[1]);
            gap(a).total_cmp(&gap(b))
        });
        if let Some(k) = nearest.filter(|&k| raised[k]) {
            p[2] = p[2].max(crest.vertices[k][2]);
        }
    }
}

/// The sheet between two strings running the same way, wound upward: the berm between a crest and the
/// next toe. Its outline is triangulated in plan, so it can't reach across either line. A closed pair
/// goes round as one outline, joined by a slit from first vertex to first vertex.
fn band(a: &Polyline, b: &Polyline) -> Result<TriMesh, GeomError> {
    if a.vertices.len() < 2 || b.vertices.len() < 2 || a.vertices == b.vertices {
        return Ok(TriMesh::default());
    }
    let mut outline: Vec<[f64; 3]> = a.vertices.clone();
    if a.closed {
        outline.push(a.vertices[0]);
        outline.push(b.vertices[0]);
    }
    outline.extend(b.vertices.iter().rev());
    if a.closed {
        outline.pop();
        outline.insert(a.vertices.len() + 2, b.vertices[0]);
        outline.truncate(a.vertices.len() + 2 + b.vertices.len());
    }
    let about = outline[0];
    let local: Vec<[f64; 3]> = outline.iter().map(|p| [p[0] - about[0], p[1] - about[1], p[2] - about[2]]).collect();
    let triangles = std::panic::catch_unwind(|| triangles_of(&local)).ok().flatten();
    let triangles = triangles.ok_or_else(|| GeomError::InvalidInput(format!("the berm near {:.3}, {:.3} folds over itself", about[0], about[1])))?;
    Ok(TriMesh::new(outline, triangles.into_iter().map(|t| t.map(|v| v as u32)).collect()))
}

fn mirrored_line(line: &Polyline) -> Polyline {
    Polyline { vertices: line.vertices.iter().map(|p| [p[0], p[1], -p[2]]).collect(), closed: line.closed }
}

fn mirrored_mesh(mesh: &TriMesh) -> TriMesh {
    TriMesh::new(mesh.vertices.iter().map(|p| [p[0], p[1], -p[2]]).collect(), mesh.triangles.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::triangulate_grid;
    use zd_core::Grid;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    fn surface(height: impl Fn(f64, f64) -> f64) -> TriMesh {
        triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, y| height(x - E, y - N)))
    }

    fn line(points: &[[f64; 3]]) -> Polyline {
        Polyline { vertices: points.iter().map(|p| [p[0] + E, p[1] + N, p[2]]).collect(), closed: false }
    }

    fn close(a: f64, b: f64, tolerance: f64) {
        assert!((a - b).abs() <= tolerance, "{a} is not {b} to {tolerance}");
    }

    /// Tim's sequence on level surfaces, where every line has a known place: the toe on Grid A at RL 100,
    /// to Grid B at 120 and a berm, to Grid C at 140 and a berm, to the topography at 160, on to RL 200,
    /// and back down from the toe to RL 50.
    #[test]
    fn a_whole_face_runs_from_rl_to_rl_with_its_berms_where_they_belong() {
        let (a, b, c, topo) = (surface(|_, _| 100.0), surface(|_, _| 120.0), surface(|_, _| 140.0), surface(|_, _| 160.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let up = [
            FaceStep::Project { to: FaceTarget::Surface(&b), batter: 45.0 },
            FaceStep::Berm { width: 5.0 },
            FaceStep::Project { to: FaceTarget::Surface(&c), batter: 45.0 },
            FaceStep::Berm { width: 5.0 },
            FaceStep::Project { to: FaceTarget::Surface(&topo), batter: 45.0 },
            FaceStep::Project { to: FaceTarget::Rl(200.0), batter: 45.0 },
        ];
        let down = [FaceStep::Project { to: FaceTarget::Rl(50.0), batter: 45.0 }];
        let face = project_face(&toe, &a, OffsetSide::Left, &up, &down, 0.1).unwrap();

        let crests: Vec<(f64, f64)> = face.benches.iter().map(|b| (b.crest.vertices[0][1] - N, b.crest.vertices[0][2])).collect();
        for ((y, z), (want_y, want_z)) in crests.iter().zip([(120.0, 120.0), (145.0, 140.0), (170.0, 160.0), (210.0, 200.0), (50.0, 50.0)]) {
            close(*y, want_y, 1e-6);
            close(*z, want_z, 1e-6);
        }
        // The walls' slope lengths and two berms, over the 300 m toe.
        let walls = (20.0 + 20.0 + 20.0 + 40.0 + 50.0) * 2f64.sqrt();
        close(face.surface.area(), 300.0 * (walls + 5.0 + 5.0), 1e-3);
        let v = face.surface.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges, 0, "{v:?}");
        assert!(face.surface.triangles.iter().all(|&t| face.surface.triangle_cross(t)[2] > 0.0), "every triangle faces up");
        assert_eq!(crate::boundary_loops(&face.surface).unwrap().len(), 1, "one sheet, with one edge round it");
    }

    #[test]
    fn the_leg_down_carries_on_the_sheet_below_the_toe() {
        let a = surface(|x, _| 100.0 + 0.05 * x);
        let toe = register_toe(&line(&[[50.0, 200.0, 0.0], [350.0, 200.0, 0.0]]), &a).unwrap().0;
        let up = [FaceStep::Project { to: FaceTarget::Rl(180.0), batter: 60.0 }];
        let down = [FaceStep::Project { to: FaceTarget::Rl(20.0), batter: 60.0 }];
        let face = project_face(&toe, &a, OffsetSide::Right, &up, &down, 0.1).unwrap();
        // Right of an eastward string is south, so up goes south and down goes north, one plane through the toe.
        let k = 60f64.to_radians().tan();
        for bench in &face.benches {
            for p in &bench.crest.vertices {
                close(p[2], 100.0 + 0.05 * (p[0] - E) + k * (200.0 - (p[1] - N)), 1e-6);
            }
        }
        assert!(face.benches[0].crest.vertices.iter().all(|p| p[1] - N < 200.0));
        assert!(face.benches[1].crest.vertices.iter().all(|p| p[1] - N > 200.0));
    }

    #[test]
    fn a_step_with_no_height_passes_to_the_next_one() {
        let (a, lower, top) = (surface(|_, _| 100.0), surface(|_, _| 90.0), surface(|_, _| 130.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let up = [FaceStep::Project { to: FaceTarget::Surface(&lower), batter: 45.0 }, FaceStep::Project { to: FaceTarget::Surface(&top), batter: 45.0 }];
        let face = project_face(&toe, &a, OffsetSide::Left, &up, &[], 0.1).unwrap();
        assert!(face.steps[0].no_wall);
        assert_eq!(face.benches.len(), 1);
        close(face.benches[0].crest.vertices[0][1] - N, 130.0, 1e-6);
    }

    #[test]
    fn a_berm_where_the_crest_ran_along_the_string_stays_at_the_string() {
        // The target is below the toe for the middle of the string.
        let a = surface(|_, _| 100.0);
        let dipping = surface(|x, _| 90.0 + 0.5 * (x - 200.0).abs());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let up = [FaceStep::Project { to: FaceTarget::Surface(&dipping), batter: 45.0 }, FaceStep::Berm { width: 5.0 }];
        let face = project_face(&toe, &a, OffsetSide::Left, &up, &[], 0.1).unwrap();
        assert_eq!(face.steps[0].no_height.len(), 1);
        let v = face.surface.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges, 0, "{v:?}");
    }

    #[test]
    fn run_on_extends_each_end_straight() {
        let extended = run_on(&line(&[[100.0, 100.0, 5.0], [200.0, 100.0, 6.0], [200.0, 200.0, 7.0]]), [30.0, 40.0]).unwrap();
        assert_eq!(extended.vertices.len(), 5);
        close(extended.vertices[0][0] - E, 70.0, 1e-9);
        close(extended.vertices[4][1] - N, 240.0, 1e-9);
    }

    #[test]
    fn a_toe_off_its_grid_carries_on_at_the_grade_beside_it() {
        // The grid stops at x = 200 and has a hole from 100 to 140; it rises 0.1 in 1.
        let grid = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, y| {
            let (x, y) = (x - E, y - N);
            if x > 200.0 || (100.0..=140.0).contains(&x) && (50.0..=150.0).contains(&y) { f64::NAN } else { 100.0 + 0.1 * x }
        }));
        let (toe, log) = register_toe(&line(&[[20.0, 100.0, 0.0], [380.0, 100.0, 0.0]]), &grid).unwrap();
        assert!(log.off_grid > 0);
        for p in &toe.vertices {
            close(p[2], 100.0 + 0.1 * (p[0] - E), 1e-6);
        }
        assert!(register_toe(&line(&[[900.0, 100.0, 0.0], [980.0, 100.0, 0.0]]), &grid).is_err());
    }

    #[test]
    fn a_wall_bridges_a_hole_in_its_target_and_says_where() {
        let holed = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, y| {
            let (x, y) = (x - E, y - N);
            if (150.0..=250.0).contains(&x) && (60.0..=200.0).contains(&y) { f64::NAN } else { 120.0 }
        }));
        let a = surface(|_, _| 100.0);
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let face = project_face(&toe, &a, OffsetSide::Left, &[FaceStep::Project { to: FaceTarget::Surface(&holed), batter: 45.0 }], &[], 0.1).unwrap();
        assert_eq!(face.steps[0].no_target.len(), 1, "{:?}", face.steps[0]);
        let stretch = face.steps[0].no_target[0];
        assert!(stretch.from[0] - E <= 150.0 + 10.0 && stretch.to[0] - E >= 250.0 - 10.0, "{stretch:?}");
        // A level target bridges level, so the crest is unbroken at its one height and offset.
        for p in &face.benches[0].crest.vertices {
            close(p[2], 120.0, 1e-6);
            close(p[1] - N, 120.0, 1e-6);
        }
        close(face.surface.area(), 300.0 * 20.0 * 2f64.sqrt(), 1e-3);
    }

    #[test]
    fn a_wall_past_the_edge_of_its_target_holds_the_level_of_the_edge() {
        let short = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, _| if x - E > 200.0 { f64::NAN } else { 120.0 }));
        let a = surface(|_, _| 100.0);
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let face = project_face(&toe, &a, OffsetSide::Left, &[FaceStep::Project { to: FaceTarget::Surface(&short), batter: 45.0 }], &[], 0.1).unwrap();
        assert_eq!(face.steps[0].no_target.len(), 1);
        for p in &face.benches[0].crest.vertices {
            close(p[2], 120.0, 1e-6);
        }
    }

    #[test]
    fn a_target_wound_downward_gives_the_same_crest() {
        let a = surface(|_, _| 100.0);
        let target = surface(|x, y| 130.0 + 5.0 * (x / 37.0).sin() + 3.0 * (y / 23.0).cos());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = |mesh| [FaceStep::Project { to: FaceTarget::Surface(mesh), batter: 70.0 }];
        let flipped = target.flipped();
        let (up, down) = (steps(&target), steps(&flipped));
        let (normal, reversed) = (project_face(&toe, &a, OffsetSide::Left, &up, &[], 0.1).unwrap(), project_face(&toe, &a, OffsetSide::Left, &down, &[], 0.1).unwrap());
        close(normal.benches[0].crest.plan_length(), reversed.benches[0].crest.plan_length(), 1e-6);
    }
}
