use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{Grid, LayerId, MeshValidity, ObjectId, RunId, TriMesh};

/// What kind of thing an object is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    String,
    Surface,
    Solid,
    Grid,
}

/// A string: ordered f64 vertices, open or closed.
#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    pub vertices: Vec<[f64; 3]>,
    pub closed: bool,
}

/// A triangulated surface with its validity flags, computed once when it is made.
#[derive(Debug, Clone)]
pub struct Surface {
    mesh: Arc<TriMesh>,
    validity: MeshValidity,
}

impl Surface {
    pub fn new(mesh: TriMesh) -> Self {
        let validity = mesh.validity();
        Surface { mesh: Arc::new(mesh), validity }
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
}

impl Solid {
    /// The mesh as a solid, or its validity flags if it is not watertight.
    pub fn new(mesh: TriMesh) -> Result<Self, MeshValidity> {
        let validity = mesh.validity();
        if validity.is_watertight() { Ok(Solid { mesh: Arc::new(mesh), validity }) } else { Err(validity) }
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
}

impl Body {
    pub fn kind(&self) -> ObjectKind {
        match self {
            Body::String(_) => ObjectKind::String,
            Body::Surface(_) => ObjectKind::Surface,
            Body::Solid(_) => ObjectKind::Solid,
            Body::Grid(_) => ObjectKind::Grid,
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
