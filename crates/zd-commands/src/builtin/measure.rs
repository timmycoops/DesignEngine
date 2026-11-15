//! `solid.measure` (decision 0074): each block's geometry written onto it as attributes, before a reserves dataflow
//! reads it. Tim, 15 November 2026: "The dataflow - will just utilize what is inside the data table (the block length
//! would come from an interrogation step essentially)", so it can be checked, and corrected in the dataflow's
//! parameters where a solid gives something odd. As Spry's Imported fields, every block, coal and waste.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Body, Change, ObjectId, TriMesh, attribute_ci, attribute_word};

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// What a measure writes, after any prefix: the solid's volume (m³), its footprint in plan (m²), its vertical thickness
/// (volume ÷ footprint, m), its length and width (the sides of the smallest rectangle round its footprint, m, the longer
/// first), its centroid, its dip (degrees, as its seam's floor and roof lean, a block's benches of a seam sharing one)
/// and its true thickness (the vertical × cos dip, m: what seam rules and loss and dilution thicknesses are about; Tim:
/// "true thickness very important").
pub const MEASURES: [&str; 10] = ["volume", "plan_area", "thickness", "length", "width", "centroid_x", "centroid_y", "centroid_z", "dip", "true_thickness"];

pub struct Measure;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasureParams {
    pub solids: Vec<ObjectId>,
    /// Put in front of each attribute's name.
    #[serde(default)]
    pub prefix: String,
}

impl Command for Measure {
    type Params = MeasureParams;
    const NAME: &'static str = "solid.measure";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Write each block's geometry onto it: volume, plan area, vertical and true thickness, length, width, dip and centroid",
            inputs: vec![Field::new("solids", FieldType::Objects, "The blocks")],
            params: vec![Field::new("prefix", FieldType::Text, "Put in front of each attribute's name")],
            outputs: vec![],
        }
    }

    fn run(&self, inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let mut changes = Vec::with_capacity(p.solids.len());
        let (mut volume, mut area) = (0.0, 0.0);
        // Each solid's measures and the leaning of its seam faces, then a block's benches of a seam given one dip: the
        // seam's, read from all their faces, so a bench between two levels, flat top and bottom, dips as its seam does.
        let mut measured = Vec::with_capacity(p.solids.len());
        let mut leaning: BTreeMap<(String, String), [[f64; 3]; 2]> = BTreeMap::new();
        for &id in &p.solids {
            let object = inputs.object(id)?;
            let Body::Solid(solid) = &object.body else {
                return Err(CommandError::WrongKind { id, expected: zd_core::ObjectKind::Solid, found: object.kind() });
            };
            let word = |k: &str| attribute_ci(&object.attributes, k).map(attribute_word);
            let group = match (word("benched_from"), word("block")) {
                (Some(from), Some(block)) => (from, block),
                _ => (id.to_string(), String::new()),
            };
            let lean = leanings(solid.mesh());
            let g = leaning.entry(group.clone()).or_default();
            for (sum, part) in g.iter_mut().zip(lean) {
                for k in 0..3 {
                    sum[k] += part[k];
                }
            }
            measured.push((object, measure(solid.mesh()), group));
        }
        for (object, mut m, group) in measured {
            let id = object.id;
            let [seam, level] = leaning[&group];
            let dip = dip_of(if seam[2] != 0.0 { seam } else { level });
            m[8] = dip;
            m[9] = m[2] * dip.to_radians().cos();
            volume += m[0];
            area += m[1];
            let mut attributes = object.attributes.clone();
            for (name, value) in MEASURES.iter().zip(m) {
                attributes.insert(format!("{}{name}", p.prefix), AttrValue::Number(value));
            }
            changes.push(Change::Revise { id, base: object.revision, name: None, layer: None, attributes: Some(attributes), body: None });
        }
        let n = changes.len();
        Ok(Outcome::new(changes).message(format!(
            "measured {n} block{}: {volume:.1} m3 over {area:.1} m2 in plan; {} written onto each",
            if n == 1 { "" } else { "s" },
            MEASURES.iter().map(|m| format!("{}{m}", p.prefix)).collect::<Vec<_>>().join(", ")
        )))
    }
}

/// A solid's measures, in `MEASURES` order.
pub fn measure(mesh: &TriMesh) -> [f64; 10] {
    let volume = mesh.signed_volume();
    let area = plan_area(mesh);
    let thickness = if area > 0.0 { volume / area } else { 0.0 };
    let (length, width) = footprint_sides(mesh);
    let c = centroid(mesh);
    let dip = floor_dip(mesh);
    [volume, area, thickness, length, width, c[0], c[1], c[2], dip, thickness * dip.to_radians().cos()]
}

/// A solid's dip (degrees) on its own, as its seam faces lean.
pub fn floor_dip(mesh: &TriMesh) -> f64 {
    let [seam, level] = leanings(mesh);
    dip_of(if seam[2] != 0.0 { seam } else { level })
}

