//! The `.zdp` project file (the Build Plan's Store plan tab, decision 0058): a project as a set of Parquet tables
//! with a small JSON manifest, held by a container. Stage 1 writes and reads the tables through a folder; stage 2
//! puts the same entries in one zip file.
//!
//! ```text
//! manifest.json                    format and version, each table's version, layers, hidden objects, looks,
//!                                  and every other entry with its size and checksum
//! objects.parquet                  one row per live object, in listing order: id, revision, name, layer,
//!                                  provenance, kind, and where its body is
//! attributes/g00001.parquet        object (its row in objects), name, number | text | flag; 1,000 objects a group
//! meshes/g00001-vertices.parquet   x, y, z (f64): OMF2, the group's meshes end to end
//! meshes/g00001-triangles.parquet  a, b, c (u32), counted from each mesh's own first vertex
//! meshes/<id>-r<rev>-....parquet   a mesh over a million triangles, on its own
//! grids/<id>-r<rev>.parquet        z (f64, NaN = null), row-major; the lattice is in objects
//! strings.parquet                  object (its row in objects), x, y, z (f64), in vertex order
//! runlog.json                      the session's run log, as the caller passes it
//! ```
//!
//! Seam models and face designs are JSON in their object's row. Geology stays outside the file: a seam model
//! holds links to its grid files, never the grids. The same project always encodes to the same bytes, and only
//! each object's current revision is saved.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use arrow_array::{Array, ArrayRef, BooleanArray, Float64Array, RecordBatch, StringArray, UInt32Array, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Crossings, Display, FaceDesign, Grid, Layer, LayerId, Object, ObjectId, Polyline, Project, RunId, SeamModel, Solid, Surface, TriMesh};

use crate::seam::fnv1a;
use crate::store::{StoreError, saved_display};

pub const ZDP_FORMAT: &str = "zdp";
pub const ZDP_VERSION: u32 = 1;
const MANIFEST: &str = "manifest.json";
const OBJECTS: &str = "objects.parquet";
const STRINGS: &str = "strings.parquet";
const RUN_LOG: &str = "runlog.json";
/// Objects to an attribute group, and small meshes to a mesh group.
const GROUP: usize = 1000;
/// A mesh with more triangles than this goes in entries of its own.
const LARGE_MESH: usize = 1_000_000;
/// Each table's version. A table changes only by columns that may be empty, or by a new version that this
/// build's readers know; a file naming a newer one is refused.
const TABLES: [(&str, u32); 6] = [("objects", 1), ("attributes", 1), ("meshes", 1), ("grids", 1), ("strings", 1), ("runlog", 1)];

/// Where a project's entries are kept: a folder for tests, one zip file from stage 2.
pub trait Container {
    fn put(&mut self, name: &str, bytes: &[u8]) -> Result<(), StoreError>;
    fn get(&self, name: &str) -> Result<Vec<u8>, StoreError>;
}

/// Entries as files under a folder, by their names.
pub struct FolderContainer {
    dir: PathBuf,
}

impl FolderContainer {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        FolderContainer { dir: dir.into() }
    }
}

impl Container for FolderContainer {
    fn put(&mut self, name: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let path = self.dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(std::fs::write(path, bytes)?)
    }

    fn get(&self, name: &str) -> Result<Vec<u8>, StoreError> {
        std::fs::read(self.dir.join(name)).map_err(|e| StoreError::Format(format!("{name}: {e}")))
    }
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    tables: BTreeMap<String, u32>,
    objects: usize,
    layers: Vec<Layer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hidden: Vec<ObjectId>,
    #[serde(default)]
    display: Display,
    /// Every entry but the manifest, in the order written.
    entries: Vec<EntryInfo>,
}

#[derive(Serialize, Deserialize)]
struct EntryInfo {
    name: String,
    bytes: u64,
    /// 64-bit FNV-1a of the entry, as 16 hex digits.
    fnv1a: String,
}

/// What writing a project did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WriteReport {
    pub entries: usize,
    pub bytes: u64,
    pub encode_ms: f64,
    pub write_ms: f64,
}

/// A project read back.
pub struct ReadProject {
    pub project: Project,
    pub run_log: Option<String>,
    pub ms: f64,
}

/// Encodes the project and puts every entry in the container, the manifest last.
pub fn write_project(project: &Project, run_log: &str, out: &mut impl Container) -> Result<WriteReport, StoreError> {
    let start = Instant::now();
    let entries = encode_project(project, run_log)?;
    let encode_ms = start.elapsed().as_secs_f64() * 1e3;
    let start = Instant::now();
    for (name, bytes) in &entries {
        out.put(name, bytes)?;
    }
    Ok(WriteReport { entries: entries.len(), bytes: entries.iter().map(|(_, b)| b.len() as u64).sum(), encode_ms, write_ms: start.elapsed().as_secs_f64() * 1e3 })
}

