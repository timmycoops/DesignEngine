//! The project store: a project saved as a `<name>.zdproj/` folder (decision 0004).
//!
//! ```text
//! <name>.zdproj/
//!   manifest.json                          format, version, layers, and every live object with
//!                                          its revision, layer, provenance, attributes and files
//!   run-log-<save>.json                    the session's run log, as the caller passes it
//!   strings-<save>.parquet                 all string vertices: object (u32, index into the
//!                                          manifest's objects), x, y, z (f64), in vertex order
//!   objects/<id>-r<rev>-vertices.parquet   x, y, z (f64): OMF2 vertex array (surfaces, solids)
//!   objects/<id>-r<rev>-triangles.parquet  a, b, c (u32): OMF2 triangle array
//!   objects/<id>-r<rev>-grid.parquet       z (f64, NaN = null), row-major; lattice in the manifest
//! ```
//!
//! A seam model is written whole into the manifest: it holds its stack definition, the fingerprints
//! of the grid files it links to and its check results, never the grids themselves.
//!
//! Object files are named by the object and the revision that first wrote them. Revisions never
//! change, so a later save reuses those files and rewrites only what changed: renaming a surface or
//! moving a string vertex never rewrites a large mesh. A save writes new files first and swaps the
//! manifest in last, so a crash mid-save leaves the previous save intact. Afterwards it deletes files
//! the new manifest no longer names, but only files of the shapes above, never anything else.
//!
//! Only each object's current revision is saved; the undo history does not survive closing.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use arrow_array::{Array, ArrayRef, Float64Array, RecordBatch, UInt32Array};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use zd_core::{
    Attributes, Body, Crossings, FaceDesign, Grid, Layer, LayerId, Object, ObjectId, Polyline, Project, ProjectError, RunId, SeamModel, Solid,
    Surface, TriMesh,
};

