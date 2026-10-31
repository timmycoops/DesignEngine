//! A whole face (Phase 3): a toe registered on its grid, then a list of steps going one way from it, each
//! a wall to a surface or RL or a berm. Built oversized on purpose: it runs on past the pit and the cut
//! does the fitting (decision 0023).
//!
//! Each wall comes from `project_wall`, and each row says whether its line is conditioned and, for a berm,
//! registered, before the next one. Steps at one batter with no berm between them are one wall: a face
//! projected to the topography and on to an RL is the same plane carrying straight through, not a new wall
//! from a rough line. Going down is the same thing upside down: the sheet carries on below the toe on the
//! far side, so heights are mirrored and the side swapped, and a wall that "rises" there is going down.
//! Plan winding doesn't change, so the sheet stays wound upward.

use std::collections::HashMap;

use zd_core::{FaceDirection, Polyline, TriMesh};

use crate::string::TriangleBins;
use crate::wall::{Stretch, at_crest_heights, next_line, triangles_of, with_crest_vertices};
use crate::{GeomError, OffsetSide, drape_string, filter_string, next_toe, project_wall};

/// What a wall rises to.
#[derive(Debug, Clone, Copy)]
pub enum FaceTarget<'a> {
    Surface(&'a TriMesh),
    Rl(f64),
}