/// Each object's body placed, then the mesh and grid entries to encode.
type Placement = (Vec<Placed>, Vec<Job<'static>>, Vec<Job<'static>>);

/// A mesh entry's vertices and triangles, by its stem.
type MeshParts = HashMap<String, (Vec<[f64; 3]>, Vec<[u32; 3]>)>;

/// Where an object's body goes.
enum Placed {
    String { closed: bool, vertices: usize },
    Mesh(MeshAt),
    Grid { entry: String, grid: Arc<Grid> },
    Json(String),
}

#[derive(Clone)]
struct MeshAt {
    stem: String,
    vertex_start: u64,
    vertex_count: u64,
    triangle_start: u64,
    triangle_count: u64,
}

/// One entry, or a pair, to encode.
enum Job<'a> {
    Objects,
    Strings,
    Attributes { group: usize, objects: &'a [Arc<Object>], first: usize },
    Meshes { stem: String, meshes: Vec<Arc<TriMesh>> },
    Grid { entry: String, grid: Arc<Grid> },
}

/// Every entry of the project in a fixed order, the manifest last.
pub fn encode_project(project: &Project, run_log: &str) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
    let objects: Vec<Arc<Object>> = project.objects().cloned().collect();
    let (placed, mesh_jobs, grid_jobs) = place_bodies(&objects)?;

    let mut jobs = vec![Job::Objects, Job::Strings];
    for (group, chunk) in objects.chunks(GROUP).enumerate() {
        if chunk.iter().any(|o| !o.attributes.is_empty()) {
            jobs.push(Job::Attributes { group: group + 1, objects: chunk, first: group * GROUP });
        }
    }
    jobs.extend(mesh_jobs);
    jobs.extend(grid_jobs);

    let encoded = in_parallel(jobs, |job| -> Result<Vec<(String, Vec<u8>)>, StoreError> { match job {
        Job::Objects => Ok(vec![(OBJECTS.to_string(), objects_table(&objects, &placed)?)]),
        Job::Strings => Ok(vec![(STRINGS.to_string(), strings_table(&objects)?)]),
        Job::Attributes { group, objects, first } => Ok(vec![(format!("attributes/g{group:05}.parquet"), attributes_table(objects, first)?)]),
        Job::Meshes { stem, meshes } => {
            let (vertices, triangles) = mesh_tables(&meshes)?;
            Ok(vec![(format!("{stem}-vertices.parquet"), vertices), (format!("{stem}-triangles.parquet"), triangles)])
        }
        Job::Grid { entry, grid } => Ok(vec![(entry, encode(vec![column("z", f64s(grid.z.clone()), false)])?)]),
    }});
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for part in encoded {
        entries.extend(part?);
    }
    entries.push((RUN_LOG.to_string(), run_log.as_bytes().to_vec()));

    let infos = in_parallel(entries.iter().collect::<Vec<_>>(), |(name, bytes)| EntryInfo { name: name.clone(), bytes: bytes.len() as u64, fnv1a: format!("{:016x}", fnv1a(bytes)) });
    let manifest = Manifest {
        format: ZDP_FORMAT.into(),
        version: ZDP_VERSION,
        tables: TABLES.iter().map(|(t, v)| (t.to_string(), *v)).collect(),
        objects: objects.len(),
        layers: project.layers().to_vec(),
        hidden: objects.iter().map(|o| o.id).filter(|&id| project.is_hidden(id)).collect(),
        display: saved_display(project),
        entries: infos,
    };
    entries.push((MANIFEST.to_string(), serde_json::to_vec_pretty(&manifest).map_err(|e| StoreError::Format(e.to_string()))?));
    Ok(entries)
}