const FORMAT: &str = "zdproj";
const VERSION: u32 = 1;
const MANIFEST: &str = "manifest.json";
const OBJECTS: &str = "objects";

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Parquet(String),
    /// A file that reads but does not hold what the manifest says.
    Format(String),
    /// A manifest from another format or a newer version.
    Version { format: String, version: u32 },
    Project(ProjectError),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "{e}"),
            StoreError::Parquet(message) => write!(f, "parquet: {message}"),
            StoreError::Format(message) => write!(f, "{message}"),
            StoreError::Version { format, version } => {
                write!(f, "not a project this build can open: format '{format}' version {version}")
            }
            StoreError::Project(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<parquet::errors::ParquetError> for StoreError {
    fn from(e: parquet::errors::ParquetError) -> Self {
        StoreError::Parquet(e.to_string())
    }
}

impl From<arrow_schema::ArrowError> for StoreError {
    fn from(e: arrow_schema::ArrowError) -> Self {
        StoreError::Parquet(e.to_string())
    }
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    saved_unix_ms: u64,
    #[serde(default)]
    run_log: Option<String>,
    #[serde(default)]
    strings: Option<String>,
    layers: Vec<Layer>,
    objects: Vec<StoredObject>,
}

#[derive(Serialize, Deserialize)]
struct StoredObject {
    id: ObjectId,
    revision: u32,
    name: String,
    layer: LayerId,
    provenance: RunId,
    #[serde(default)]
    attributes: Attributes,
    body: StoredBody,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StoredBody {
    /// `vertices` rows of the strings file carry this object's index.
    String { closed: bool, vertices: usize },
    Surface {
        vertices: String,
        triangles: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crossings: Option<Crossings>,
    },
    Solid {
        vertices: String,
        triangles: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crossings: Option<Crossings>,
    },
    Grid { origin: [f64; 2], cell_size: [f64; 2], cols: usize, rows: usize, z: String },
    SeamModel { model: SeamModel },
    FaceDesign { design: FaceDesign },
}

/// Files already on disk for a shared mesh or grid, so a later save can point at them again.
#[derive(Clone)]
enum Written {
    Mesh { _mesh: Arc<TriMesh>, vertices: String, triangles: String },
    Grid { _grid: Arc<Grid>, z: String },
}

/// What a save did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SaveReport {
    /// Files written under `objects/`.
    pub object_files_written: usize,
    /// Files under `objects/` the new manifest reuses from earlier saves.
    pub object_files_reused: usize,
    /// Stale files removed after the manifest swap.
    pub files_deleted: usize,
    pub bytes_written: u64,
    pub ms: f64,
}

/// A project folder, and what this session has already written to it.
pub struct ProjectStore {
    dir: PathBuf,
    written: HashMap<usize, Written>,
}

/// A project read back from its folder.
pub struct Opened {
    pub project: Project,
    /// The run log as it was saved, if any.
    pub run_log: Option<String>,
    /// The store, ready to save back incrementally.
    pub store: ProjectStore,
    pub ms: f64,
}

impl ProjectStore {
    /// A store that will save into `dir` (created if needed). Its first save writes every object.
    pub fn create(dir: impl Into<PathBuf>) -> Result<ProjectStore, StoreError> {
        let dir = dir.into();
        std::fs::create_dir_all(dir.join(OBJECTS))?;
        Ok(ProjectStore { dir, written: HashMap::new() })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Reads a project folder.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Opened, StoreError> {
        let start = Instant::now();
        let dir = dir.into();
        let manifest: Manifest = serde_json::from_slice(&std::fs::read(dir.join(MANIFEST))?)
            .map_err(|e| StoreError::Format(format!("{MANIFEST}: {e}")))?;
        if manifest.format != FORMAT || manifest.version != VERSION {
            return Err(StoreError::Version { format: manifest.format, version: manifest.version });
        }

        let mut strings = match &manifest.strings {
            Some(file) => read_string_rows(&dir.join(file), manifest.objects.len())?,
            None => HashMap::new(),
        };
        let mut store = ProjectStore { dir: dir.clone(), written: HashMap::new() };
        let mut objects = Vec::with_capacity(manifest.objects.len());
        let mut meshes = read_meshes(&dir, &manifest.objects)?;
        for (index, stored) in manifest.objects.into_iter().enumerate() {
            let body = match stored.body {
                StoredBody::String { closed, vertices } => {
                    let rows = strings.remove(&index).unwrap_or_default();
                    if rows.len() != vertices {
                        return Err(StoreError::Format(format!(
                            "string '{}' should have {vertices} vertices but the strings file holds {}",
                            stored.name,
                            rows.len()
                        )));
                    }
                    Body::String(Polyline { vertices: rows, closed })
                }
                StoredBody::Surface { vertices, triangles, crossings } => {
                    let mut surface = Surface::new(meshes[index].take().expect("a surface's mesh was read"));
                    if let Some(crossings) = crossings {
                        surface = surface.with_crossings(crossings);
                    }
                    store.remember_mesh(surface.mesh_arc(), vertices, triangles);
                    Body::Surface(surface)
                }
                StoredBody::Solid { vertices, triangles, crossings } => {
                    let mut solid = Solid::new(meshes[index].take().expect("a solid's mesh was read")).map_err(|_| {
                        StoreError::Format(format!("solid '{}' is not watertight in {vertices}", stored.name))
                    })?;
                    if let Some(crossings) = crossings {
                        solid = solid.with_crossings(crossings);
                    }
                    store.remember_mesh(solid.mesh_arc(), vertices, triangles);
                    Body::Solid(solid)
                }
                StoredBody::Grid { origin, cell_size, cols, rows, z } => {
                    let values = read_f64_columns(&dir.join(&z), &["z"])?.remove(0);
                    if values.len() != cols * rows || cell_size[0] <= 0.0 || cell_size[1] <= 0.0 {
                        return Err(StoreError::Format(format!("{z} does not match its {cols} x {rows} lattice")));
                    }
                    let grid = Arc::new(Grid::new(origin, cell_size, cols, rows, values));
                    store.written.insert(Arc::as_ptr(&grid) as usize, Written::Grid { _grid: grid.clone(), z });
                    Body::Grid(grid)
                }
                StoredBody::SeamModel { model } => Body::SeamModel(Arc::new(model)),
                StoredBody::FaceDesign { design } => Body::FaceDesign(Arc::new(design)),
            };
            objects.push(Object {
                id: stored.id,
                revision: stored.revision,
                name: stored.name,
                layer: stored.layer,
                provenance: stored.provenance,
                attributes: stored.attributes,
                body,
            });
        }
        let project = Project::from_saved(manifest.layers, objects).map_err(StoreError::Project)?;
        let run_log = match &manifest.run_log {
            Some(file) => Some(std::fs::read_to_string(dir.join(file))?),
            None => None,
        };
        Ok(Opened { project, run_log, store, ms: start.elapsed().as_secs_f64() * 1e3 })
    }

    /// Saves the project's live objects and the run log. New files go down first, the manifest is
    /// swapped in last, then stale files are removed.
    pub fn save(&mut self, project: &Project, run_log: &str) -> Result<SaveReport, StoreError> {
        let start = Instant::now();
        let mut report = SaveReport::default();
        let objects_dir = self.dir.join(OBJECTS);
        std::fs::create_dir_all(&objects_dir)?;
        let save = RunId::new().to_string();

        let mut written = HashMap::new();
        let mut stored = Vec::new();
        let mut string_rows: (Vec<u32>, Vec<[f64; 3]>) = (Vec::new(), Vec::new());
        for (index, object) in project.objects().enumerate() {
            let body = match &object.body {
                Body::String(line) => {
                    string_rows.0.extend(std::iter::repeat_n(index as u32, line.vertices.len()));
                    string_rows.1.extend_from_slice(&line.vertices);
                    StoredBody::String { closed: line.closed, vertices: line.vertices.len() }
                }
                Body::Surface(surface) => {
                    let (vertices, triangles) = self.mesh_files(object, surface.mesh_arc(), &mut written, &mut report)?;
                    StoredBody::Surface { vertices, triangles, crossings: surface.crossings().cloned() }
                }
                Body::Solid(solid) => {
                    let (vertices, triangles) = self.mesh_files(object, solid.mesh_arc(), &mut written, &mut report)?;
                    StoredBody::Solid { vertices, triangles, crossings: solid.crossings().cloned() }
                }
                Body::Grid(grid) => {
                    let z = self.grid_file(object, grid, &mut written, &mut report)?;
                    StoredBody::Grid { origin: grid.origin, cell_size: grid.cell_size, cols: grid.cols, rows: grid.rows, z }
                }
                Body::SeamModel(model) => StoredBody::SeamModel { model: (**model).clone() },
                Body::FaceDesign(design) => StoredBody::FaceDesign { design: (**design).clone() },
            };
            stored.push(StoredObject {
                id: object.id,
                revision: object.revision,
                name: object.name.clone(),
                layer: object.layer,
                provenance: object.provenance,
                attributes: object.attributes.clone(),
                body,
            });
        }

        let strings = if string_rows.0.is_empty() {
            None
        } else {
            let file = format!("strings-{save}.parquet");
            report.bytes_written += write_string_rows(&self.dir.join(&file), &string_rows.0, &string_rows.1)?;
            Some(file)
        };
        let run_log_file = format!("run-log-{save}.json");
        std::fs::write(self.dir.join(&run_log_file), run_log)?;
        report.bytes_written += run_log.len() as u64;

        let manifest = Manifest {
            format: FORMAT.into(),
            version: VERSION,
            saved_unix_ms: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64),
            run_log: Some(run_log_file.clone()),
            strings: strings.clone(),
            layers: project.layers().to_vec(),
            objects: stored,
        };
        let text = serde_json::to_vec_pretty(&manifest).map_err(|e| StoreError::Format(e.to_string()))?;
        let staging = self.dir.join(format!("{MANIFEST}.tmp"));
        std::fs::write(&staging, &text)?;
        std::fs::rename(&staging, self.dir.join(MANIFEST))?;
        report.bytes_written += text.len() as u64;

        let mut keep: HashSet<String> = HashSet::new();
        keep.insert(run_log_file);
        keep.extend(strings);
        for files in written.values() {
            match files {
                Written::Mesh { vertices, triangles, .. } => {
                    keep.insert(vertices.clone());
                    keep.insert(triangles.clone());
                }
                Written::Grid { z, .. } => {
                    keep.insert(z.clone());
                }
            }
        }
        report.files_deleted = self.remove_stale(&keep)?;
        self.written = written;
        report.ms = start.elapsed().as_secs_f64() * 1e3;
        Ok(report)
    }

    fn remember_mesh(&mut self, mesh: &Arc<TriMesh>, vertices: String, triangles: String) {
        self.written.insert(Arc::as_ptr(mesh) as usize, Written::Mesh { _mesh: mesh.clone(), vertices, triangles });
    }

    /// The files holding a mesh: reused if this store already wrote that shared mesh, else written.
    fn mesh_files(
        &self,
        object: &Object,
        mesh: &Arc<TriMesh>,
        written: &mut HashMap<usize, Written>,
        report: &mut SaveReport,
    ) -> Result<(String, String), StoreError> {
        let key = Arc::as_ptr(mesh) as usize;
        // Already placed in this save (two objects sharing one mesh).
        if let Some(Written::Mesh { vertices, triangles, .. }) = written.get(&key) {
            return Ok((vertices.clone(), triangles.clone()));
        }
        // Written by an earlier save and still on disk.
        if let Some(prior @ Written::Mesh { vertices, triangles, .. }) = self.written.get(&key)
            && self.dir.join(vertices).exists()
            && self.dir.join(triangles).exists()
        {
            report.object_files_reused += 2;
            written.insert(key, prior.clone());
            return Ok((vertices.clone(), triangles.clone()));
        }
        let vertices = format!("{OBJECTS}/{}-r{}-vertices.parquet", object.id, object.revision);
        let triangles = format!("{OBJECTS}/{}-r{}-triangles.parquet", object.id, object.revision);
        report.bytes_written += write_points(&self.dir.join(&vertices), &mesh.vertices)?;
        report.bytes_written += write_triangles(&self.dir.join(&triangles), &mesh.triangles)?;
        report.object_files_written += 2;
        written.insert(key, Written::Mesh { _mesh: mesh.clone(), vertices: vertices.clone(), triangles: triangles.clone() });
        Ok((vertices, triangles))
    }

    fn grid_file(
        &self,
        object: &Object,
        grid: &Arc<Grid>,
        written: &mut HashMap<usize, Written>,
        report: &mut SaveReport,
    ) -> Result<String, StoreError> {
        let key = Arc::as_ptr(grid) as usize;
        if let Some(Written::Grid { z, .. }) = written.get(&key) {
            return Ok(z.clone());
        }
        if let Some(prior @ Written::Grid { z, .. }) = self.written.get(&key)
            && self.dir.join(z).exists()
        {
            report.object_files_reused += 1;
            written.insert(key, prior.clone());
            return Ok(z.clone());
        }
        let z = format!("{OBJECTS}/{}-r{}-grid.parquet", object.id, object.revision);
        report.bytes_written += write_f64_columns(&self.dir.join(&z), &[("z", grid.z.clone())])?;
        report.object_files_written += 1;
        written.insert(key, Written::Grid { _grid: grid.clone(), z: z.clone() });
        Ok(z)
    }

    /// Deletes this store's own files that `keep` does not name: object files, strings files, run
    /// logs and a leftover staging manifest. Anything else in the folder is left alone.
    fn remove_stale(&self, keep: &HashSet<String>) -> Result<usize, StoreError> {
        let mut deleted = 0;
        let ours_at_root = |name: &str| {
            (name.starts_with("strings-") && name.ends_with(".parquet"))
                || (name.starts_with("run-log-") && name.ends_with(".json"))
                || name == "manifest.json.tmp"
        };
        let ours_in_objects = |name: &str| {
            name.ends_with("-vertices.parquet") || name.ends_with("-triangles.parquet") || name.ends_with("-grid.parquet")
        };
        for (folder, prefix, ours) in [
            (self.dir.clone(), "", &ours_at_root as &dyn Fn(&str) -> bool),
            (self.dir.join(OBJECTS), "objects/", &ours_in_objects as &dyn Fn(&str) -> bool),
        ] {
            for entry in std::fs::read_dir(&folder)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type()?.is_file() && ours(&name) && !keep.contains(&format!("{prefix}{name}")) {
                    std::fs::remove_file(entry.path())?;
                    deleted += 1;
                }
            }
        }
        Ok(deleted)
    }
}