/// One step of a face.
#[derive(Debug, Clone, Copy)]
pub enum FaceStep<'a> {
    /// A wall at a batter in degrees. With a tolerance, its crest is conditioned for the next step.
    Project { to: FaceTarget<'a>, batter: f64, condition: Option<f64> },
    /// A berm of a width in metres. Its line registers on `on`, or on what the last wall reached if that's
    /// left out. With `register` off it keeps its crest's heights instead. With a tolerance it's
    /// conditioned after it's moved out.
    Berm { on: Option<FaceTarget<'a>>, width: f64, register: bool, condition: Option<f64> },
}

/// One wall of a face.
#[derive(Debug, Clone)]
pub struct FaceBench {
    /// Where it is in the steps, from 0.
    pub step: usize,
    pub toe: Polyline,
    pub crest: Polyline,
}

/// What one step did.
#[derive(Debug, Clone)]
pub struct StepReport {
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

/// A toe's plan shape simplified to within `tolerance` metres, ready to be registered. Its ends stay.
pub fn conditioned_toe(line: &Polyline, tolerance: f64) -> Result<Polyline, GeomError> {
    filter_string(&Polyline { vertices: line.vertices.iter().map(|v| [v[0], v[1], 0.0]).collect(), closed: line.closed }, tolerance)
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

/// The whole face from a toe already on its grid, going one way. `register` is the surface the toe lies on.
pub fn project_face(toe: &Polyline, register: &TriMesh, side: OffsetSide, direction: FaceDirection, steps: &[FaceStep<'_>]) -> Result<Face, GeomError> {
    let mut face = FaceParts::default();
    match direction {
        FaceDirection::Up => run_steps(toe, register, side, steps, &mut face)?,
        FaceDirection::Down => {
            let (toe, surface) = (mirrored_line(toe), mirrored_mesh(register));
            let swapped = if side == OffsetSide::Left { OffsetSide::Right } else { OffsetSide::Left };
            let surfaces: Vec<Option<TriMesh>> = steps
                .iter()
                .map(|step| match target_of(step) {
                    Some(FaceTarget::Surface(mesh)) => Some(mirrored_mesh(mesh)),
                    _ => None,
                })
                .collect();
            let mirrored: Vec<FaceStep<'_>> = steps
                .iter()
                .zip(&surfaces)
                .map(|(step, mesh)| match *step {
                    FaceStep::Project { to, batter, condition } => FaceStep::Project { to: mirrored_target(to, mesh.as_ref()), batter, condition },
                    FaceStep::Berm { on, width, register, condition } => FaceStep::Berm { on: on.map(|on| mirrored_target(on, mesh.as_ref())), width, register, condition },
                })
                .collect();
            run_steps(&toe, &surface, swapped, &mirrored, &mut face)?;
            face.mirror();
        }
    }
    Ok(Face { surface: face.joined(), benches: face.benches, steps: face.steps })
}

/// What a step goes to, where it names one.
fn target_of<'a>(step: &FaceStep<'a>) -> Option<FaceTarget<'a>> {
    match step {
        FaceStep::Project { to, .. } => Some(*to),
        FaceStep::Berm { on, .. } => *on,
    }
}

fn mirrored_target<'a>(target: FaceTarget<'_>, mesh: Option<&'a TriMesh>) -> FaceTarget<'a> {
    match (target, mesh) {
        (FaceTarget::Rl(rl), _) => FaceTarget::Rl(-rl),
        (FaceTarget::Surface(_), Some(mesh)) => FaceTarget::Surface(mesh),
        (FaceTarget::Surface(_), None) => unreachable!("every surface is mirrored first"),
    }
}

/// What the steps have made so far.
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

    /// One mesh, sharing vertices within a micrometre. A wall and a berm work out the same point apart, so
    /// the two can land either side of a rounding step: each point looks in the cells round its own.
    fn joined(&self) -> TriMesh {
        let mut out = TriMesh::default();
        let mut numbered: HashMap<[i64; 3], u32> = HashMap::new();
        for mesh in &self.meshes {
            let renumbered: Vec<u32> = mesh
                .vertices
                .iter()
                .map(|p| {
                    let cell = p.map(|c| (c / WELD).round() as i64);
                    let same = |v: &u32| out.vertices[*v as usize].iter().zip(p).all(|(a, b)| (a - b).abs() <= WELD);
                    cells_round(cell).find_map(|c| numbered.get(&c).filter(|v| same(v)).copied()).unwrap_or_else(|| {
                        out.vertices.push(*p);
                        numbered.insert(cell, out.vertices.len() as u32 - 1);
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

/// A level sheet at an RL over a string's plan area, as a target.
fn level_sheet(line: &Polyline, rl: f64, reach: f64) -> TriMesh {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &line.vertices {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    let (x0, y0, x1, y1) = (lo[0] - reach, lo[1] - reach, hi[0] + reach, hi[1] + reach);
    TriMesh::new(vec![[x0, y0, rl], [x1, y0, rl], [x1, y1, rl], [x0, y1, rl]], vec![[0, 1, 2], [0, 2, 3]])
}

/// A crest waiting to become the next toe, and how its row conditions it.
struct Waiting {
    crest: Polyline,
    condition: Option<f64>,
}

fn run_steps<'a>(toe: &Polyline, register: &'a TriMesh, side: OffsetSide, steps: &[FaceStep<'a>], face: &mut FaceParts) -> Result<(), GeomError> {
    let mut from = toe.clone();
    let mut waiting: Option<Waiting> = None;
    // The crest a berm starts from. The berm waits for the wall above it, as that wall's crest can come
    // down onto `from` part way along a segment.
    let mut berm_from: Option<Polyline> = None;
    let mut on = Resting::Surface(register);
    let fail = |k: usize, e: GeomError| GeomError::InvalidInput(format!("step {}: {e}", k + 1));
    let mut k = 0;
    while k < steps.len() {
        match steps[k] {
            FaceStep::Project { batter, .. } => {
                if let Some(Waiting { crest, condition }) = waiting.take() {
                    from = match condition.filter(|t| *t > 0.0) {
                        // A conditioned line weaves either side of its crest, so with no berm here it
                        // stands clear of it by the tolerance, which can only widen what's there.
                        Some(tolerance) => {
                            let (toes, _) = next_toe(&crest, tolerance, side, tolerance, on.mesh()).map_err(|e| fail(k, e))?;
                            berm_from = Some(crest);
                            longest(toes)
                        }
                        None => crest,
                    };
                }
                // Steps at one batter with no berm between them are one wall carrying straight on: the
                // targets on the way only mark where its crest lines fall.
                let wall: Vec<(usize, FaceTarget<'a>, Option<f64>)> = (k..steps.len())
                    .map_while(|j| match steps[j] {
                        FaceStep::Project { to, batter: b, condition } if b == batter => Some((j, to, condition)),
                        _ => None,
                    })
                    .collect();
                let (&(last, to, condition), marks) = wall.split_last().expect("this step at least");
                for &(j, to, _) in marks {
                    let target = Resting::of(to, &from, batter);
                    match project_wall(&from, side, batter, target.mesh()) {
                        Ok(wall) => {
                            face.steps.push(wall_report(j, &wall));
                            face.benches.push(FaceBench { step: j, toe: from.clone(), crest: wall.crest });
                        }
                        // The wall carries on through here whatever this target does.
                        Err(_) => face.steps.push(no_wall_report(j)),
                    }
                }
                let target = Resting::of(to, &from, batter);
                match project_wall(&from, side, batter, target.mesh()) {
                    Ok(wall) => {
                        face.steps.push(wall_report(last, &wall));
                        let toe = if wall.log.no_height.is_empty() { from.clone() } else { with_touches(&from, &wall.crest) };
                        finish_berm(&mut berm_from, &toe, face).map_err(|e| fail(last, e))?;
                        face.benches.push(FaceBench { step: last, toe, crest: wall.crest.clone() });
                        face.meshes.push(wall.wall);
                        waiting = Some(Waiting { crest: wall.crest, condition });
                        on = target;
                    }
                    Err(GeomError::NoHeight) => {
                        // Nothing to build here: the sheet carries on from the same string.
                        face.steps.push(no_wall_report(last));
                        finish_berm(&mut berm_from, &from, face).map_err(|e| fail(last, e))?;
                        waiting = Some(Waiting { crest: from.clone(), condition });
                    }
                    Err(e) => return Err(fail(last, e)),
                }
                k = last + 1;
            }
            FaceStep::Berm { on: named, width, register, condition } => {
                let Waiting { crest, condition: before } = waiting.take().unwrap_or_else(|| Waiting { crest: from.clone(), condition: None });
                finish_berm(&mut berm_from, &from, face).map_err(|e| fail(k, e))?;
                if let Some(target) = named {
                    on = Resting::of(target, &crest, BERM_BATTER);
                }
                let (lines, _) = next_line(&crest, width, side, before.unwrap_or(0.0), condition.unwrap_or(0.0)).map_err(|e| fail(k, e))?;
                let dropped = lines.len().saturating_sub(1);
                let line = longest(lines);
                let mut report = no_wall_report(k);
                from = if register {
                    let (mut toe, draped) = drape_string(&line, on.mesh(), true).map_err(|e| fail(k, e))?;
                    hold_above_the_target(&mut toe, &crest, on.mesh());
                    report.off_surface = draped.off_surface;
                    toe
                } else {
                    at_crest_heights(&with_crest_vertices(&line, &crest, |_, on, _| Some(on)), &crest)
                };
                berm_from = Some(crest);
                (report.no_wall, report.pieces_dropped) = (false, dropped);
                face.steps.push(report);
                k += 1;
            }
        }
    }
    finish_berm(&mut berm_from, &from, face).map_err(|e| fail(steps.len().saturating_sub(1), e))?;
    // The last crest goes nowhere, so conditioning it only cleans the string: the sheet keeps its own edge.
    if let Some(Waiting { crest, condition: Some(tolerance) }) = waiting
        && tolerance > 0.0
        && let Some(bench) = face.benches.last_mut().filter(|b| b.step + 1 == steps.len())
    {
        let (lines, _) = next_toe(&crest, 0.0, side, tolerance, on.mesh()).map_err(|e| fail(bench.step, e))?;
        bench.crest = longest(lines);
    }
    Ok(())
}

/// The berm waiting on a crest, now the line it runs out to is known.
fn finish_berm(berm_from: &mut Option<Polyline>, line: &Polyline, face: &mut FaceParts) -> Result<(), GeomError> {
    if let Some(crest) = berm_from.take() {
        face.meshes.push(band(&crest, line)?);
    }
    Ok(())
}

/// A toe with a vertex wherever its crest touches it, so the berm below meets the wall and the crest above
/// there with no slit between them.
fn with_touches(toe: &Polyline, crest: &Polyline) -> Polyline {
    with_crest_vertices(toe, crest, |vertex, on, gap| (gap <= WELD && (vertex[2] - on[2]).abs() <= WELD).then_some(vertex))
}

/// The batter a level sheet for a berm is sized for, flat enough to reach past any berm.
const BERM_BATTER: f64 = 5.0;

/// How close two points of a face are to be the same point (m).
const WELD: f64 = 1e-6;

/// A cell of the weld's lattice and those touching it.
fn cells_round([x, y, z]: [i64; 3]) -> impl Iterator<Item = [i64; 3]> {
    (-1..=1).flat_map(move |i| (-1..=1).flat_map(move |j| (-1..=1).map(move |k| [x + i, y + j, z + k])))
}

/// What a line lies on or a wall rises to.
enum Resting<'a> {
    Surface(&'a TriMesh),
    Level(TriMesh),
}

impl<'a> Resting<'a> {
    /// A target, for a wall from `from` at a batter.
    fn of(target: FaceTarget<'a>, from: &Polyline, batter: f64) -> Self {
        match target {
            FaceTarget::Surface(mesh) => Resting::Surface(mesh),
            FaceTarget::Rl(rl) => Resting::Level(level_sheet(from, rl, sheet_reach(from, rl, batter))),
        }
    }

    fn mesh(&self) -> &TriMesh {
        match self {
            Resting::Surface(mesh) => mesh,
            Resting::Level(sheet) => sheet,
        }
    }
}

fn wall_report(step: usize, wall: &crate::WallStep) -> StepReport {
    StepReport {
        walls: wall.log.walls,
        squeezed_out: wall.log.squeezed_out,
        no_wall: false,
        no_height: wall.log.no_height.clone(),
        no_target: wall.log.no_target.clone(),
        ..no_wall_report(step)
    }
}

fn no_wall_report(step: usize) -> StepReport {
    StepReport { step, walls: 0, squeezed_out: 0, no_wall: true, no_height: Vec::new(), no_target: Vec::new(), off_surface: 0, pieces_dropped: 0 }
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

    /// What's left of a line's plan shape simplified to a centimetre.
    fn plan_corners(line: &Polyline) -> usize {
        conditioned_toe(line, 0.01).unwrap().vertices.len()
    }

    /// A wall and a berm as the table starts them, conditioned at 0.1 m.
    fn wall<'a>(to: FaceTarget<'a>, batter: f64) -> FaceStep<'a> {
        FaceStep::Project { to, batter, condition: Some(0.1) }
    }

    fn berm<'a>(width: f64) -> FaceStep<'a> {
        FaceStep::Berm { on: None, width, register: true, condition: Some(0.1) }
    }

    /// Tim's sequence on level surfaces, where every line has a known place: the toe on Grid A at RL 100,
    /// to Grid B at 120 and a berm, to Grid C at 140 and a berm, to the topography at 160, on to RL 200.
    #[test]
    fn a_whole_face_runs_up_with_its_berms_where_they_belong() {
        let (a, b, c, topo) = (surface(|_, _| 100.0), surface(|_, _| 120.0), surface(|_, _| 140.0), surface(|_, _| 160.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = [wall(FaceTarget::Surface(&b), 45.0), berm(5.0), wall(FaceTarget::Surface(&c), 45.0), berm(5.0), wall(FaceTarget::Surface(&topo), 45.0), wall(FaceTarget::Rl(200.0), 45.0)];
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps).unwrap();

        let crests: Vec<(f64, f64)> = face.benches.iter().map(|b| (b.crest.vertices[0][1] - N, b.crest.vertices[0][2])).collect();
        assert_eq!(crests.len(), 4);
        for ((y, z), (want_y, want_z)) in crests.iter().zip([(120.0, 120.0), (145.0, 140.0), (170.0, 160.0), (210.0, 200.0)]) {
            close(*y, want_y, 1e-6);
            close(*z, want_z, 1e-6);
        }
        // The walls' slope lengths and two berms, over the 300 m toe.
        let walls = (20.0 + 20.0 + 20.0 + 40.0) * 2f64.sqrt();
        close(face.surface.area(), 300.0 * (walls + 5.0 + 5.0), 1e-3);
        let v = face.surface.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges, 0, "{v:?}");
        assert!(face.surface.triangles.iter().all(|&t| face.surface.triangle_cross(t)[2] > 0.0), "every triangle faces up");
        assert_eq!(crate::boundary_loops(&face.surface).unwrap().len(), 1, "one sheet, with one edge round it");
    }

    #[test]
    fn going_down_carries_on_the_same_plane_below_the_toe() {
        let a = surface(|x, _| 100.0 + 0.05 * x);
        let toe = register_toe(&line(&[[50.0, 200.0, 0.0], [350.0, 200.0, 0.0]]), &a).unwrap().0;
        let up = project_face(&toe, &a, OffsetSide::Right, FaceDirection::Up, &[wall(FaceTarget::Rl(180.0), 60.0)]).unwrap();
        let down = project_face(&toe, &a, OffsetSide::Right, FaceDirection::Down, &[wall(FaceTarget::Rl(20.0), 60.0)]).unwrap();
        // Right of an eastward string is south, so up goes south and down goes north, one plane through the toe.
        let k = 60f64.to_radians().tan();
        let on_the_plane = |p: &[f64; 3]| close(p[2], 100.0 + 0.05 * (p[0] - E) + k * (200.0 - (p[1] - N)), 1e-6);
        up.benches[0].crest.vertices.iter().for_each(on_the_plane);
        down.benches[0].crest.vertices.iter().for_each(on_the_plane);
        assert!(up.benches[0].crest.vertices.iter().all(|p| p[1] - N < 200.0));
        assert!(down.benches[0].crest.vertices.iter().all(|p| p[1] - N > 200.0));
        assert!(down.surface.triangles.iter().all(|&t| down.surface.triangle_cross(t)[2] > 0.0), "going down still faces up");
    }

    /// Down from a level toe through a level grid and a berm on it, to an RL.
    #[test]
    fn going_down_steps_through_a_grid_and_a_berm() {
        let (a, b) = (surface(|_, _| 100.0), surface(|_, _| 80.0));
        let toe = line(&[[50.0, 200.0, 100.0], [350.0, 200.0, 100.0]]);
        let steps = [wall(FaceTarget::Surface(&b), 45.0), berm(5.0), wall(FaceTarget::Rl(50.0), 45.0)];
        // Left of an eastward string is north, so a wall rising left goes down to the south.
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Down, &steps).unwrap();
        close(face.benches[0].crest.vertices[0][1] - N, 180.0, 1e-6);
        close(face.benches[0].crest.vertices[0][2], 80.0, 1e-6);
        close(face.benches[1].crest.vertices[0][1] - N, 145.0, 1e-6);
        close(face.benches[1].crest.vertices[0][2], 50.0, 1e-6);
        close(face.surface.area(), 300.0 * ((20.0 + 30.0) * 2f64.sqrt() + 5.0), 1e-3);
    }

    #[test]
    fn a_step_with_no_height_passes_to_the_next_one() {
        let (a, lower, top) = (surface(|_, _| 100.0), surface(|_, _| 90.0), surface(|_, _| 130.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(&lower), 45.0), wall(FaceTarget::Surface(&top), 45.0)]).unwrap();
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
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(&dipping), 45.0), berm(5.0)]).unwrap();
        assert_eq!(face.steps[0].no_height.len(), 1);
        let v = face.surface.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges, 0, "{v:?}");
    }

    #[test]
    fn a_berm_over_a_hole_holds_the_height_of_its_crest() {
        // A kink in the toe puts a vertex of the berm line over the hole.
        let holed = triangulate_grid(&Grid::from_fn([E, N], [10.0, 10.0], 41, 41, |x, y| {
            let (x, y) = (x - E, y - N);
            if (150.0..=250.0).contains(&x) && (60.0..=200.0).contains(&y) { f64::NAN } else { 120.0 }
        }));
        let a = surface(|_, _| 100.0);
        let toe = line(&[[50.0, 100.0, 100.0], [200.0, 80.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = [wall(FaceTarget::Surface(&holed), 45.0), berm(5.0), wall(FaceTarget::Rl(140.0), 45.0)];
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps).unwrap();
        assert!(face.steps.iter().any(|s| s.off_surface > 0), "{:?}", face.steps);
        for p in &face.benches[1].toe.vertices {
            close(p[2], 120.0, 1e-6);
        }
    }

    /// A berm with Register off keeps its crest's heights: on a target with a cross fall the line stays at
    /// the crest, and along a rolling target it follows the crest between its own two ends.
    #[test]
    fn a_berm_with_register_off_keeps_the_heights_of_its_crest() {
        let a = surface(|_, _| 100.0);
        let rolling = surface(|x, y| 120.0 + 0.2 * (y - 120.0) + 4.0 * (x / 41.0).sin());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = |register| [wall(FaceTarget::Surface(&rolling), 60.0), FaceStep::Berm { on: None, width: 10.0, register, condition: Some(0.1) }, wall(FaceTarget::Rl(160.0), 60.0)];
        let held = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps(false)).unwrap();
        let crest = &held.benches[0].crest;
        let next = &held.benches[1].toe;
        let height_of = |line: &Polyline, x: f64| {
            let w = line.vertices.windows(2).find(|w| (w[0][0] - x) * (w[1][0] - x) <= 0.0).unwrap();
            w[0][2] + (x - w[0][0]) / (w[1][0] - w[0][0]) * (w[1][2] - w[0][2])
        };
        let (first, last) = (crest.vertices[0][0].max(next.vertices[0][0]), crest.vertices[crest.vertices.len() - 1][0].min(next.vertices[next.vertices.len() - 1][0]));
        for x in (0..=100).map(|k| first + (last - first) * f64::from(k) / 100.0) {
            // The crest runs off straight in plan only by its small swing, so the two read the same to 10 cm.
            close(height_of(next, x), height_of(crest, x), 0.1);
        }
        let registered = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps(true)).unwrap();
        // Registered, the same line sits 10 m further up the 1 in 5 cross fall, 2 m higher.
        let middle = E + 200.0;
        close(height_of(&registered.benches[1].toe, middle) - height_of(next, middle), 2.0, 0.1);
    }

    #[test]
    fn a_berm_can_name_the_surface_or_rl_it_registers_on() {
        let (a, b, c) = (surface(|_, _| 100.0), surface(|_, _| 120.0), surface(|_, _| 125.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        for (on, want) in [(FaceTarget::Surface(&c), 125.0), (FaceTarget::Rl(123.0), 123.0)] {
            let steps = [wall(FaceTarget::Surface(&b), 45.0), FaceStep::Berm { on: Some(on), width: 5.0, register: true, condition: None }, wall(FaceTarget::Rl(160.0), 45.0)];
            let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps).unwrap();
            for p in &face.benches[1].toe.vertices {
                close(p[2], want, 1e-6);
                close(p[1] - N, 125.0, 1e-6);
            }
        }
    }

    /// A rolling target gives a crest that weaves in plan. Each row's tick decides what happens to it.
    #[test]
    fn each_row_says_whether_its_line_is_conditioned() {
        let a = surface(|_, _| 100.0);
        let rolling = surface(|x, y| 120.0 + 3.0 * (x / 29.0).sin() + 2.0 * (y / 17.0).cos());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let corners_of_next_toe = |crest: Option<f64>, after: Option<f64>| {
            let steps = [FaceStep::Project { to: FaceTarget::Surface(&rolling), batter: 60.0, condition: crest }, FaceStep::Berm { on: None, width: 10.0, register: true, condition: after }, wall(FaceTarget::Rl(170.0), 60.0)];
            let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps).unwrap();
            plan_corners(&face.benches[1].toe)
        };
        let (raw, cleaned, after_only) = (corners_of_next_toe(None, None), corners_of_next_toe(Some(0.5), None), corners_of_next_toe(None, Some(0.5)));
        assert!(cleaned * 3 < raw, "{cleaned} against {raw}");
        assert!(after_only * 3 < raw, "{after_only} against {raw}");
    }

    #[test]
    fn a_change_of_batter_with_conditioning_off_starts_on_the_crest() {
        let (a, b) = (surface(|_, _| 100.0), surface(|_, _| 120.0));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = |condition| [FaceStep::Project { to: FaceTarget::Surface(&b), batter: 45.0, condition }, wall(FaceTarget::Rl(140.0), 60.0)];
        let walls = 20.0 * 2f64.sqrt() + 20.0 / 60f64.to_radians().sin();
        let on_the_crest = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps(None)).unwrap();
        close(on_the_crest.surface.area(), 300.0 * walls, 1e-3);
        let clear = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps(Some(0.1))).unwrap();
        close(clear.surface.area(), 300.0 * (walls + 0.1), 1e-3);
    }

    #[test]
    fn conditioning_between_walls_of_one_batter_changes_nothing() {
        let (a, topo) = (surface(|_, _| 100.0), surface(|x, y| 130.0 + 3.0 * (x / 29.0).sin() + 2.0 * (y / 17.0).cos()));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let run = |condition| {
            let steps = [FaceStep::Project { to: FaceTarget::Surface(&topo), batter: 70.0, condition }, wall(FaceTarget::Rl(200.0), 70.0)];
            project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps).unwrap()
        };
        let (on, off) = (run(Some(1.0)), run(None));
        assert_eq!(on.surface.triangles.len(), off.surface.triangles.len());
        close(on.surface.area(), off.surface.area(), 1e-9);
        close(on.surface.area(), 300.0 * 100.0 / 70f64.to_radians().sin(), 1e-3);
    }

    #[test]
    fn conditioning_the_last_crest_cleans_the_string_and_leaves_the_sheet_alone() {
        let (a, rolling) = (surface(|_, _| 100.0), surface(|x, y| 130.0 + 3.0 * (x / 29.0).sin() + 2.0 * (y / 17.0).cos()));
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let run = |condition| project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[FaceStep::Project { to: FaceTarget::Surface(&rolling), batter: 70.0, condition }]).unwrap();
        let (raw, clean) = (run(None), run(Some(1.0)));
        // The cleaned crest is laid back on the target with a vertex at every triangle, so it's its plan
        // shape that's simpler.
        let (rough, cleaned) = (plan_corners(&raw.benches[0].crest), plan_corners(&clean.benches[0].crest));
        assert!(cleaned * 2 < rough, "{cleaned} against {rough}");
        assert_eq!(raw.surface.triangles.len(), clean.surface.triangles.len());
        close(raw.surface.area(), clean.surface.area(), 1e-9);
    }

    #[test]
    fn a_conditioned_toe_keeps_its_ends_and_stays_within_the_tolerance() {
        let wavy = line(&[[0.0, 100.0, 9.0], [50.0, 100.05, 9.0], [100.0, 99.95, 9.0], [150.0, 100.2, 9.0], [200.0, 100.0, 9.0]]);
        let cleaned = conditioned_toe(&wavy, 0.1).unwrap();
        assert_eq!(cleaned.vertices.len(), 4, "the 5 cm wiggle goes and the larger two stay");
        assert_eq!((cleaned.vertices[0][0], cleaned.vertices[3][0]), (wavy.vertices[0][0], wavy.vertices[4][0]));
        assert_eq!(conditioned_toe(&wavy, 0.5).unwrap().vertices.len(), 2);
    }

    /// A wall up to a target that dips below its toe in the middle, 2 m at `centre`, with a berm either side
    /// of it, so the sheet carries on over the stretch with no height. The last wall is steeper because a
    /// target with no cross-fall puts a line moved square off its crest in the plane of the wall below, and
    /// at that same batter neighbouring walls are one plane, with no hip between them. That's refused.
    fn over_a_dip(centre: f64) -> Face {
        let (a, b) = (surface(|_, _| 100.0), surface(|_, _| 120.0));
        let dipping = surface(|x, _| 118.0 + 0.025 * (x - centre).abs());
        let toe = line(&[[50.0, 100.0, 100.0], [350.0, 100.0, 100.0]]);
        let steps = [wall(FaceTarget::Surface(&b), 45.0), berm(5.0), wall(FaceTarget::Surface(&dipping), 45.0), berm(5.0), wall(FaceTarget::Rl(150.0), 60.0)];
        project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &steps).unwrap()
    }

    fn assert_one_sheet_over_the_dip(face: &Face, centre: f64) {
        assert_eq!(face.steps[2].no_height.len(), 1, "{:?}", face.steps[2]);
        let stretch = face.steps[2].no_height[0];
        close(stretch.from[0].min(stretch.to[0]) - E, centre - 80.0, 1e-6);
        close(stretch.from[0].max(stretch.to[0]) - E, centre + 80.0, 1e-6);
        let v = face.surface.validity();
        assert_eq!(v.non_manifold_edges + v.inconsistent_edges, 0, "{v:?}");
        assert_eq!(crate::boundary_loops(&face.surface).unwrap().len(), 1, "one sheet, with one edge round it");
    }

    /// The crest comes down onto its toe between two of the toe's vertices, and rises again further on.
    #[test]
    fn a_crest_touching_its_toe_part_way_along_a_segment_leaves_no_slit() {
        assert_one_sheet_over_the_dip(&over_a_dip(203.7), 203.7);
    }

    /// The target meets the toe on grid lines, where the toe has its vertices, as grids that pinch together do.
    #[test]
    fn a_crest_landing_exactly_on_a_string_vertex_carries_on() {
        assert_one_sheet_over_the_dip(&over_a_dip(200.0), 200.0);
    }

    /// Two triangles that share an edge, worked out a whisker apart either side of a rounding step.
    #[test]
    fn a_face_welds_across_a_rounding_step() {
        let step = 2.5e-6;
        let (below, above) = (step - 1e-12, step + 1e-12);
        let left = TriMesh::new(vec![[0.0, 0.0, 0.0], [below, 10.0, 0.0], [-5.0, 5.0, 0.0]], vec![[0, 1, 2]]);
        let right = TriMesh::new(vec![[0.0, 0.0, 0.0], [5.0, 5.0, 0.0], [above, 10.0, 0.0]], vec![[0, 1, 2]]);
        let joined = FaceParts { meshes: vec![left, right], ..FaceParts::default() }.joined();
        assert_eq!(joined.vertices.len(), 4);
        assert_eq!(crate::boundary_loops(&joined).unwrap().len(), 1);
    }

    #[test]
    fn points_further_apart_than_the_weld_stay_apart() {
        let left = TriMesh::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], vec![[0, 1, 2]]);
        let right = TriMesh::new(vec![[3e-6, 0.0, 0.0], [1.0, 3e-6, 0.0], [1.0, 1.0, 0.0]], vec![[0, 1, 2]]);
        assert_eq!(FaceParts { meshes: vec![left, right], ..FaceParts::default() }.joined().vertices.len(), 6);
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
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(&holed), 45.0)]).unwrap();
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
        let face = project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(&short), 45.0)]).unwrap();
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
        let flipped = target.flipped();
        let run = |mesh| project_face(&toe, &a, OffsetSide::Left, FaceDirection::Up, &[wall(FaceTarget::Surface(mesh), 70.0)]).unwrap();
        let (normal, reversed) = (run(&target), run(&flipped));
        close(normal.benches[0].crest.plan_length(), reversed.benches[0].crest.plan_length(), 1e-6);
    }
}