/// Where each object's body goes, and the mesh and grid entries to encode. A mesh or grid that two objects
/// share is written once and both rows point at it.
fn place_bodies(objects: &[Arc<Object>]) -> Result<Placement, StoreError> {
    let mut placed = Vec::with_capacity(objects.len());
    let mut mesh_at: HashMap<usize, MeshAt> = HashMap::new();
    let mut grid_at: HashMap<usize, String> = HashMap::new();
    let (mut mesh_jobs, mut grid_jobs) = (Vec::new(), Vec::new());
    let mut group: Vec<Arc<TriMesh>> = Vec::new();
    let (mut group_number, mut vertices, mut triangles) = (1usize, 0u64, 0u64);
    let close_group = |group: &mut Vec<Arc<TriMesh>>, number: &mut usize, jobs: &mut Vec<Job<'static>>, vertices: &mut u64, triangles: &mut u64| {
        if !group.is_empty() {
            jobs.push(Job::Meshes { stem: format!("meshes/g{number:05}"), meshes: std::mem::take(group) });
            *number += 1;
            (*vertices, *triangles) = (0, 0);
        }
    };
    for object in objects {
        let entry = match &object.body {
            Body::String(line) => Placed::String { closed: line.closed, vertices: line.vertices.len() },
            Body::Surface(_) | Body::Solid(_) => {
                let mesh = match &object.body {
                    Body::Surface(s) => s.mesh_arc(),
                    Body::Solid(s) => s.mesh_arc(),
                    _ => unreachable!(),
                };
                let key = Arc::as_ptr(mesh) as usize;
                if let Some(at) = mesh_at.get(&key) {
                    Placed::Mesh(at.clone())
                } else {
                    let (v, t) = (mesh.vertices.len() as u64, mesh.triangles.len() as u64);
                    let at = if mesh.triangles.len() > LARGE_MESH {
                        let stem = format!("meshes/{}-r{}", object.id, object.revision);
                        mesh_jobs.push(Job::Meshes { stem: stem.clone(), meshes: vec![mesh.clone()] });
                        MeshAt { stem, vertex_start: 0, vertex_count: v, triangle_start: 0, triangle_count: t }
                    } else {
                        let at = MeshAt { stem: format!("meshes/g{group_number:05}"), vertex_start: vertices, vertex_count: v, triangle_start: triangles, triangle_count: t };
                        group.push(mesh.clone());
                        (vertices, triangles) = (vertices + v, triangles + t);
                        if group.len() == GROUP {
                            close_group(&mut group, &mut group_number, &mut mesh_jobs, &mut vertices, &mut triangles);
                        }
                        at
                    };
                    mesh_at.insert(key, at.clone());
                    Placed::Mesh(at)
                }
            }
            Body::Grid(grid) => {
                let key = Arc::as_ptr(grid) as usize;
                let entry = grid_at.entry(key).or_insert_with(|| {
                    let entry = format!("grids/{}-r{}.parquet", object.id, object.revision);
                    grid_jobs.push(Job::Grid { entry: entry.clone(), grid: grid.clone() });
                    entry
                });
                Placed::Grid { entry: entry.clone(), grid: grid.clone() }
            }
            Body::SeamModel(model) => Placed::Json(serde_json::to_string(&**model).map_err(|e| StoreError::Format(e.to_string()))?),
            Body::FaceDesign(design) => Placed::Json(serde_json::to_string(&**design).map_err(|e| StoreError::Format(e.to_string()))?),
        };
        placed.push(entry);
    }
    close_group(&mut group, &mut group_number, &mut mesh_jobs, &mut vertices, &mut triangles);
    Ok((placed, mesh_jobs, grid_jobs))
}

fn kind_name(body: &Body) -> &'static str {
    match body {
        Body::String(_) => "string",
        Body::Surface(_) => "surface",
        Body::Solid(_) => "solid",
        Body::Grid(_) => "grid",
        Body::SeamModel(_) => "seam_model",
        Body::FaceDesign(_) => "face_design",
    }
}

fn objects_table(objects: &[Arc<Object>], placed: &[Placed]) -> Result<Vec<u8>, StoreError> {
    let text = |f: &dyn Fn(&Object) -> String| strs(objects.iter().map(|o| Some(f(o))).collect());
    let mesh = |f: &dyn Fn(&MeshAt) -> u64| u64s(placed.iter().map(|p| if let Placed::Mesh(at) = p { Some(f(at)) } else { None }).collect());
    let grid = |f: &dyn Fn(&Grid) -> f64| f64s_opt(placed.iter().map(|p| if let Placed::Grid { grid, .. } = p { Some(f(grid)) } else { None }).collect());
    let grid_size = |f: &dyn Fn(&Grid) -> usize| u64s(placed.iter().map(|p| if let Placed::Grid { grid, .. } = p { Some(f(grid) as u64) } else { None }).collect());
    let crossings: Vec<Option<String>> = objects
        .iter()
        .map(|o| {
            let c = match &o.body {
                Body::Surface(s) => s.crossings(),
                Body::Solid(s) => s.crossings(),
                _ => None,
            };
            c.map(|c| serde_json::to_string(c).expect("crossings serialise"))
        })
        .collect();
    encode(vec![
        column("id", text(&|o| o.id.to_string()), false),
        column("revision", Arc::new(UInt32Array::from(objects.iter().map(|o| o.revision).collect::<Vec<u32>>())), false),
        column("name", text(&|o| o.name.clone()), false),
        column("layer", text(&|o| o.layer.to_string()), false),
        column("provenance", text(&|o| o.provenance.to_string()), false),
        column("kind", text(&|o| kind_name(&o.body).to_string()), false),
        column("closed", Arc::new(BooleanArray::from(placed.iter().map(|p| if let Placed::String { closed, .. } = p { Some(*closed) } else { None }).collect::<Vec<_>>())), true),
        column("vertices", u64s(placed.iter().map(|p| if let Placed::String { vertices, .. } = p { Some(*vertices as u64) } else { None }).collect()), true),
        column("mesh", strs(placed.iter().map(|p| if let Placed::Mesh(at) = p { Some(at.stem.clone()) } else { None }).collect()), true),
        column("vertex_start", mesh(&|at| at.vertex_start), true),
        column("vertex_count", mesh(&|at| at.vertex_count), true),
        column("triangle_start", mesh(&|at| at.triangle_start), true),
        column("triangle_count", mesh(&|at| at.triangle_count), true),
        column("crossings", strs(crossings), true),
        column("grid", strs(placed.iter().map(|p| if let Placed::Grid { entry, .. } = p { Some(entry.clone()) } else { None }).collect()), true),
        column("origin_x", grid(&|g| g.origin[0]), true),
        column("origin_y", grid(&|g| g.origin[1]), true),
        column("cell_x", grid(&|g| g.cell_size[0]), true),
        column("cell_y", grid(&|g| g.cell_size[1]), true),
        column("cols", grid_size(&|g| g.cols), true),
        column("rows", grid_size(&|g| g.rows), true),
        column("body", strs(placed.iter().map(|p| if let Placed::Json(json) = p { Some(json.clone()) } else { None }).collect()), true),
    ])
}