/// Every surface's and solid's mesh, in object order, read across the machine's cores: a pit is
/// tens of thousands of solids, two files each.
fn read_meshes(dir: &Path, objects: &[StoredObject]) -> Result<Vec<Option<TriMesh>>, StoreError> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = objects.len().div_ceil(threads).max(1);
    let read = |stored: &StoredObject| match &stored.body {
        StoredBody::Surface { vertices, triangles, .. } | StoredBody::Solid { vertices, triangles, .. } => read_mesh(dir, vertices, triangles).map(Some),
        _ => Ok(None),
    };
    std::thread::scope(|scope| {
        let read = &read;
        let handles: Vec<_> = objects.chunks(chunk).map(|part| scope.spawn(move || part.iter().map(read).collect::<Result<Vec<_>, StoreError>>())).collect();
        let mut meshes = Vec::with_capacity(objects.len());
        for handle in handles {
            meshes.extend(handle.join().expect("a mesh reading thread panicked")?);
        }
        Ok(meshes)
    })
}

fn read_mesh(dir: &Path, vertices: &str, triangles: &str) -> Result<TriMesh, StoreError> {
    let xyz = read_f64_columns(&dir.join(vertices), &["x", "y", "z"])?;
    let abc = read_u32_columns(&dir.join(triangles), &["a", "b", "c"])?;
    let points: Vec<[f64; 3]> = (0..xyz[0].len()).map(|i| [xyz[0][i], xyz[1][i], xyz[2][i]]).collect();
    let tris: Vec<[u32; 3]> = (0..abc[0].len()).map(|i| [abc[0][i], abc[1][i], abc[2][i]]).collect();
    let n = points.len();
    if let Some(t) = tris.iter().find(|t| t.iter().any(|&i| i as usize >= n)) {
        return Err(StoreError::Format(format!("{triangles}: triangle {t:?} refers past the {n} vertices")));
    }
    Ok(TriMesh::new(points, tris))
}

