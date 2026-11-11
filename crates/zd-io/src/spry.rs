//! Micromine Spry projects (`.spry`, decision 0039): read and never written, as Deswik's and Vulcan's
//! formats are. The format has no public description. This follows what Spry writes, checked on its
//! database versions 168 and 175.
//!
//! A `.spry` is SQLite, opened read-only and immutable so the file is never touched. Its geometry is
//! in blobs of a 12-byte header (a magic byte of 10 or 11, then `72 79 b8`, four bytes not read, the
//! size unpacked as a little-endian `u32`) and one LZ4 block. Two places hold solids:
//!
//! - **A table's Solid fields:** one blob a node in `NodeData`, its column the field. The vertices
//!   (an `i32` count, then x, y and z), Spry's faces (an `i32` count, each an `i32` count of corners
//!   and the corners), then the triangles (an `i32` count, three `i32` corners each). What follows is
//!   not read. A node is named by its path down the table's levels, as `Pit/Strip/Block/Bench`, and
//!   the other fields of the Solid field's folder, read from `FieldNodeData` (a node count, the nodes,
//!   then their values in that order), come with it, Spry's volume among them.
//! - **Design layers:** `LayerTriangles` holds a layer's solids back to back (vertex and triangle
//!   counts, vertices, triangles, then 24 bytes starting with the solid's handle), `Layer.Shapes` its
//!   strings (a count, then for each a vertex count, the vertices and 30 bytes: the handle, and a 1 at
//!   byte 20 where it is closed), and `LayerAttributes` the attributes of both by handle.
//!
//! Spry's faces are its own drawing of the solid and are passed over: the triangles are the mesh.
//! Surfaces, grids and schedules are not read yet.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use zd_core::{AttrValue, Attributes, Polyline, TriMesh};

use crate::IoError;

/// How the levels of a folder path are joined in a solid field's or layer's name, as Deswik's are.
pub const SPRY_LEVELS: &str = "\\";
/// How the positions of a node's path are joined in its name.
pub const NODE_LEVELS: &str = "/";
/// The `DataType` Spry gives a field or attribute holding a solid.
const SOLID_TYPE: &str = "PrecisionMining.Common.Design.Data.Solid";
/// Bytes after each string's vertices in `Layer.Shapes`, and where in them the closed flag is.
const SHAPE_TAIL: usize = 30;
const SHAPE_CLOSED_AT: usize = 20;
/// Bytes after each solid's triangles in `LayerTriangles`.
const LAYER_SOLID_TAIL: usize = 24;
/// The largest blob unpacked, so a corrupt size can't ask for more memory than any real one needs.
const LARGEST_BLOB: usize = 1 << 31;

/// A table's Solid field, as the project lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SprySolidField {
    /// The table, the folders and the field, joined by [`SPRY_LEVELS`], as `Deposit\Coal\Solid`.
    pub name: String,
    /// Nodes with a solid in this field.
    pub solids: usize,
    /// Their size as stored, packed.
    pub bytes: u64,
}

/// A design layer, as the project lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpryLayer {
    /// The layer's folders and name, joined by [`SPRY_LEVELS`].
    pub name: String,
    /// Its strings, solids and attributes as stored, packed.
    pub bytes: u64,
}

/// What a project holds that can be read, listed from its tables without unpacking any geometry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpryContents {
    pub database_version: i64,
    pub solid_fields: Vec<SprySolidField>,
    pub layers: Vec<SpryLayer>,
}

/// What to read. With nothing named, every solid field and every layer is read; otherwise only those
/// named.
#[derive(Debug, Clone, Default)]
pub struct SpryRead {
    pub solid_fields: Vec<String>,
    pub layers: Vec<String>,
    /// The field in a Solid field's folder that holds Spry's volume of each node's solid, matched
    /// whatever its case and with spaces either side trimmed, as Spry's own field names carry them.
    pub volume_field: String,
}

#[derive(Debug, Clone, Default)]
pub struct SpryProject {
    pub strings: Vec<SpryString>,
    pub meshes: Vec<SpryMesh>,
    pub log: SpryLog,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpryString {
    pub layer: String,
    pub line: Polyline,
    pub attributes: Attributes,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpryMesh {
    /// The solid field's or the design layer's name.
    pub layer: String,
    /// A node's path, for a table's solid; none on a design layer.
    pub node: Option<String>,
    pub mesh: TriMesh,
    pub attributes: Attributes,
    /// Spry's volume of it in m³, from the volume field beside its Solid field.
    pub spry_volume: Option<f64>,
}

/// What the reader did to get from Spry's tables to strings and meshes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpryLog {
    pub database_version: i64,
    pub solid_fields_read: usize,
    pub node_solids_read: usize,
    /// Nodes whose solid has faces but no triangles (Spry's flat plot polygons), passed over.
    pub node_solids_without_triangles: usize,
    /// Nodes with a solid but no value in the volume field.
    pub node_solids_without_volume: usize,
    /// Solid fields whose folder has no field of the volume field's name.
    pub solid_fields_without_volume: Vec<String>,
    pub layers_read: usize,
    pub layer_solids_read: usize,
    pub strings_read: usize,
    /// Strings with fewer than two vertices, passed over.
    pub short_strings: usize,
    /// Closed strings with only two vertices, read as open.
    pub closed_too_short: usize,
    pub triangles_read: usize,
    /// Stored fields of types not read (points, solids), counted by type.
    pub values_passed_over: BTreeMap<String, usize>,
    /// Fields whose stored column doesn't run to its end as its type says, named with why.
    pub fields_unread: Vec<String>,
    /// Layers whose attributes could not be read, named with why; their geometry is read.
    pub attributes_unread: Vec<String>,
    /// Layers carrying text, passed over.
    pub layers_with_text: usize,
}

/// A project, opened read-only and immutable (so SQLite neither writes nor locks it), and checked
/// to be Spry's.
fn open(path: &Path) -> Result<Connection, IoError> {
    if !path.is_file() {
        return Err(IoError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{}: no such file", path.display()))));
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(uri(path), flags).map_err(|e| IoError::Binary(format!("{}: {e}", path.display())))?;
    let version: Option<i64> = connection
        .query_row("select Value from SpryProperties where Name = 'DatabaseVersion'", [], |r| r.get(0))
        .optional()
        .map_err(|e| IoError::Unsupported(format!("{}: not a Spry project ({e})", path.display())))?;
    if version.is_none() {
        return Err(IoError::Unsupported(format!("{}: no Spry database version, so not a Spry project", path.display())));
    }
    Ok(connection)
}

/// A file path as an SQLite URI that opens it read-only and immutable. `%`, `?` and `#` are escaped
/// so a name holding them still names the file.
fn uri(path: &Path) -> String {
    let mut plain = path.to_string_lossy().replace('\\', "/");
    for (c, escaped) in [('%', "%25"), ('?', "%3f"), ('#', "%23")] {
        plain = plain.replace(c, escaped);
    }
    let rooted = if plain.starts_with('/') { plain } else { format!("/{plain}") };
    format!("file:{rooted}?mode=ro&immutable=1")
}

fn sql(path: &Path) -> impl Fn(rusqlite::Error) -> IoError + '_ {
    move |e| IoError::Binary(format!("{}: {e}", path.display()))
}

fn database_version(c: &Connection, path: &Path) -> Result<i64, IoError> {
    c.query_row("select Value from SpryProperties where Name = 'DatabaseVersion'", [], |r| r.get(0)).map_err(sql(path))
}

/// A blob as Spry stores geometry: the 12-byte header, then one LZ4 block.
pub fn unpack_spry_blob(blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < 12 {
        return Err(format!("a geometry blob of {} bytes, shorter than its header", blob.len()));
    }
    if !matches!(blob[0], 10 | 11) || blob[1..4] != [0x72, 0x79, 0xb8] {
        return Err(format!("a geometry blob starting {:02x?}, not as Spry starts one", &blob[..4]));
    }
    let size = u32::from_le_bytes([blob[8], blob[9], blob[10], blob[11]]) as usize;
    if size > LARGEST_BLOB {
        return Err(format!("a geometry blob that says it unpacks to {size} bytes"));
    }
    let unpacked = lz4_flex::block::decompress(&blob[12..], size).map_err(|e| format!("a geometry blob that doesn't unpack: {e}"))?;
    if unpacked.len() != size {
        return Err(format!("a geometry blob that unpacks to {} bytes where its header says {size}", unpacked.len()));
    }
    Ok(unpacked)
}

/// Rows of one stream, unpacked and joined in the order of their `Volume`: Spry splits a long
/// stream (a layer's solids) into rows of 100 MB unpacked, and a solid can run across the split.
fn joined(blobs: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    if let [one] = blobs {
        return unpack_spry_blob(one);
    }
    let mut data = Vec::new();
    for blob in blobs {
        data.extend(unpack_spry_blob(blob)?);
    }
    Ok(data)
}

/// Reads little-endian values in order, refusing to run past the end.
struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    fn new(data: &'a [u8]) -> Self {
        Bytes { data, at: 0 }
    }

    fn done(&self) -> bool {
        self.at >= self.data.len()
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&end| end <= self.data.len()).ok_or_else(|| format!("ends at byte {} where {n} more are needed", self.data.len()))?;
        let bytes = &self.data[self.at..end];
        self.at = end;
        Ok(bytes)
    }

    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap_or_default()))
    }

    fn count(&mut self, what: &str) -> Result<usize, String> {
        let n = self.i32()?;
        usize::try_from(n).map_err(|_| format!("a {what} count of {n}"))
    }

    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap_or_default()))
    }

    fn points(&mut self, n: usize) -> Result<Vec<[f64; 3]>, String> {
        let bytes = self.take(n.checked_mul(24).ok_or("too many vertices")?)?;
        Ok(bytes.as_chunks::<24>().0.iter().map(|p| [le_f64(&p[..8]), le_f64(&p[8..16]), le_f64(&p[16..])]).collect())
    }

    fn triangles(&mut self, n: usize, vertices: usize) -> Result<Vec<[u32; 3]>, String> {
        let bytes = self.take(n.checked_mul(12).ok_or("too many triangles")?)?;
        let mut triangles = Vec::with_capacity(n);
        for t in bytes.as_chunks::<12>().0 {
            let corner = |k: usize| {
                let i = i32::from_le_bytes([t[k], t[k + 1], t[k + 2], t[k + 3]]);
                u32::try_from(i).ok().filter(|&i| (i as usize) < vertices).ok_or_else(|| format!("a triangle corner {i} past the {vertices} vertices"))
            };
            triangles.push([corner(0)?, corner(4)?, corner(8)?]);
        }
        Ok(triangles)
    }

    /// A .NET string: its UTF-8 length in 7-bit groups, low first, then the bytes.
    fn dotnet_string(&mut self) -> Result<String, String> {
        let (mut length, mut shift) = (0usize, 0);
        loop {
            let b = self.take(1)?[0];
            length |= ((b & 0x7f) as usize) << shift;
            if b < 0x80 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return Err("a text length that runs on".into());
            }
        }
        Ok(String::from_utf8_lossy(self.take(length)?).into_owned())
    }
}