fn strings_table(objects: &[Arc<Object>]) -> Result<Vec<u8>, StoreError> {
    let (mut object, mut x, mut y, mut z) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (row, o) in objects.iter().enumerate() {
        if let Body::String(line) = &o.body {
            for p in &line.vertices {
                object.push(row as u32);
                x.push(p[0]);
                y.push(p[1]);
                z.push(p[2]);
            }
        }
    }
    encode(vec![column("object", Arc::new(UInt32Array::from(object)), false), column("x", f64s(x), false), column("y", f64s(y), false), column("z", f64s(z), false)])
}

fn attributes_table(objects: &[Arc<Object>], first: usize) -> Result<Vec<u8>, StoreError> {
    let (mut object, mut name, mut number, mut text, mut flag) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (i, o) in objects.iter().enumerate() {
        for (key, value) in &o.attributes {
            object.push((first + i) as u32);
            name.push(Some(key.clone()));
            let (n, t, f) = match value {
                AttrValue::Number(n) => (Some(*n), None, None),
                AttrValue::Text(t) => (None, Some(t.clone()), None),
                AttrValue::Bool(b) => (None, None, Some(*b)),
            };
            number.push(n);
            text.push(t);
            flag.push(f);
        }
    }
    encode(vec![
        column("object", Arc::new(UInt32Array::from(object)), false),
        column("name", strs(name), false),
        column("number", f64s_opt(number), true),
        column("text", strs(text), true),
        column("flag", Arc::new(BooleanArray::from(flag)), true),
    ])
}

fn mesh_tables(meshes: &[Arc<TriMesh>]) -> Result<(Vec<u8>, Vec<u8>), StoreError> {
    let axis = |a: usize| f64s(meshes.iter().flat_map(|m| m.vertices.iter().map(move |p| p[a])).collect());
    let corner = |c: usize| Arc::new(UInt32Array::from(meshes.iter().flat_map(|m| m.triangles.iter().map(move |t| t[c])).collect::<Vec<u32>>())) as ArrayRef;
    let vertices = encode(vec![column("x", axis(0), false), column("y", axis(1), false), column("z", axis(2), false)])?;
    let triangles = encode(vec![column("a", corner(0), false), column("b", corner(1), false), column("c", corner(2), false)])?;
    Ok((vertices, triangles))
}

fn f64s(v: Vec<f64>) -> ArrayRef {
    Arc::new(Float64Array::from(v))
}

fn f64s_opt(v: Vec<Option<f64>>) -> ArrayRef {
    Arc::new(Float64Array::from(v))
}

fn u64s(v: Vec<Option<u64>>) -> ArrayRef {
    Arc::new(UInt64Array::from(v))
}

fn strs(v: Vec<Option<String>>) -> ArrayRef {
    Arc::new(StringArray::from(v))
}

fn column(name: &str, array: ArrayRef, nullable: bool) -> (Field, ArrayRef) {
    (Field::new(name, array.data_type().clone(), nullable), array)
}

/// One record batch as Snappy-compressed Parquet in memory.
fn encode(columns: Vec<(Field, ArrayRef)>) -> Result<Vec<u8>, StoreError> {
    let (fields, arrays): (Vec<Field>, Vec<ArrayRef>) = columns.into_iter().unzip();
    let schema = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema.clone(), arrays)?;
    let properties = WriterProperties::builder().set_compression(Compression::SNAPPY).build();
    let mut out = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut out, schema, Some(properties))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(out)
}