fn read_string_rows(path: &Path, object_count: usize) -> Result<HashMap<usize, Vec<[f64; 3]>>, StoreError> {
    let objects = read_u32_columns(path, &["object"])?.remove(0);
    let xyz = read_f64_columns(path, &["x", "y", "z"])?;
    let mut rows: HashMap<usize, Vec<[f64; 3]>> = HashMap::new();
    for (i, &object) in objects.iter().enumerate() {
        if object as usize >= object_count {
            return Err(StoreError::Format(format!("{}: row {i} names object {object}", path.display())));
        }
        rows.entry(object as usize).or_default().push([xyz[0][i], xyz[1][i], xyz[2][i]]);
    }
    Ok(rows)
}

fn write_points(path: &Path, points: &[[f64; 3]]) -> Result<u64, StoreError> {
    let axis = |a: usize| points.iter().map(|p| p[a]).collect::<Vec<f64>>();
    write_f64_columns(path, &[("x", axis(0)), ("y", axis(1)), ("z", axis(2))])
}

fn write_triangles(path: &Path, triangles: &[[u32; 3]]) -> Result<u64, StoreError> {
    let corner = |c: usize| Arc::new(UInt32Array::from(triangles.iter().map(|t| t[c]).collect::<Vec<u32>>())) as ArrayRef;
    write_batch(path, vec![("a", corner(0)), ("b", corner(1)), ("c", corner(2))])
}