fn le_f64(bytes: &[u8]) -> f64 {
    f64::from_le_bytes(bytes.try_into().unwrap_or_default())
}

/// A table's solid, as stored for one node: vertices, Spry's faces (passed over), then triangles.
fn parse_node_solid(data: &[u8]) -> Result<TriMesh, String> {
    let mut b = Bytes::new(data);
    let nv = b.count("vertex")?;
    let vertices = b.points(nv)?;
    let faces = b.count("face")?;
    for _ in 0..faces {
        let corners = b.count("face corner")?;
        b.take(corners.checked_mul(4).ok_or("too many face corners")?)?;
    }
    let nt = b.count("triangle")?;
    let triangles = b.triangles(nt, nv)?;
    Ok(TriMesh::new(vertices, triangles))
}

/// A design layer's solids, back to back, each with its handle.
fn parse_layer_solids(data: &[u8]) -> Result<Vec<(i32, TriMesh)>, String> {
    let mut b = Bytes::new(data);
    let mut solids = Vec::new();
    while !b.done() {
        let nv = b.count("vertex")?;
        let nt = b.count("triangle")?;
        let vertices = b.points(nv)?;
        let triangles = b.triangles(nt, nv)?;
        let tail = b.take(LAYER_SOLID_TAIL)?;
        solids.push((i32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]), TriMesh::new(vertices, triangles)));
    }
    Ok(solids)
}

/// A design layer's string as stored: its handle, vertices and whether it is closed.
type Shape = (i32, Vec<[f64; 3]>, bool);

/// A design layer's strings.
fn parse_shapes(data: &[u8]) -> Result<Vec<Shape>, String> {
    let mut b = Bytes::new(data);
    let n = b.count("string")?;
    let mut shapes = Vec::with_capacity(n.min(1 << 20));
    for _ in 0..n {
        let nv = b.count("vertex")?;
        let vertices = b.points(nv)?;
        let tail = b.take(SHAPE_TAIL)?;
        shapes.push((i32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]), vertices, tail[SHAPE_CLOSED_AT] == 1));
    }
    if !b.done() {
        return Err(format!("{} bytes left after its {n} strings", data.len() - b.at));
    }
    Ok(shapes)
}

/// A design layer's attributes by handle. Each record is a handle, a count, the attributes' ids, then
/// their values, as each attribute's type says.
fn parse_layer_attributes(data: &[u8], attributes: &HashMap<i64, (String, String)>) -> Result<HashMap<i32, Attributes>, String> {
    let mut b = Bytes::new(data);
    let mut by_handle = HashMap::new();
    while !b.done() {
        let handle = b.i32()?;
        let n = b.count("attribute")?;
        let ids: Vec<i32> = (0..n).map(|_| b.i32()).collect::<Result<_, _>>()?;
        let mut values = Attributes::new();
        for id in ids {
            let (name, kind) = attributes.get(&i64::from(id)).ok_or_else(|| format!("attribute {id}, which the project doesn't define"))?;
            let value = match kind.as_str() {
                "System.String" => Some(b.dotnet_string()?).filter(|s| !s.is_empty()).map(AttrValue::Text),
                "System.Double" => Some(b.f64()?).filter(|v| v.is_finite()).map(AttrValue::Number),
                "System.Int32" => Some(AttrValue::Number(f64::from(b.i32()?))),
                "System.Boolean" => Some(AttrValue::Bool(b.take(1)?[0] != 0)),
                other => return Err(format!("attribute {name} is of type {other}, whose layout isn't known")),
            };
            if let Some(value) = value {
                values.insert(name.clone(), value);
            }
        }
        by_handle.insert(handle, values);
    }
    Ok(by_handle)
}