/// Runs `f` on each item across the machine's cores, results in the items' order.
fn in_parallel<T: Send, R: Send>(items: Vec<T>, f: impl Fn(T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    let chunk = slots.len().div_ceil(threads).max(1);
    let f = &f;
    std::thread::scope(|scope| {
        let handles: Vec<_> = slots.chunks_mut(chunk).map(|part| scope.spawn(move || part.iter_mut().map(|slot| f(slot.take().expect("each item taken once"))).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().expect("an encoding thread panicked")).collect()
    })
}

/// A decoded table: its batches, named for errors.
struct Table {
    entry: String,
    batches: Vec<RecordBatch>,
}

impl Table {
    fn decode(entry: &str, bytes: Vec<u8>) -> Result<Table, StoreError> {
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(bytes))?.with_batch_size(1 << 20).build()?;
        let batches = reader.collect::<Result<Vec<_>, _>>()?;
        Ok(Table { entry: entry.to_string(), batches })
    }

    fn rows(&self) -> usize {
        self.batches.iter().map(|b| b.num_rows()).sum()
    }

    fn arrays<'a, A: Array + 'static>(&'a self, name: &str, data_type: &DataType) -> Result<Vec<&'a A>, StoreError> {
        self.batches
            .iter()
            .map(|b| {
                let array = b.column_by_name(name).ok_or_else(|| StoreError::Format(format!("{}: no column '{name}'", self.entry)))?;
                if array.data_type() != data_type {
                    return Err(StoreError::Format(format!("{}: column '{name}' is {}, expected {data_type}", self.entry, array.data_type())));
                }
                Ok(array.as_any().downcast_ref::<A>().expect("type checked"))
            })
            .collect()
    }

    fn f64s(&self, name: &str) -> Result<Vec<f64>, StoreError> {
        self.complete(name, self.f64s_opt(name)?)
    }

    fn f64s_opt(&self, name: &str) -> Result<Vec<Option<f64>>, StoreError> {
        Ok(self.arrays::<Float64Array>(name, &DataType::Float64)?.into_iter().flat_map(|a| a.iter()).collect())
    }

    fn u32s(&self, name: &str) -> Result<Vec<u32>, StoreError> {
        let values: Vec<Option<u32>> = self.arrays::<UInt32Array>(name, &DataType::UInt32)?.into_iter().flat_map(|a| a.iter()).collect();
        self.complete(name, values)
    }

    fn u64s_opt(&self, name: &str) -> Result<Vec<Option<u64>>, StoreError> {
        Ok(self.arrays::<UInt64Array>(name, &DataType::UInt64)?.into_iter().flat_map(|a| a.iter()).collect())
    }

    fn bools_opt(&self, name: &str) -> Result<Vec<Option<bool>>, StoreError> {
        Ok(self.arrays::<BooleanArray>(name, &DataType::Boolean)?.into_iter().flat_map(|a| a.iter()).collect())
    }

    fn strs_opt(&self, name: &str) -> Result<Vec<Option<String>>, StoreError> {
        Ok(self.arrays::<StringArray>(name, &DataType::Utf8)?.into_iter().flat_map(|a| a.iter().map(|s| s.map(str::to_string))).collect())
    }

    fn strs(&self, name: &str) -> Result<Vec<String>, StoreError> {
        self.complete(name, self.strs_opt(name)?)
    }

    fn complete<T>(&self, name: &str, values: Vec<Option<T>>) -> Result<Vec<T>, StoreError> {
        values.into_iter().collect::<Option<Vec<T>>>().ok_or_else(|| StoreError::Format(format!("{}: an empty value in column '{name}', which must be complete", self.entry)))
    }
}

/// Reads a project from its container: every entry is checked against the manifest's size and checksum before
/// any is used, and nothing is half read.
pub fn read_project(input: &(impl Container + Sync)) -> Result<ReadProject, StoreError> {
    let start = Instant::now();
    let manifest: Manifest = serde_json::from_slice(&input.get(MANIFEST)?).map_err(|e| StoreError::Format(format!("{MANIFEST}: {e}")))?;
    if manifest.format != ZDP_FORMAT || manifest.version > ZDP_VERSION {
        return Err(StoreError::Version { format: manifest.format, version: manifest.version });
    }
    for (table, version) in &manifest.tables {
        let known = TABLES.iter().find(|(t, _)| t == table).map(|(_, v)| *v);
        if known.is_none_or(|k| *version > k) {
            return Err(StoreError::Format(format!("the {table} table is version {version}, newer than this build reads")));
        }
    }

    let fetched = in_parallel(manifest.entries.iter().collect::<Vec<_>>(), |info| -> Result<(String, Vec<u8>), StoreError> {
        let bytes = input.get(&info.name)?;
        if bytes.len() as u64 != info.bytes || format!("{:016x}", fnv1a(&bytes)) != info.fnv1a {
            return Err(StoreError::Format(format!("{} is not as saved: {} bytes against {}, or its checksum differs", info.name, bytes.len(), info.bytes)));
        }
        Ok((info.name.clone(), bytes))
    });
    let mut entries: HashMap<String, Vec<u8>> = HashMap::new();
    for entry in fetched {
        let (name, bytes) = entry?;
        entries.insert(name, bytes);
    }
    let mut take = |name: &str| entries.remove(name).ok_or_else(|| StoreError::Format(format!("{name} is not in the manifest")));

    let objects = Table::decode(OBJECTS, take(OBJECTS)?)?;
    let rows = objects.rows();
    if rows != manifest.objects {
        return Err(StoreError::Format(format!("{OBJECTS} holds {rows} objects, the manifest {}", manifest.objects)));
    }
    let strings = Table::decode(STRINGS, take(STRINGS)?)?;
    let run_log = entries.remove(RUN_LOG).map(|b| String::from_utf8(b).map_err(|_| StoreError::Format(format!("{RUN_LOG} is not text")))).transpose()?;

    // What's left is attribute groups, meshes and grids: decoded across cores.
    let mut rest: Vec<(String, Vec<u8>)> = entries.into_iter().collect();
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    let decoded = in_parallel(rest, |(name, bytes)| Table::decode(&name, bytes).map(|t| (name, t)));
    let mut tables: HashMap<String, Table> = HashMap::new();
    for d in decoded {
        let (name, table) = d?;
        tables.insert(name, table);
    }

    let mut attributes: Vec<Attributes> = vec![Attributes::new(); rows];
    for (name, table) in tables.iter().filter(|(n, _)| n.starts_with("attributes/")) {
        let (object, key, number, text, flag) = (table.u32s("object")?, table.strs("name")?, table.f64s_opt("number")?, table.strs_opt("text")?, table.bools_opt("flag")?);
        for i in 0..object.len() {
            let value = match (number[i], text[i].clone(), flag[i]) {
                (Some(n), None, None) => AttrValue::Number(n),
                (None, Some(t), None) => AttrValue::Text(t),
                (None, None, Some(b)) => AttrValue::Bool(b),
                _ => return Err(StoreError::Format(format!("{name}: row {i} holds no value, or more than one"))),
            };
            let row = object[i] as usize;
            if row >= rows {
                return Err(StoreError::Format(format!("{name}: row {i} names object {row} of {rows}")));
            }
            attributes[row].insert(key[i].clone(), value);
        }
    }

    let mut lines: Vec<Vec<[f64; 3]>> = vec![Vec::new(); rows];
    let (owner, x, y, z) = (strings.u32s("object")?, strings.f64s("x")?, strings.f64s("y")?, strings.f64s("z")?);
    for i in 0..owner.len() {
        let row = owner[i] as usize;
        if row >= rows {
            return Err(StoreError::Format(format!("{STRINGS}: row {i} names object {row} of {rows}")));
        }
        lines[row].push([x[i], y[i], z[i]]);
    }

    // Mesh entries as plain arrays, so each object's mesh is a slice of its group.
    let mut mesh_parts: MeshParts = HashMap::new();
    for (name, table) in &tables {
        if let Some(stem) = name.strip_suffix("-vertices.parquet").filter(|_| name.starts_with("meshes/")) {
            let (x, y, z) = (table.f64s("x")?, table.f64s("y")?, table.f64s("z")?);
            mesh_parts.entry(stem.to_string()).or_default().0 = (0..x.len()).map(|i| [x[i], y[i], z[i]]).collect();
        } else if let Some(stem) = name.strip_suffix("-triangles.parquet").filter(|_| name.starts_with("meshes/")) {
            let (a, b, c) = (table.u32s("a")?, table.u32s("b")?, table.u32s("c")?);
            mesh_parts.entry(stem.to_string()).or_default().1 = (0..a.len()).map(|i| [a[i], b[i], c[i]]).collect();
        }
    }

    let (ids, revisions, names, layers, provenance, kinds) =
        (objects.strs("id")?, objects.u32s("revision")?, objects.strs("name")?, objects.strs("layer")?, objects.strs("provenance")?, objects.strs("kind")?);
    let (closed, counts, mesh) = (objects.bools_opt("closed")?, objects.u64s_opt("vertices")?, objects.strs_opt("mesh")?);
    let (vertex_start, vertex_count, triangle_start, triangle_count) =
        (objects.u64s_opt("vertex_start")?, objects.u64s_opt("vertex_count")?, objects.u64s_opt("triangle_start")?, objects.u64s_opt("triangle_count")?);
    let (crossings, grid, body) = (objects.strs_opt("crossings")?, objects.strs_opt("grid")?, objects.strs_opt("body")?);
    let (origin_x, origin_y, cell_x, cell_y, cols, grid_rows) =
        (objects.f64s_opt("origin_x")?, objects.f64s_opt("origin_y")?, objects.f64s_opt("cell_x")?, objects.f64s_opt("cell_y")?, objects.u64s_opt("cols")?, objects.u64s_opt("rows")?);

    let mut grids: HashMap<String, Arc<Grid>> = HashMap::new();
    let mut built: Vec<Object> = Vec::with_capacity(rows);
    let id = |row: usize| ObjectId::parse(&ids[row]).ok_or_else(|| StoreError::Format(format!("{OBJECTS}: row {row} has id '{}'", ids[row])));
    let needs = |row: usize, what: &str| StoreError::Format(format!("{OBJECTS}: {} '{}' (row {row}) has no {what}", kinds[row], names[row]));

    // Meshes are checked as they're made (closed and wound for a solid), so they're built across cores.
    let mesh_rows: Vec<usize> = (0..rows).filter(|&r| kinds[r] == "surface" || kinds[r] == "solid").collect();
    let made = in_parallel(mesh_rows.clone(), |row| -> Result<Body, StoreError> {
        let stem = mesh[row].as_ref().ok_or_else(|| needs(row, "mesh"))?;
        let (vertices, triangles) = mesh_parts.get(stem).ok_or_else(|| StoreError::Format(format!("{stem} is not in the manifest")))?;
        let (vs, vc, ts, tc) = (
            vertex_start[row].ok_or_else(|| needs(row, "vertex_start"))? as usize,
            vertex_count[row].ok_or_else(|| needs(row, "vertex_count"))? as usize,
            triangle_start[row].ok_or_else(|| needs(row, "triangle_start"))? as usize,
            triangle_count[row].ok_or_else(|| needs(row, "triangle_count"))? as usize,
        );
        if vs + vc > vertices.len() || ts + tc > triangles.len() {
            return Err(StoreError::Format(format!("'{}' reaches past the end of {stem}", names[row])));
        }
        let tris = triangles[ts..ts + tc].to_vec();
        if let Some(t) = tris.iter().find(|t| t.iter().any(|&i| i as usize >= vc)) {
            return Err(StoreError::Format(format!("'{}' in {stem}: triangle {t:?} refers past its {vc} vertices", names[row])));
        }
        let mesh = TriMesh::new(vertices[vs..vs + vc].to_vec(), tris);
        let crossings: Option<Crossings> = crossings[row].as_deref().map(serde_json::from_str).transpose().map_err(|e| StoreError::Format(format!("{OBJECTS}: crossings of '{}': {e}", names[row])))?;
        Ok(if kinds[row] == "surface" {
            let surface = Surface::new(mesh);
            Body::Surface(match crossings {
                Some(c) => surface.with_crossings(c),
                None => surface,
            })
        } else {
            let solid = Solid::new(mesh).map_err(|_| StoreError::Format(format!("solid '{}' in {stem} is not watertight", names[row])))?;
            Body::Solid(match crossings {
                Some(c) => solid.with_crossings(c),
                None => solid,
            })
        })
    });
    let mut made: HashMap<usize, Result<Body, StoreError>> = mesh_rows.into_iter().zip(made).collect();

    for row in 0..rows {
        let body = match kinds[row].as_str() {
            "string" => {
                let vertices = std::mem::take(&mut lines[row]);
                let expected = counts[row].ok_or_else(|| needs(row, "vertex count"))? as usize;
                if vertices.len() != expected {
                    return Err(StoreError::Format(format!("string '{}' should have {expected} vertices but {STRINGS} holds {}", names[row], vertices.len())));
                }
                Body::String(Polyline { vertices, closed: closed[row].ok_or_else(|| needs(row, "closed"))? })
            }
            "surface" | "solid" => made.remove(&row).expect("built above")?,
            "grid" => {
                let entry = grid[row].as_ref().ok_or_else(|| needs(row, "grid entry"))?;
                let shape = (origin_x[row], origin_y[row], cell_x[row], cell_y[row], cols[row], grid_rows[row]);
                let (Some(ox), Some(oy), Some(cx), Some(cy), Some(nc), Some(nr)) = shape else { return Err(needs(row, "lattice")) };
                let grid = match grids.get(entry) {
                    Some(g) => g.clone(),
                    None => {
                        let z = tables.get(entry).ok_or_else(|| StoreError::Format(format!("{entry} is not in the manifest")))?.f64s("z")?;
                        if z.len() != (nc * nr) as usize || cx <= 0.0 || cy <= 0.0 {
                            return Err(StoreError::Format(format!("{entry} does not match its {nc} x {nr} lattice")));
                        }
                        let g = Arc::new(Grid::new([ox, oy], [cx, cy], nc as usize, nr as usize, z));
                        grids.insert(entry.clone(), g.clone());
                        g
                    }
                };
                Body::Grid(grid)
            }
            kind @ ("seam_model" | "face_design") => {
                let json = body[row].as_deref().ok_or_else(|| needs(row, "body"))?;
                let bad = |e: serde_json::Error| StoreError::Format(format!("{OBJECTS}: {kind} '{}': {e}", names[row]));
                if kind == "seam_model" {
                    Body::SeamModel(Arc::new(serde_json::from_str::<SeamModel>(json).map_err(bad)?))
                } else {
                    Body::FaceDesign(Arc::new(serde_json::from_str::<FaceDesign>(json).map_err(bad)?))
                }
            }
            other => return Err(StoreError::Format(format!("{OBJECTS}: '{}' is of a kind this build doesn't know: {other}", names[row]))),
        };
        let parse_layer = LayerId::parse(&layers[row]).ok_or_else(|| needs(row, "layer id"))?;
        let parse_run = RunId::parse(&provenance[row]).ok_or_else(|| needs(row, "provenance"))?;
        built.push(Object {
            id: id(row)?,
            revision: revisions[row],
            name: names[row].clone(),
            layer: parse_layer,
            provenance: parse_run,
            attributes: std::mem::take(&mut attributes[row]),
            body,
        });
    }
    let project = Project::from_saved(manifest.layers, built).map_err(StoreError::Project)?.with_hidden(manifest.hidden).with_display(manifest.display);
    Ok(ReadProject { project, run_log, ms: start.elapsed().as_secs_f64() * 1e3 })
}

/// Where two projects differ in anything a save keeps, one line each; empty when they're the same. Coordinates,
/// numbers and grid nodes are compared bit for bit, nulls included.
pub fn project_differences(a: &Project, b: &Project) -> Vec<String> {
    let mut out = Vec::new();
    if a.layers() != b.layers() {
        out.push("layers differ".to_string());
    }
    let (oa, ob): (Vec<_>, Vec<_>) = (a.objects().collect(), b.objects().collect());
    if oa.len() != ob.len() {
        out.push(format!("{} objects against {}", oa.len(), ob.len()));
    }
    let bits = |p: &[[f64; 3]]| p.iter().flat_map(|v| v.map(f64::to_bits)).collect::<Vec<u64>>();
    let same_attr = |x: &Attributes, y: &Attributes| {
        x.len() == y.len()
            && x.iter().zip(y).all(|((kx, vx), (ky, vy))| {
                kx == ky
                    && match (vx, vy) {
                        (AttrValue::Number(p), AttrValue::Number(q)) => p.to_bits() == q.to_bits(),
                        _ => vx == vy,
                    }
            })
    };
    for (x, y) in oa.iter().zip(&ob) {
        let say = |what: &str| format!("'{}' ({}): {what}", x.name, x.id);
        if (x.id, x.revision, &x.name, x.layer, x.provenance) != (y.id, y.revision, &y.name, y.layer, y.provenance) {
            out.push(say("id, revision, name, layer or provenance differ"));
        }
        if !same_attr(&x.attributes, &y.attributes) {
            out.push(say("attributes differ"));
        }
        if a.is_hidden(x.id) != b.is_hidden(y.id) {
            out.push(say("hidden in one only"));
        }
        let same_mesh = |p: &TriMesh, q: &TriMesh| bits(&p.vertices) == bits(&q.vertices) && p.triangles == q.triangles;
        let same = match (&x.body, &y.body) {
            (Body::String(p), Body::String(q)) => p.closed == q.closed && bits(&p.vertices) == bits(&q.vertices),
            (Body::Surface(p), Body::Surface(q)) => same_mesh(p.mesh(), q.mesh()) && p.validity() == q.validity() && p.crossings() == q.crossings(),
            (Body::Solid(p), Body::Solid(q)) => same_mesh(p.mesh(), q.mesh()) && p.validity() == q.validity() && p.crossings() == q.crossings(),
            (Body::Grid(p), Body::Grid(q)) => {
                (p.origin.map(f64::to_bits), p.cell_size.map(f64::to_bits), p.cols, p.rows) == (q.origin.map(f64::to_bits), q.cell_size.map(f64::to_bits), q.cols, q.rows)
                    && p.z.iter().map(|v| v.to_bits()).eq(q.z.iter().map(|v| v.to_bits()))
            }
            (Body::SeamModel(p), Body::SeamModel(q)) => p == q,
            (Body::FaceDesign(p), Body::FaceDesign(q)) => p == q,
            _ => false,
        };
        if !same {
            out.push(say("body differs"));
        }
    }
    if saved_display(a) != saved_display(b) {
        out.push("looks differ".to_string());
    }
    out
}