fn write_string_rows(path: &Path, objects: &[u32], points: &[[f64; 3]]) -> Result<u64, StoreError> {
    let axis = |a: usize| Arc::new(Float64Array::from(points.iter().map(|p| p[a]).collect::<Vec<f64>>())) as ArrayRef;
    write_batch(
        path,
        vec![("object", Arc::new(UInt32Array::from(objects.to_vec())) as ArrayRef), ("x", axis(0)), ("y", axis(1)), ("z", axis(2))],
    )
}

fn write_f64_columns(path: &Path, columns: &[(&str, Vec<f64>)]) -> Result<u64, StoreError> {
    write_batch(path, columns.iter().map(|(name, values)| (*name, Arc::new(Float64Array::from(values.clone())) as ArrayRef)).collect())
}

/// Writes one record batch as a Snappy-compressed Parquet file with non-null columns; returns its size.
fn write_batch(path: &Path, columns: Vec<(&str, ArrayRef)>) -> Result<u64, StoreError> {
    let schema = Arc::new(Schema::new(
        columns.iter().map(|(name, array)| Field::new(*name, array.data_type().clone(), false)).collect::<Vec<_>>(),
    ));
    let batch = RecordBatch::try_new(schema.clone(), columns.into_iter().map(|(_, a)| a).collect())?;
    let properties = WriterProperties::builder().set_compression(Compression::SNAPPY).build();
    let mut writer = ArrowWriter::try_new(File::create(path)?, schema, Some(properties))?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(std::fs::metadata(path)?.len())
}