/// A field's stored values by node. Each chunk holds a count, the nodes, then their values in that
/// order. None for a type not read.
fn parse_column(data: &[u8], kind: &str) -> Result<Option<Vec<(i64, AttrValue)>>, String> {
    if !matches!(kind, "System.Double" | "PrecisionMining.Common.Units.Percentage" | "System.Int64" | "System.DateTime" | "System.Int32" | "System.Boolean" | "System.String") {
        return Ok(None);
    }
    let mut b = Bytes::new(data);
    let n = b.count("node")?;
    let nodes: Vec<i64> = (0..n).map(|_| b.i32().map(i64::from)).collect::<Result<_, _>>()?;
    let mut values = Vec::with_capacity(n);
    for node in nodes {
        let value = match kind {
            "System.String" => {
                let length = b.count("text length")?;
                Some(String::from_utf8_lossy(b.take(length)?).into_owned()).filter(|s| !s.is_empty()).map(AttrValue::Text)
            }
            "System.Int32" => Some(AttrValue::Number(f64::from(b.i32()?))),
            "System.Int64" => Some(AttrValue::Number(i64::from_le_bytes(b.take(8)?.try_into().unwrap_or_default()) as f64)),
            "System.Boolean" => Some(AttrValue::Bool(b.take(1)?[0] != 0)),
            "System.DateTime" => date_time(u64::from_le_bytes(b.take(8)?.try_into().unwrap_or_default())).map(AttrValue::Text),
            _ => Some(b.f64()?).filter(|v| v.is_finite()).map(AttrValue::Number),
        };
        if let Some(value) = value {
            values.push((node, value));
        }
    }
    if !b.done() {
        return Err(format!("{} bytes left after its {n} values", data.len() - b.at));
    }
    Ok(Some(values))
}

/// A .NET `DateTime` (100 ns ticks from 1 January of year 1 in its low 62 bits) as
/// `YYYY-MM-DD hh:mm:ss`, or none for the zero Spry leaves where there is no date.
fn date_time(raw: u64) -> Option<String> {
    let ticks = raw & 0x3fff_ffff_ffff_ffff;
    if ticks == 0 {
        return None;
    }
    let seconds = ticks / 10_000_000;
    let (days, of_day) = ((seconds / 86_400) as i64, seconds % 86_400);
    // Days from 1970-01-01, then the civil date (Howard Hinnant's days_from_civil, inverted).
    let z = days - 719_162 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Some(format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", of_day / 3600, of_day / 60 % 60, of_day % 60))
}

/// A Solid field of a table: its id, name, table and the folder its siblings share.
struct SolidField {
    id: i64,
    name: String,
    table: i64,
    container: (String, i64),
}

/// Every Solid field, named by its table, folders and own name. Names that repeat are told apart by
/// the field's id.
fn solid_fields(c: &Connection, path: &Path) -> Result<Vec<SolidField>, IoError> {
    let folders: HashMap<i64, (String, i64, String)> = c
        .prepare("select id, ContainerType, Container_id, Name from FieldFolder")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?.collect())
        .map_err(sql(path))?;
    let schemas: HashMap<i64, i64> = c.prepare("select id, Table_id from Schema").and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()).map_err(sql(path))?;
    let tables: HashMap<i64, String> = c.prepare("select id, Name from Table_").and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()).map_err(sql(path))?;
    let rows: Vec<(i64, String, i64, String)> = c
        .prepare("select id, ContainerType, Container_id, Name from Field where DataType = ?1 order by id")
        .and_then(|mut s| s.query_map([SOLID_TYPE], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, Option<String>>(3)?.unwrap_or_default())))?.collect())
        .map_err(sql(path))?;
    let mut fields = Vec::new();
    for (id, kind, container, name) in rows {
        let mut levels = vec![name.trim().to_string()];
        let (mut kind_at, mut at) = (kind.clone(), container);
        let mut steps = 0;
        while kind_at == "FieldFolder" && steps < 1000 {
            let Some((up_kind, up, folder)) = folders.get(&at) else { break };
            levels.push(folder.trim().to_string());
            (kind_at, at) = (up_kind.clone(), *up);
            steps += 1;
        }
        let table = if kind_at == "Schema" { schemas.get(&at).copied() } else { None };
        let Some(table) = table else { continue };
        levels.push(tables.get(&table).map_or_else(|| format!("table {table}"), |t| t.trim().to_string()));
        levels.reverse();
        fields.push(SolidField { id, name: levels.join(SPRY_LEVELS), table, container: (kind, container) });
    }
    tell_apart(&mut fields);
    Ok(fields)
}

/// A solid field or layer, named in a list where names may repeat.
trait Named {
    fn name(&mut self) -> &mut String;
    /// What tells it apart from others of its name.
    fn which(&self) -> String;
}

impl Named for SolidField {
    fn name(&mut self) -> &mut String {
        &mut self.name
    }

    fn which(&self) -> String {
        format!("field {}", self.id)
    }
}

impl Named for Layer {
    fn name(&mut self) -> &mut String {
        &mut self.name
    }

    fn which(&self) -> String {
        format!("layer {}", self.id)
    }
}

/// Gives every name that repeats a suffix telling them apart, as `Deposit\Coal\Solid [field 38230]`.
fn tell_apart<T: Named>(items: &mut [T]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for item in items.iter_mut() {
        *seen.entry(item.name().clone()).or_default() += 1;
    }
    for item in items.iter_mut() {
        if seen[&*item.name()] > 1 {
            let which = item.which();
            let name = item.name();
            *name = format!("{name} [{which}]");
        }
    }
}

/// A design layer: its id and name.
struct Layer {
    id: i64,
    name: String,
}

/// Every design layer, named by its folders and own name.
fn layers(c: &Connection, path: &Path) -> Result<Vec<Layer>, IoError> {
    let folders: HashMap<i64, (String, i64, String)> = c
        .prepare("select id, ContainerType, Container_id, Name from LayerFolder")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get::<_, Option<String>>(3)?.unwrap_or_default()))))?.collect())
        .map_err(sql(path))?;
    let rows: Vec<(i64, String, i64, String)> = c
        .prepare("select id, ContainerType, Container_id, Name from Layer order by id")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, Option<String>>(3)?.unwrap_or_default())))?.collect())
        .map_err(sql(path))?;
    let mut layers: Vec<Layer> = rows
        .into_iter()
        .map(|(id, kind, container, name)| {
            let mut levels = vec![name.trim().to_string()];
            let (mut kind_at, mut at, mut steps) = (kind, container, 0);
            while kind_at == "LayerFolder" && steps < 1000 {
                let Some((up_kind, up, folder)) = folders.get(&at) else { break };
                levels.push(folder.trim().to_string());
                (kind_at, at) = (up_kind.clone(), *up);
                steps += 1;
            }
            levels.reverse();
            Layer { id, name: levels.join(SPRY_LEVELS) }
        })
        .collect();
    tell_apart(&mut layers);
    Ok(layers)
}

/// What a project holds, from its tables alone, so even a file of gigabytes lists at once.
pub fn list_spry(path: &Path) -> Result<SpryContents, IoError> {
    let c = open(path)?;
    let database_version = database_version(&c, path)?;
    let mut listed = Vec::new();
    for field in solid_fields(&c, path)? {
        let (solids, bytes): (i64, Option<i64>) = c
            .query_row("select count(*), sum(length(Value)) from NodeData where Column_Id = ?1", [field.id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql(path))?;
        if solids > 0 {
            listed.push(SprySolidField { name: field.name, solids: solids as usize, bytes: bytes.unwrap_or(0) as u64 });
        }
    }
    let mut stored: HashMap<i64, i64> = HashMap::new();
    for query in [
        "select id, coalesce(length(Shapes), 0) from Layer",
        "select Layer_id, sum(length(Data)) from LayerTriangles group by Layer_id",
        "select Layer_id, sum(length(Data)) from LayerAttributes group by Layer_id",
    ] {
        let rows: Vec<(i64, Option<i64>)> = c.prepare(query).and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()).map_err(sql(path))?;
        for (id, bytes) in rows {
            *stored.entry(id).or_default() += bytes.unwrap_or(0);
        }
    }
    let layers = layers(&c, path)?
        .into_iter()
        .map(|l| SpryLayer { bytes: stored.get(&l.id).copied().unwrap_or(0) as u64, name: l.name })
        .filter(|l| l.bytes > 0)
        .collect();
    Ok(SpryContents { database_version, solid_fields: listed, layers })
}