/// How a solid's floor and roof faces lean: the sums of their normals, each as long as twice its area, all turned to
/// face down, the seam's apart from those exactly level. A floor or roof is a face within 60° of level; walls are left
/// out, so a block's own sides and a strip's face don't count, and v1 seams dip under 30°. A face exactly level is a bench
/// or RL cut, not the seam: a dip is read from the seam's faces where there are any, else from the level ones, flat.
fn leanings(mesh: &TriMesh) -> [[f64; 3]; 2] {
    let (mut seam, mut level) = ([0.0f64; 3], [0.0f64; 3]);
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        let mut n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len == 0.0 || n[2].abs() < 0.5 * len {
            continue;
        }
        if n[2] > 0.0 {
            n = [-n[0], -n[1], -n[2]];
        }
        let target = if n[0].hypot(n[1]) <= 1e-9 * len { &mut level } else { &mut seam };
        for k in 0..3 {
            target[k] += n[k];
        }
    }
    [seam, level]
}

/// The dip (degrees) of a downward leaning; none, 0.
fn dip_of(n: [f64; 3]) -> f64 {
    if n[2] == 0.0 {
        return 0.0;
    }
    n[0].hypot(n[1]).atan2(-n[2]).to_degrees()
}

/// The plan area of a solid: its upward-facing triangles in plan, about its first vertex so mine coordinates keep their
/// precision. For a block, whose walls are vertical, its footprint.
pub fn plan_area(mesh: &TriMesh) -> f64 {
    let Some(&o) = mesh.vertices.first() else { return 0.0 };
    let twice: f64 = mesh
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| {
                let v = mesh.vertices[i as usize];
                [v[0] - o[0], v[1] - o[1]]
            });
            ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).max(0.0)
        })
        .sum();
    twice / 2.0
}

/// A block's length in plan: the longer side of the smallest rectangle round its footprint.
pub fn block_length(mesh: &TriMesh) -> f64 {
    footprint_sides(mesh).0
}

/// The sides of the smallest rectangle round a footprint, the longer first, from the convex hull of its vertices about
/// the first so mine coordinates keep their precision.
fn footprint_sides(mesh: &TriMesh) -> (f64, f64) {
    let Some(&o) = mesh.vertices.first() else { return (0.0, 0.0) };
    let mut points: Vec<[f64; 2]> = mesh.vertices.iter().map(|v| [v[0] - o[0], v[1] - o[1]]).collect();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 2 {
        return (0.0, 0.0);
    }
    // Andrew's monotone chain.
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut hull: Vec<[f64; 2]> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        let ordered: Box<dyn Iterator<Item = &[f64; 2]>> = if pass == 0 { Box::new(points.iter()) } else { Box::new(points.iter().rev()) };
        for &p in ordered {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    if hull.len() < 2 {
        return (0.0, 0.0);
    }
    // The smallest rectangle has a side along a hull edge.
    let mut best = (f64::INFINITY, 0.0, 0.0);
    for i in 0..hull.len() {
        let (a, b) = (hull[i], hull[(i + 1) % hull.len()]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = dx.hypot(dy);
        if len == 0.0 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (mut lo_u, mut hi_u, mut lo_v, mut hi_v) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for p in &hull {
            let (u, v) = (p[0] * ux + p[1] * uy, -p[0] * uy + p[1] * ux);
            lo_u = lo_u.min(u);
            hi_u = hi_u.max(u);
            lo_v = lo_v.min(v);
            hi_v = hi_v.max(v);
        }
        let (w, h) = (hi_u - lo_u, hi_v - lo_v);
        if w * h < best.0 {
            best = (w * h, w.max(h), w.min(h));
        }
    }
    (best.1, best.2)
}

/// A closed solid's centroid: its tetrahedra to the bounding box's centre, weighted by their volumes.
fn centroid(mesh: &TriMesh) -> [f64; 3] {
    let Some(bounds) = mesh.bounds() else { return [f64::NAN; 3] };
    let c = bounds.centre();
    let (mut sum, mut six_v) = ([0.0; 3], 0.0);
    for t in &mesh.triangles {
        let [a, b, d] = t.map(|i| {
            let p = mesh.vertices[i as usize];
            [p[0] - c[0], p[1] - c[1], p[2] - c[2]]
        });
        let v = a[0] * (b[1] * d[2] - b[2] * d[1]) - a[1] * (b[0] * d[2] - b[2] * d[0]) + a[2] * (b[0] * d[1] - b[1] * d[0]);
        six_v += v;
        for k in 0..3 {
            sum[k] += v * (a[k] + b[k] + d[k]) / 4.0;
        }
    }
    if six_v == 0.0 {
        return c;
    }
    [c[0] + sum[0] / six_v, c[1] + sum[1] / six_v, c[2] + sum[2] / six_v]
}
