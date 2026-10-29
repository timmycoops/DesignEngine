use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{Crossings, Grid, LayerId, MeshValidity, ObjectId, RunId, SeamModel, TriMesh};

/// What kind of thing an object is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    String,
    Surface,
    Solid,
    Grid,
    SeamModel,
}

/// A string: ordered f64 vertices, open or closed.
#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    pub vertices: Vec<[f64; 3]>,
    pub closed: bool,
}

impl Polyline {
    /// The segments in order, including the closing segment of a closed string.
    fn segments(&self) -> impl Iterator<Item = ([f64; 3], [f64; 3])> + '_ {
        let n = self.vertices.len();
        let count = if self.closed && n > 2 { n } else { n.saturating_sub(1) };
        (0..count).map(move |i| (self.vertices[i], self.vertices[(i + 1) % n]))
    }

    /// Length along the string in 3D (slope length).
    pub fn length(&self) -> f64 {
        self.segments()
            .map(|(a, b)| ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt())
            .sum()
    }

    /// Length in plan, ignoring elevation.
    pub fn plan_length(&self) -> f64 {
        self.segments().map(|(a, b)| (b[0] - a[0]).hypot(b[1] - a[1])).sum()
    }

    /// Plan area enclosed by a closed string (shoelace), positive whatever the direction;
    /// `None` for an open string or fewer than three vertices. Coordinates are taken about
    /// the first vertex so mine grid eastings and northings keep their precision.
    pub fn plan_area(&self) -> Option<f64> {
        if !self.closed || self.vertices.len() < 3 {
            return None;
        }
        let o = self.vertices[0];
        let twice: f64 = self
            .segments()
            .map(|(a, b)| (a[0] - o[0]) * (b[1] - o[1]) - (b[0] - o[0]) * (a[1] - o[1]))
            .sum();
        Some(twice.abs() / 2.0)
    }
}

/// A triangulated surface with its validity flags, computed once when it is made, and the result
/// of its crossing check once one has run.
#[derive(Debug, Clone)]
pub struct Surface {
    mesh: Arc<TriMesh>,
    validity: MeshValidity,
    crossings: Option<Arc<Crossings>>,
}

impl Surface {
    pub fn new(mesh: TriMesh) -> Self {
        let validity = mesh.validity();
        Surface { mesh: Arc::new(mesh), validity, crossings: None }
    }

    /// The surface with the result of its crossing check, which sets `NonSelfIntersecting`.
    pub fn with_crossings(mut self, crossings: Crossings) -> Self {
        self.validity.non_self_intersecting = Some(crossings.pairs == 0);
        self.crossings = Some(Arc::new(crossings));
        self
    }

    /// The crossing check's result; `None` if none has run.
    pub fn crossings(&self) -> Option<&Crossings> {
        self.crossings.as_deref()
    }

    pub fn mesh(&self) -> &TriMesh {
        &self.mesh
    }

    /// The shared mesh: revisions that keep the geometry share this.
    pub fn mesh_arc(&self) -> &Arc<TriMesh> {
        &self.mesh
    }

    pub fn validity(&self) -> MeshValidity {
        self.validity
    }
}

/// A closed, manifold, consistently wound mesh: the volume of record. It cannot be made otherwise.
#[derive(Debug, Clone)]
pub struct Solid {
    mesh: Arc<TriMesh>,
    validity: MeshValidity,
    crossings: Option<Arc<Crossings>>,
}

impl Solid {
    /// The mesh as a solid, or its validity flags if it is not watertight.
    pub fn new(mesh: TriMesh) -> Result<Self, MeshValidity> {
        let validity = mesh.validity();
        if validity.is_watertight() { Ok(Solid { mesh: Arc::new(mesh), validity, crossings: None }) } else { Err(validity) }
    }

    /// The solid with the result of its crossing check, which sets `NonSelfIntersecting`.
    pub fn with_crossings(mut self, crossings: Crossings) -> Self {
        self.validity.non_self_intersecting = Some(crossings.pairs == 0);
        self.crossings = Some(Arc::new(crossings));
        self
    }