/// The strings and meshes of the solid fields and layers named (every one when none is named). A
/// name the project doesn't hold is refused.
pub fn read_spry(path: &Path, read: &SpryRead) -> Result<SpryProject, IoError> {
    let c = open(path)?;
    let fields = solid_fields(&c, path)?;
    let all_layers = layers(&c, path)?;
    let everything = read.solid_fields.is_empty() && read.layers.is_empty();
    let missing: Vec<&str> = read
        .solid_fields
        .iter()
        .filter(|n| !fields.iter().any(|f| &f.name == *n))
        .chain(read.layers.iter().filter(|n| !all_layers.iter().any(|l| &l.name == *n)))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(IoError::Unsupported(format!("{} has no solid field or layer named {}", path.display(), missing.join(", "))));
    }
    let mut project = SpryProject { log: SpryLog { database_version: database_version(&c, path)?, ..SpryLog::default() }, ..SpryProject::default() };
    let bad = |what: &str, e: String| IoError::Binary(format!("{}: {what}: {e}", path.display()));

    for field in fields.iter().filter(|f| everything || read.solid_fields.contains(&f.name)) {
        read_solid_field(&c, path, field, read.volume_field.trim(), &mut project).map_err(|e| match e {
            IoError::Binary(m) if !m.starts_with(&path.display().to_string()) => bad(&format!("solid field {}", field.name), m),
            other => other,
        })?;
    }

    let attributes: HashMap<i64, (String, String)> = c
        .prepare("select id, Name, DataType from Attribute")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, (r.get::<_, Option<String>>(1)?.unwrap_or_default(), r.get::<_, Option<String>>(2)?.unwrap_or_default()))))?.collect())
        .map_err(sql(path))?;
    for layer in all_layers.iter().filter(|l| everything || read.layers.contains(&l.name)) {
        read_layer(&c, path, layer, &attributes, &mut project).map_err(|e| bad(&format!("layer {}", layer.name), e))?;
    }
    Ok(project)
}

/// One Solid field: each node's solid, named by its path, with the values of the fields beside it.
fn read_solid_field(c: &Connection, path: &Path, field: &SolidField, volume_field: &str, project: &mut SpryProject) -> Result<(), IoError> {
    let rows: Vec<(i64, Vec<u8>)> = c
        .prepare("select Node_Id, Value from NodeData where Column_Id = ?1 order by Node_Id")
        .and_then(|mut s| s.query_map([field.id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .map_err(sql(path))?;
    if rows.is_empty() {
        return Ok(());
    }
    let log = &mut project.log;
    log.solid_fields_read += 1;
    let nodes = node_paths(c, path, field.table)?;

    // The fields of the Solid field's own folder, read by node.
    let siblings: Vec<(i64, String, String)> = c
        .prepare("select id, Name, DataType from Field where ContainerType = ?1 and Container_id = ?2 and id != ?3 order by Idx, id")
        .and_then(|mut s| {
            s.query_map(rusqlite::params![field.container.0, field.container.1, field.id], |r| {
                Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default().trim().to_string(), r.get::<_, Option<String>>(2)?.unwrap_or_default()))
            })?
            .collect()
        })
        .map_err(sql(path))?;
    let mut values: HashMap<i64, Attributes> = HashMap::new();
    let mut volumes: HashMap<i64, f64> = HashMap::new();
    let has_volume = siblings.iter().any(|(_, name, _)| name.eq_ignore_ascii_case(volume_field));
    if !has_volume && !volume_field.is_empty() {
        log.solid_fields_without_volume.push(field.name.clone());
    }
    for (id, name, kind) in &siblings {
        let chunks: Vec<Vec<u8>> = c
            .prepare("select Data from FieldNodeData where Field_id = ?1 order by Volume")
            .and_then(|mut s| s.query_map([id], |r| r.get(0))?.collect())
            .map_err(sql(path))?;
        if chunks.is_empty() {
            continue;
        }
        let column = joined(&chunks).and_then(|data| parse_column(&data, kind));
        match column {
            Ok(Some(column)) => {
                for (node, value) in column {
                    if name.eq_ignore_ascii_case(volume_field)
                        && let AttrValue::Number(v) = value
                    {
                        volumes.insert(node, v);
                    }
                    values.entry(node).or_default().insert(name.clone(), value);
                }
            }
            Ok(None) => *log.values_passed_over.entry(kind.rsplit('.').next().unwrap_or(kind).to_string()).or_default() += 1,
            Err(why) => log.fields_unread.push(format!("{}{SPRY_LEVELS}{name}: {why}", field.name.rsplit_once(SPRY_LEVELS).map_or("", |(folder, _)| folder))),
        }
    }

    let solids = in_parallel(&rows, |(node, blob)| unpack_spry_blob(blob).and_then(|data| parse_node_solid(&data)).map_err(|e| format!("node {}: {e}", nodes.get(node).map_or_else(|| node.to_string(), |n| n.0.clone()))));
    for ((node, _), mesh) in rows.iter().zip(solids) {
        let mesh = mesh.map_err(IoError::Binary)?;
        if mesh.triangles.is_empty() {
            log.node_solids_without_triangles += 1;
            continue;
        }
        let (name, levels) = nodes.get(node).cloned().unwrap_or_else(|| (format!("node {node}"), Vec::new()));
        let mut attributes: Attributes = levels.into_iter().map(|(level, position)| (level, AttrValue::Text(position))).collect();
        attributes.extend(values.remove(node).unwrap_or_default());
        let spry_volume = volumes.get(node).copied();
        if spry_volume.is_none() && has_volume {
            log.node_solids_without_volume += 1;
        }
        log.node_solids_read += 1;
        log.triangles_read += mesh.triangles.len();
        project.meshes.push(SpryMesh { layer: field.name.clone(), node: Some(name), mesh, attributes, spry_volume });
    }
    Ok(())
}

/// A node's name, and each level's name with the node's position on it.
type NodePath = (String, Vec<(String, String)>);

/// Each node of a table: its path of positions joined by [`NODE_LEVELS`], and each level's name
/// with the node's position on it. A level name that repeats in a path takes its depth after it.
fn node_paths(c: &Connection, path: &Path, table: i64) -> Result<HashMap<i64, NodePath>, IoError> {
    let levels: HashMap<i64, String> = c.prepare("select id, Name from Level").and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))?.collect()).map_err(sql(path))?;
    let position_sets: HashMap<i64, i64> = c.prepare("select id, Level_id from Positions").and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()).map_err(sql(path))?;
    let positions: HashMap<i64, (String, String)> = c
        .prepare("select id, Name, Positions_id from Position")
        .and_then(|mut s| {
            s.query_map([], |r| {
                let set: Option<i64> = r.get(2)?;
                let level = set.and_then(|s| position_sets.get(&s)).and_then(|l| levels.get(l)).cloned().unwrap_or_default();
                Ok((r.get(0)?, (level.trim().to_string(), r.get::<_, Option<String>>(1)?.unwrap_or_default().trim().to_string())))
            })?
            .collect()
        })
        .map_err(sql(path))?;
    let nodes: HashMap<i64, (Option<i64>, Option<i64>)> = c
        .prepare("select id, Parent_id, Position_id from Node where Table_id = ?1")
        .and_then(|mut s| s.query_map([table], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?.collect())
        .map_err(sql(path))?;
    let mut paths = HashMap::with_capacity(nodes.len());
    for &id in nodes.keys() {
        let mut chain = Vec::new();
        let mut at = Some(id);
        while let Some(node) = at {
            let Some((parent, position)) = nodes.get(&node) else { break };
            if let Some(p) = position.and_then(|p| positions.get(&p)) {
                chain.push(p.clone());
            }
            at = *parent;
            if chain.len() > 64 {
                break;
            }
        }
        chain.reverse();
        let name = chain.iter().map(|(_, p)| p.as_str()).collect::<Vec<_>>().join(NODE_LEVELS);
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let levels = chain
            .into_iter()
            .enumerate()
            .map(|(depth, (level, position))| {
                let level = if level.is_empty() { format!("Level {}", depth + 1) } else { level };
                let level = if seen.insert(level.clone()) { level } else { format!("{level} {}", depth + 1) };
                (level, position)
            })
            .collect();
        paths.insert(id, (name, levels));
    }
    Ok(paths)
}