fn read_f64_columns(path: &Path, names: &[&str]) -> Result<Vec<Vec<f64>>, StoreError> {
    read_columns(path, names, &DataType::Float64, |array| {
        array.as_any().downcast_ref::<Float64Array>().map(|a| a.values().to_vec())
    })
}

fn read_u32_columns(path: &Path, names: &[&str]) -> Result<Vec<Vec<u32>>, StoreError> {
    read_columns(path, names, &DataType::UInt32, |array| {
        array.as_any().downcast_ref::<UInt32Array>().map(|a| a.values().to_vec())
    })
}

/// Reads the named non-null columns of one type, in full.
fn read_columns<T>(
    path: &Path,
    names: &[&str],
    data_type: &DataType,
    values: impl Fn(&dyn Array) -> Option<Vec<T>>,
) -> Result<Vec<Vec<T>>, StoreError> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?.with_batch_size(1 << 20);
    let schema = builder.schema().clone();
    let mut indices = Vec::with_capacity(names.len());
    for name in names {
        let (index, field) = schema
            .column_with_name(name)
            .ok_or_else(|| StoreError::Format(format!("{}: no column '{name}'", path.display())))?;
        if field.data_type() != data_type {
            return Err(StoreError::Format(format!(
                "{}: column '{name}' is {}, expected {data_type}",
                path.display(),
                field.data_type()
            )));
        }
        indices.push(index);
    }
    let mut out: Vec<Vec<T>> = names.iter().map(|_| Vec::new()).collect();
    for batch in builder.build()? {
        let batch = batch?;
        for (column, &index) in out.iter_mut().zip(&indices) {
            let array = batch.column(index);
            if array.null_count() > 0 {
                return Err(StoreError::Format(format!("{}: nulls in a column that must be complete", path.display())));
            }
            column.extend(values(array.as_ref()).expect("column type checked against the schema"));
        }
    }
    Ok(out)
}