    /// The crossing check's result; `None` if none has run.
    pub fn crossings(&self) -> Option<&Crossings> {
        self.crossings.as_deref()
    }

    pub fn mesh(&self) -> &TriMesh {
        &self.mesh
    }

    /// The shared mesh: revisions that keep the geometry share this.
    pub fn mesh_arc(&self) -> &Arc<TriMesh> {
        &self.mesh
    }

    pub fn validity(&self) -> MeshValidity {
        self.validity
    }

    pub fn volume(&self) -> f64 {
        self.mesh.signed_volume()
    }
}

/// An object's geometry. Meshes and grids are shared, so a revision that only renames an object
/// does not copy them.
#[derive(Debug, Clone)]
pub enum Body {
    String(Polyline),
    Surface(Surface),
    Solid(Solid),
    Grid(Arc<Grid>),
    /// The stack definition and its check results; the grids stay in their files.
    SeamModel(Arc<SeamModel>),
}

impl Body {
    pub fn kind(&self) -> ObjectKind {
        match self {
            Body::String(_) => ObjectKind::String,
            Body::Surface(_) => ObjectKind::Surface,
            Body::Solid(_) => ObjectKind::Solid,
            Body::Grid(_) => ObjectKind::Grid,
            Body::SeamModel(_) => ObjectKind::SeamModel,
        }
    }
}

/// An attribute value on an object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttrValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

pub type Attributes = BTreeMap<String, AttrValue>;

/// One revision of an object. Revisions never change once made; an edit makes the next one.
#[derive(Debug, Clone)]
pub struct Object {
    pub id: ObjectId,
    /// 1 for the object as created, then one more per edit.
    pub revision: u32,
    pub name: String,
    pub layer: LayerId,
    /// The command run that made this revision.
    pub provenance: RunId,
    pub attributes: Attributes,
    pub body: Body,
}

impl Object {
    pub fn kind(&self) -> ObjectKind {
        self.body.kind()
    }
}

/// A named layer. Visibility and colour are display state, not part of the undo history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub colour: [u8; 3],
    pub visible: bool,
}

/// One object revision, as a run record lists its inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectRef {
    pub id: ObjectId,
    pub revision: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 500_000.0;
    const N: f64 = 7_000_000.0;

    /// A 100 m square in plan at mine grid coordinates, with its east side 30 m lower.
    fn square(closed: bool) -> Polyline {
        let vertices = vec![[E, N, 100.0], [E + 100.0, N, 70.0], [E + 100.0, N + 100.0, 70.0], [E, N + 100.0, 100.0]];
        Polyline { vertices, closed }
    }

    #[test]
    fn closed_square_lengths_and_area() {
        let s = square(true);
        // Plan: four 100 m sides. Slope: two sides drop 30 m over 100 m, so each is sqrt(100² + 30²).
        assert!((s.plan_length() - 400.0).abs() < 1e-9);
        assert!((s.length() - (200.0 + 2.0 * 10_900f64.sqrt())).abs() < 1e-9);
        // Exact to 1e-6 m² at MGA coordinates; reversing the direction gives the same area.
        assert!((s.plan_area().unwrap() - 10_000.0).abs() < 1e-6);
        let mut reversed = s.clone();
        reversed.vertices.reverse();
        assert!((reversed.plan_area().unwrap() - 10_000.0).abs() < 1e-6);
    }

    #[test]
    fn open_string_has_no_closing_segment_or_area() {
        let s = square(false);
        assert!((s.plan_length() - 300.0).abs() < 1e-9);
        assert_eq!(s.plan_area(), None);
        let point = Polyline { vertices: vec![[E, N, 0.0]], closed: true };
        assert_eq!(point.length(), 0.0);
        assert_eq!(point.plan_area(), None);
    }
}