/// One design layer: its strings and solids, with their attributes.
fn read_layer(c: &Connection, path: &Path, layer: &Layer, attributes: &HashMap<i64, (String, String)>, project: &mut SpryProject) -> Result<(), String> {
    let log = &mut project.log;
    let (shapes, texts): (Option<Vec<u8>>, Option<Vec<u8>>) =
        c.query_row("select Shapes, Texts from Layer where id = ?1", [layer.id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|e| format!("{}: {e}", path.display()))?;
    let blobs = |table: &str| -> Result<Vec<Vec<u8>>, String> {
        c.prepare(&format!("select Data from {table} where Layer_id = ?1 order by Volume"))
            .and_then(|mut s| s.query_map([layer.id], |r| r.get(0))?.collect())
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    let triangles = joined(&blobs("LayerTriangles")?)?;
    let attribute_blobs = blobs("LayerAttributes")?;
    log.layers_read += 1;
    if texts.as_deref().is_some_and(|t| !t.is_empty()) {
        log.layers_with_text += 1;
    }

    let mut by_handle: HashMap<i32, Attributes> = HashMap::new();
    if !attribute_blobs.is_empty() {
        match joined(&attribute_blobs).and_then(|data| parse_layer_attributes(&data, attributes)) {
            Ok(read) => by_handle = read,
            Err(why) => log.attributes_unread.push(format!("{}: {why}", layer.name)),
        }
    }

    if let Some(shapes) = shapes.filter(|s| !s.is_empty()) {
        for (handle, vertices, closed) in parse_shapes(&unpack_spry_blob(&shapes)?)? {
            if vertices.len() < 2 {
                log.short_strings += 1;
                continue;
            }
            let closed = if closed && vertices.len() < 3 {
                log.closed_too_short += 1;
                false
            } else {
                closed
            };
            log.strings_read += 1;
            let attributes = by_handle.get(&handle).cloned().unwrap_or_default();
            project.strings.push(SpryString { layer: layer.name.clone(), line: Polyline { vertices, closed }, attributes });
        }
    }
    for (handle, mesh) in parse_layer_solids(&triangles)? {
        log.layer_solids_read += 1;
        log.triangles_read += mesh.triangles.len();
        let attributes = by_handle.get(&handle).cloned().unwrap_or_default();
        project.meshes.push(SpryMesh { layer: layer.name.clone(), node: None, mesh, attributes, spry_volume: None });
    }
    Ok(())
}

/// `f` over the items across the machine's cores, results in the items' order.
fn in_parallel<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(items.len() / 64 + 1);
    if workers <= 1 {
        return items.iter().map(&f).collect();
    }
    let chunk = items.len().div_ceil(workers);
    std::thread::scope(|s| {
        let handles: Vec<_> = items.chunks(chunk).map(|part| s.spawn(|| part.iter().map(&f).collect::<Vec<R>>())).collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e))).collect()
    })
}

/// Small `.spry` files laid out as Spry lays them out, for tests of the reader: a table with one
/// level path a node and a Solid field in one folder beside the fields given, and design layers of
/// strings and solids with attributes. Never used to write a project for Spry.
#[cfg(any(test, feature = "spry-fixtures"))]
pub mod fixtures {
    use super::*;

    pub struct FixtureTable {
        pub name: String,
        pub levels: Vec<String>,
        pub folder: String,
        pub solid_field: String,
        pub nodes: Vec<FixtureNode>,
    }

    pub struct FixtureNode {
        /// One position a level.
        pub path: Vec<String>,
        pub solid: Option<FixtureSolid>,
        /// Values of fields in the Solid field's folder: numbers as doubles, text as strings.
        pub values: Vec<(String, AttrValue)>,
    }

    /// A node's solid: its vertices, Spry's faces and the triangles.
    pub struct FixtureSolid {
        pub vertices: Vec<[f64; 3]>,
        pub faces: Vec<Vec<i32>>,
        pub triangles: Vec<[i32; 3]>,
    }

    pub struct FixtureLayer {
        /// Folders, then the layer's own name.
        pub path: Vec<String>,
        pub strings: Vec<(Polyline, Attributes)>,
        pub solids: Vec<(TriMesh, Attributes)>,
    }

    #[derive(Default)]
    pub struct SpryFixture {
        pub tables: Vec<FixtureTable>,
        pub layers: Vec<FixtureLayer>,
        /// Split each layer's solids into rows of this many bytes unpacked, as Spry splits a long one.
        pub split_solids_at: Option<usize>,
    }

    /// Raw bytes packed as Spry packs geometry: the header, then an LZ4 block.
    pub fn pack_spry_blob(raw: &[u8]) -> Vec<u8> {
        let mut blob = vec![0x0b, 0x72, 0x79, 0xb8, 0, 0, 0, 0];
        blob.extend((raw.len() as u32).to_le_bytes());
        blob.extend(lz4_flex::block::compress(raw));
        blob
    }

    fn i32s(out: &mut Vec<u8>, values: impl IntoIterator<Item = i32>) {
        for v in values {
            out.extend(v.to_le_bytes());
        }
    }

    fn points(out: &mut Vec<u8>, points: &[[f64; 3]]) {
        for p in points {
            for c in p {
                out.extend(c.to_le_bytes());
            }
        }
    }

    pub fn node_solid_bytes(solid: &FixtureSolid) -> Vec<u8> {
        let mut out = Vec::new();
        i32s(&mut out, [solid.vertices.len() as i32]);
        points(&mut out, &solid.vertices);
        i32s(&mut out, [solid.faces.len() as i32]);
        for face in &solid.faces {
            i32s(&mut out, [face.len() as i32]);
            i32s(&mut out, face.iter().copied());
        }
        i32s(&mut out, [solid.triangles.len() as i32]);
        i32s(&mut out, solid.triangles.iter().flatten().copied());
        // What Spry writes after the triangles, not read: 1.0, then zeros.
        out.extend(1.0f64.to_le_bytes());
        out.extend([0u8; 84]);
        out
    }

    fn dotnet_string(out: &mut Vec<u8>, s: &str) {
        let mut n = s.len();
        loop {
            let b = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                out.push(b);
                break;
            }
            out.push(b | 0x80);
        }
        out.extend(s.as_bytes());
    }

    fn kind_of(value: &AttrValue) -> &'static str {
        match value {
            AttrValue::Text(_) => "System.String",
            AttrValue::Number(_) => "System.Double",
            AttrValue::Bool(_) => "System.Boolean",
        }
    }

    const SCHEMA: &str = "
        create table SpryProperties (Name TEXT, Value);
        create table Table_ (id integer primary key autoincrement, ContainerType TEXT, Container_id INT, Name TEXT, Items_id INT, Levels_id INT);
        create table Schema (id integer primary key autoincrement, Table_id INT, Folders_id INT, Fields_id INT);
        create table FieldFolder (id integer primary key autoincrement, ContainerType TEXT, Container_id INT, Name TEXT, Idx INT);
        create table Field (id integer primary key autoincrement, ContainerType TEXT, Container_id INT, Name TEXT, Idx INT, DataType TEXT, IsCalculated BOOL);
        create table FieldNodeData (Field_id INTEGER, Volume INTEGER, Data BLOB);
        create table Levels (id integer primary key autoincrement, Table_id INT);
        create table Level (id integer primary key autoincrement, idx INT, Name TEXT, Levels_id INT, Positions_id INT);
        create table Positions (id integer primary key autoincrement, Level_id INT);
        create table Position (id integer primary key autoincrement, Name TEXT, idx INT, Positions_id INT);
        create table Node (id integer primary key autoincrement, Parent_id INT, Table_id INT, Position_id INT);
        create table NodeData (Node_Id INT not null, Column_Id INT not null, Value BLOB, primary key (Node_Id, Column_Id));
        create table LayerFolder (id integer primary key autoincrement, ContainerType TEXT, Container_id INT, Name TEXT);
        create table Layer (id integer primary key autoincrement, ContainerType TEXT, Container_id INT, Name TEXT, Shapes BLOB, Texts BLOB);
        create table LayerTriangles (Layer_id INTEGER, Volume INTEGER, Data BLOB);
        create table LayerAttributes (Layer_id INTEGER, Volume INTEGER, Data BLOB);
        create table Attribute (id integer primary key autoincrement, Name TEXT, Idx INT, DataType TEXT, ContainerType TEXT, Container_id INT);
    ";

    /// Writes the fixture as a `.spry` of database version 175.
    pub fn write_spry_fixture(path: &Path, fixture: &SpryFixture) -> Result<(), IoError> {
        let e = |e: rusqlite::Error| IoError::Binary(e.to_string());
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let c = Connection::open(path).map_err(e)?;
        c.execute_batch(SCHEMA).map_err(e)?;
        c.execute("insert into SpryProperties values ('DatabaseVersion', 175)", []).map_err(e)?;
        for table in &fixture.tables {
            write_table(&c, table).map_err(e)?;
        }
        let mut attribute_ids: BTreeMap<(String, &'static str), i64> = BTreeMap::new();
        for layer in &fixture.layers {
            let (mut kind, mut container) = ("LayerData".to_string(), 1i64);
            for folder in &layer.path[..layer.path.len() - 1] {
                c.execute("insert into LayerFolder (ContainerType, Container_id, Name) values (?1, ?2, ?3)", rusqlite::params![kind, container, folder]).map_err(e)?;
                (kind, container) = ("LayerFolder".into(), c.last_insert_rowid());
            }
            let mut shapes = Vec::new();
            let mut attributes = Vec::new();
            let mut handle: i32 = 100;
            let mut record = |handle: i32, values: &Attributes, ids: &mut BTreeMap<(String, &'static str), i64>| -> Result<(), rusqlite::Error> {
                if values.is_empty() {
                    return Ok(());
                }
                i32s(&mut attributes, [handle, values.len() as i32]);
                let mut tail = Vec::new();
                for (name, value) in values {
                    let key = (name.to_string(), kind_of(value));
                    let id = match ids.get(&key) {
                        Some(id) => *id,
                        None => {
                            c.execute("insert into Attribute (Name, DataType, ContainerType, Container_id) values (?1, ?2, 'AttributeCollection', 1)", rusqlite::params![name, key.1])?;
                            let id = c.last_insert_rowid();
                            ids.insert(key, id);
                            id
                        }
                    };
                    i32s(&mut attributes, [id as i32]);
                    match value {
                        AttrValue::Text(t) => dotnet_string(&mut tail, t),
                        AttrValue::Number(n) => tail.extend(n.to_le_bytes()),
                        AttrValue::Bool(b) => tail.push(u8::from(*b)),
                    }
                }
                attributes.extend(tail);
                Ok(())
            };
            if !layer.strings.is_empty() {
                i32s(&mut shapes, [layer.strings.len() as i32]);
                for (line, values) in &layer.strings {
                    handle += 1;
                    i32s(&mut shapes, [line.vertices.len() as i32]);
                    points(&mut shapes, &line.vertices);
                    let mut tail = [0u8; SHAPE_TAIL];
                    tail[..4].copy_from_slice(&handle.to_le_bytes());
                    tail[4] = 1;
                    tail[SHAPE_CLOSED_AT] = u8::from(line.closed);
                    shapes.extend(tail);
                    record(handle, values, &mut attribute_ids).map_err(e)?;
                }
            }
            let mut solids = Vec::new();
            for (mesh, values) in &layer.solids {
                handle += 1;
                i32s(&mut solids, [mesh.vertices.len() as i32, mesh.triangles.len() as i32]);
                points(&mut solids, &mesh.vertices);
                i32s(&mut solids, mesh.triangles.iter().flatten().map(|&i| i as i32));
                let mut tail = [0u8; LAYER_SOLID_TAIL];
                tail[..4].copy_from_slice(&handle.to_le_bytes());
                solids.extend(tail);
                record(handle, values, &mut attribute_ids).map_err(e)?;
            }
            let name = layer.path.last().cloned().unwrap_or_default();
            let shapes = (!shapes.is_empty()).then(|| pack_spry_blob(&shapes));
            c.execute("insert into Layer (ContainerType, Container_id, Name, Shapes) values (?1, ?2, ?3, ?4)", rusqlite::params![kind, container, name, shapes]).map_err(e)?;
            let id = c.last_insert_rowid();
            for (k, part) in solids.chunks(fixture.split_solids_at.unwrap_or(usize::MAX)).enumerate() {
                c.execute("insert into LayerTriangles values (?1, ?2, ?3)", rusqlite::params![id, k as i64, pack_spry_blob(part)]).map_err(e)?;
            }
            if !attributes.is_empty() {
                c.execute("insert into LayerAttributes values (?1, 0, ?2)", rusqlite::params![id, pack_spry_blob(&attributes)]).map_err(e)?;
            }
        }
        Ok(())
    }

    fn write_table(c: &Connection, table: &FixtureTable) -> Result<(), rusqlite::Error> {
        c.execute("insert into Table_ (ContainerType, Container_id, Name) values ('Tables', 1, ?1)", [&table.name])?;
        let table_id = c.last_insert_rowid();
        c.execute("insert into Schema (Table_id) values (?1)", [table_id])?;
        let schema = c.last_insert_rowid();
        c.execute("insert into FieldFolder (ContainerType, Container_id, Name, Idx) values ('Schema', ?1, ?2, 0)", rusqlite::params![schema, table.folder])?;
        let folder = c.last_insert_rowid();
        c.execute("insert into Field (ContainerType, Container_id, Name, Idx, DataType) values ('FieldFolder', ?1, ?2, 0, ?3)", rusqlite::params![folder, table.solid_field, SOLID_TYPE])?;
        let solid_field = c.last_insert_rowid();

        c.execute("insert into Levels (Table_id) values (?1)", [table_id])?;
        let levels = c.last_insert_rowid();
        let mut position_sets = Vec::new();
        for (idx, level) in table.levels.iter().enumerate() {
            c.execute("insert into Level (idx, Name, Levels_id) values (?1, ?2, ?3)", rusqlite::params![idx as i64, level, levels])?;
            let level_id = c.last_insert_rowid();
            c.execute("insert into Positions (Level_id) values (?1)", [level_id])?;
            position_sets.push(c.last_insert_rowid());
        }
        let mut positions: BTreeMap<(usize, String), i64> = BTreeMap::new();
        let mut nodes: BTreeMap<Vec<String>, i64> = BTreeMap::new();
        let mut fields: BTreeMap<String, (i64, &'static str)> = BTreeMap::new();
        let mut columns: BTreeMap<String, Vec<(i64, AttrValue)>> = BTreeMap::new();
        for node in &table.nodes {
            let mut parent: Option<i64> = None;
            for (depth, &positions_id) in position_sets.iter().enumerate().take(node.path.len()) {
                let prefix = node.path[..=depth].to_vec();
                if let Some(id) = nodes.get(&prefix) {
                    parent = Some(*id);
                    continue;
                }
                let key = (depth, node.path[depth].clone());
                let position = match positions.get(&key) {
                    Some(p) => *p,
                    None => {
                        c.execute("insert into Position (Name, idx, Positions_id) values (?1, 0, ?2)", rusqlite::params![key.1, positions_id])?;
                        let p = c.last_insert_rowid();
                        positions.insert(key, p);
                        p
                    }
                };
                c.execute("insert into Node (Parent_id, Table_id, Position_id) values (?1, ?2, ?3)", rusqlite::params![parent, table_id, position])?;
                let id = c.last_insert_rowid();
                nodes.insert(prefix, id);
                parent = Some(id);
            }
            let Some(id) = parent else { continue };
            if let Some(solid) = &node.solid {
                c.execute("insert into NodeData values (?1, ?2, ?3)", rusqlite::params![id, solid_field, pack_spry_blob(&node_solid_bytes(solid))])?;
            }
            for (name, value) in &node.values {
                if !fields.contains_key(name) {
                    c.execute("insert into Field (ContainerType, Container_id, Name, Idx, DataType) values ('FieldFolder', ?1, ?2, ?3, ?4)", rusqlite::params![folder, name, fields.len() as i64 + 1, kind_of(value)])?;
                    fields.insert(name.clone(), (c.last_insert_rowid(), kind_of(value)));
                }
                columns.entry(name.clone()).or_default().push((id, value.clone()));
            }
        }
        for (name, column) in columns {
            let mut out = Vec::new();
            i32s(&mut out, [column.len() as i32]);
            i32s(&mut out, column.iter().map(|(node, _)| *node as i32));
            for (_, value) in &column {
                match value {
                    AttrValue::Number(n) => out.extend(n.to_le_bytes()),
                    AttrValue::Text(t) => {
                        i32s(&mut out, [t.len() as i32]);
                        out.extend(t.as_bytes());
                    }
                    AttrValue::Bool(b) => out.push(u8::from(*b)),
                }
            }
            c.execute("insert into FieldNodeData values (?1, 0, ?2)", rusqlite::params![fields[&name].0, pack_spry_blob(&out)])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("zd-spry-{tag}-{}-{}", std::process::id(), zd_core::RunId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn volume(mesh: &TriMesh) -> f64 {
        mesh.triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| mesh.vertices[i as usize]);
                a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])
            })
            .sum::<f64>()
            / 6.0
    }

    /// A 100 x 50 x 10 m box at RL 90: its six faces as Spry draws them and twelve triangles out.
    fn box_solid() -> FixtureSolid {
        let (x, y, z) = (E, N, 90.0);
        let vertices = vec![
            [x, y, z],
            [x + 100.0, y, z],
            [x + 100.0, y + 50.0, z],
            [x, y + 50.0, z],
            [x, y, z + 10.0],
            [x + 100.0, y, z + 10.0],
            [x + 100.0, y + 50.0, z + 10.0],
            [x, y + 50.0, z + 10.0],
        ];
        let faces = vec![vec![0, 3, 2, 1], vec![4, 5, 6, 7], vec![0, 1, 5, 4], vec![1, 2, 6, 5], vec![2, 3, 7, 6], vec![3, 0, 4, 7]];
        let triangles = faces.iter().flat_map(|f| [[f[0], f[1], f[2]], [f[0], f[2], f[3]]]).collect();
        FixtureSolid { vertices, faces, triangles }
    }

    /// A tetrahedron of 30 m legs from a corner at RL 100: 30³ / 6 = 4,500 m³.
    fn tetrahedron() -> TriMesh {
        let (x, y, z) = (E + 500.0, N, 100.0);
        TriMesh::new(vec![[x, y, z], [x + 30.0, y, z], [x, y + 30.0, z], [x, y, z + 30.0]], vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]])
    }

    fn fixture() -> SpryFixture {
        let text = |s: &str| AttrValue::Text(s.into());
        let number = AttrValue::Number;
        let mut attributes = Attributes::new();
        attributes.insert("Block".into(), text("B12"));
        attributes.insert("strip".into(), number(12.0));
        let toe = Polyline { vertices: vec![[E, N, 100.0], [E + 100.0, N, 100.0], [E + 100.0, N + 100.0, 100.0]], closed: false };
        let outline = Polyline { vertices: vec![[E, N, 100.0], [E + 100.0, N, 100.0], [E + 100.0, N + 100.0, 100.0], [E, N + 100.0, 100.0]], closed: true };
        let plot = FixtureSolid { vertices: vec![[E, N, 500.0], [E + 10.0, N, 500.0], [E + 10.0, N + 10.0, 500.0]], faces: vec![vec![0, 1, 2]], triangles: vec![] };
        SpryFixture {
            tables: vec![FixtureTable {
                name: "Deposit".into(),
                levels: vec!["Pit".into(), "Strip".into(), "Block".into()],
                folder: "Coal".into(),
                solid_field: "Solid".into(),
                nodes: vec![
                    FixtureNode {
                        path: vec!["PIT A".into(), "12".into(), "B3".into()],
                        solid: Some(box_solid()),
                        values: vec![("Volume".into(), number(50_000.0)), ("Seam".into(), text("4U"))],
                    },
                    FixtureNode { path: vec!["PIT A".into(), "12".into(), "B4".into()], solid: Some(plot), values: vec![("Volume".into(), number(0.0))] },
                    FixtureNode { path: vec!["PIT A".into(), "13".into(), "B1".into()], solid: None, values: vec![("Volume".into(), number(7.0))] },
                ],
            }],
            layers: vec![
                FixtureLayer { path: vec!["DESIGN".into(), "TOES".into()], strings: vec![(toe, attributes.clone()), (outline, Attributes::new())], solids: vec![] },
                FixtureLayer { path: vec!["SOLIDS".into()], strings: vec![], solids: vec![(tetrahedron(), attributes)] },
            ],
            split_solids_at: None,
        }
    }

    #[test]
    fn a_blob_unpacks_as_spry_packs_one_and_a_bad_header_or_size_is_refused() {
        let raw: Vec<u8> = (0..10_000u32).flat_map(|i| (i % 97).to_le_bytes()).collect();
        let blob = pack_spry_blob(&raw);
        assert_eq!(unpack_spry_blob(&blob).unwrap(), raw);
        let mut other_magic = blob.clone();
        other_magic[0] = 0x0a;
        assert_eq!(unpack_spry_blob(&other_magic).unwrap(), raw, "both magic bytes Spry writes read alike");

        let mut wrong = blob.clone();
        wrong[1] = 0;
        assert!(unpack_spry_blob(&wrong).unwrap_err().contains("not as Spry starts one"));
        let mut too_big = blob.clone();
        too_big[8..12].copy_from_slice(&(raw.len() as u32 + 1).to_le_bytes());
        assert!(unpack_spry_blob(&too_big).is_err());
        assert!(unpack_spry_blob(&blob[..blob.len() / 2]).is_err(), "cut short");
        assert!(unpack_spry_blob(&blob[..6]).unwrap_err().contains("shorter than its header"));
    }

    #[test]
    fn a_node_solid_reads_its_triangles_and_passes_over_spry_faces_and_tail() {
        let solid = box_solid();
        let mesh = parse_node_solid(&node_solid_bytes(&solid)).unwrap();
        assert_eq!(mesh.vertices, solid.vertices, "bit for bit");
        assert_eq!(mesh.triangles.len(), 12);
        // 100 x 50 x 10 m, to a millionth of a cubic metre.
        assert!((volume(&mesh) - 50_000.0).abs() < 1e-6, "{}", volume(&mesh));

        let mut past = box_solid();
        past.triangles[3] = [0, 1, 8];
        assert!(parse_node_solid(&node_solid_bytes(&past)).unwrap_err().contains("past the 8 vertices"));
        let bytes = node_solid_bytes(&solid);
        assert!(parse_node_solid(&bytes[..300]).unwrap_err().contains("more are needed"), "cut short in its triangles");
    }

    #[test]
    fn columns_read_by_type_and_a_date_reads_as_its_day() {
        let mut out = Vec::new();
        for v in [2i32, 7, 9] {
            out.extend(v.to_le_bytes());
        }
        out.extend(1.5f64.to_le_bytes());
        out.extend(f64::NAN.to_le_bytes());
        let read = parse_column(&out, "System.Double").unwrap().unwrap();
        assert_eq!(read, vec![(7, AttrValue::Number(1.5))], "a NaN is no value");
        let mut text = Vec::new();
        for v in [2i32, 3, 4, 2] {
            text.extend(v.to_le_bytes());
        }
        text.extend(b"4U");
        text.extend(0i32.to_le_bytes());
        assert_eq!(parse_column(&text, "System.String").unwrap().unwrap(), vec![(3, AttrValue::Text("4U".into()))]);
        assert!(parse_column(&out[..20], "System.Double").is_err(), "cut short");
        assert!(parse_column(&out, "PrecisionMining.Common.Math.Point3D").unwrap().is_none(), "not read, counted by the caller");
        // 2025-11-24 20:23:35 as .NET writes it, with its kind in the top bits.
        let ticks = 638_996_126_150_000_000u64 | (1 << 62);
        assert_eq!(date_time(ticks).as_deref(), Some("2025-11-24 20:23:35"));
        assert_eq!(date_time(0), None);
    }

    #[test]
    fn a_spry_lists_its_solid_fields_and_layers_and_reads_them_with_names_and_values() {
        let dir = scratch("read");
        let path = dir.join("Model #2.spry");
        write_spry_fixture(&path, &fixture()).unwrap();

        let contents = list_spry(&path).unwrap();
        assert_eq!(contents.database_version, 175);
        assert_eq!(contents.solid_fields.iter().map(|f| (f.name.as_str(), f.solids)).collect::<Vec<_>>(), vec![(r"Deposit\Coal\Solid", 2)]);
        assert_eq!(contents.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), vec![r"DESIGN\TOES", "SOLIDS"]);

        let project = read_spry(&path, &SpryRead { volume_field: "Volume".into(), ..SpryRead::default() }).unwrap();
        let log = &project.log;
        assert_eq!((log.solid_fields_read, log.node_solids_read, log.node_solids_without_triangles), (1, 1, 1), "the flat plot polygon passed over");
        assert_eq!((log.layers_read, log.strings_read, log.layer_solids_read, log.triangles_read), (2, 2, 1, 16));

        let block = &project.meshes[0];
        assert_eq!((block.layer.as_str(), block.node.as_deref()), (r"Deposit\Coal\Solid", Some("PIT A/12/B3")));
        assert_eq!(block.spry_volume, Some(50_000.0));
        assert_eq!(block.attributes.get("Pit"), Some(&AttrValue::Text("PIT A".into())));
        assert_eq!(block.attributes.get("Block"), Some(&AttrValue::Text("B3".into())));
        assert_eq!(block.attributes.get("Seam"), Some(&AttrValue::Text("4U".into())));
        assert_eq!(block.attributes.get("Volume"), Some(&AttrValue::Number(50_000.0)));
        assert!((volume(&block.mesh) - 50_000.0).abs() < 1e-6);

        let tet = &project.meshes[1];
        assert_eq!((tet.layer.as_str(), tet.node.as_deref(), tet.spry_volume), ("SOLIDS", None, None));
        // 30³ / 6, to a millionth of a cubic metre.
        assert!((volume(&tet.mesh) - 4_500.0).abs() < 1e-6, "{}", volume(&tet.mesh));
        assert_eq!(tet.attributes.get("Block"), Some(&AttrValue::Text("B12".into())));
        assert_eq!(tet.attributes.get("strip"), Some(&AttrValue::Number(12.0)));

        let (toe, outline) = (&project.strings[0], &project.strings[1]);
        assert_eq!(toe.layer, r"DESIGN\TOES");
        assert_eq!(toe.line.vertices[2], [E + 100.0, N + 100.0, 100.0]);
        assert!(!toe.line.closed && outline.line.closed);
        assert_eq!(toe.attributes.get("Block"), Some(&AttrValue::Text("B12".into())));
        assert!(outline.attributes.is_empty());

        // Only what's named, and a name the file doesn't hold is refused.
        let some = read_spry(&path, &SpryRead { layers: vec!["SOLIDS".into()], volume_field: "Volume".into(), ..SpryRead::default() }).unwrap();
        assert_eq!((some.meshes.len(), some.strings.len()), (1, 0));
        let refused = read_spry(&path, &SpryRead { layers: vec!["PIT B".into()], ..SpryRead::default() }).unwrap_err().to_string();
        assert!(refused.contains("no solid field or layer named PIT B"), "{refused}");
        let none = read_spry(&path, &SpryRead { solid_fields: vec![r"Deposit\Coal\Solid".into()], volume_field: "Tonnes".into(), ..SpryRead::default() }).unwrap();
        assert_eq!(none.log.solid_fields_without_volume, vec![r"Deposit\Coal\Solid".to_string()]);
        assert_eq!(none.meshes[0].spry_volume, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_layer_split_across_rows_mid_solid_reads_whole() {
        let dir = scratch("split");
        let path = dir.join("split.spry");
        let mut split = fixture();
        let mut far = tetrahedron();
        far.vertices.iter_mut().for_each(|v| v[0] += 100.0);
        split.layers[1].solids.push((far, Attributes::new()));
        // Two tetrahedra take 2 x (8 + 96 + 48 + 24) = 352 bytes; rows of 100 split the first in its vertices.
        split.split_solids_at = Some(100);
        write_spry_fixture(&path, &split).unwrap();
        let rows: i64 = Connection::open(&path).unwrap().query_row("select count(*) from LayerTriangles", [], |r| r.get(0)).unwrap();
        assert_eq!(rows, 4);
        let project = read_spry(&path, &SpryRead { layers: vec!["SOLIDS".into()], ..SpryRead::default() }).unwrap();
        assert_eq!(project.meshes.len(), 2);
        for m in &project.meshes {
            assert!((volume(&m.mesh) - 4_500.0).abs() < 1e-6, "{}", volume(&m.mesh));
        }
        assert_eq!(project.meshes[0].attributes.get("Block"), Some(&AttrValue::Text("B12".into())), "attributes still found by handle");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_that_is_not_a_spry_project_is_refused() {
        let dir = scratch("not");
        let text = dir.join("notes.spry");
        std::fs::write(&text, "not a database").unwrap();
        assert!(list_spry(&text).is_err());
        let empty_db = dir.join("empty.spry");
        Connection::open(&empty_db).unwrap().execute_batch("create table t (a)").unwrap();
        assert!(list_spry(&empty_db).unwrap_err().to_string().contains("not a Spry project"));
        assert!(list_spry(&dir.join("missing.spry")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_layer_whose_strings_run_past_their_count_is_refused_by_name() {
        let dir = scratch("bad");
        let path = dir.join("bad.spry");
        write_spry_fixture(&path, &fixture()).unwrap();
        {
            let c = Connection::open(&path).unwrap();
            let shapes: Vec<u8> = c.query_row("select Shapes from Layer where Name = 'TOES'", [], |r| r.get(0)).unwrap();
            let mut raw = unpack_spry_blob(&shapes).unwrap();
            raw.extend([0u8; 7]);
            c.execute("update Layer set Shapes = ?1 where Name = 'TOES'", [pack_spry_blob(&raw)]).unwrap();
        }
        let refused = read_spry(&path, &SpryRead::default()).unwrap_err().to_string();
        assert!(refused.contains(r"layer DESIGN\TOES: 7 bytes left after its 2 strings"), "{refused}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
