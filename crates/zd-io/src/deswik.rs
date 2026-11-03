//! Deswik.CAD drawings (`.dcf`, decision 0036): read and never written, as DXF R12 stays the way back
//! into Deswik (decision 0009). The format has no public description. This follows what Deswik
//! writes, checked on its VectorDraw 7.7 and 10.1 files.
//!
//! A `.dcf` is a zip. Its `<name>.dcf` entry is the document (layer table, blocks, settings), each
//! layer's entities are in an entry of their own, `<name>.dcf_<layer path>` with the levels of the path
//! joined by `^^` (the layer table's `\`) and a `.` written `@@`, and entries starting `_DW_` hold
//! settings. An entry is VectorDraw XML: an `O`
//! element for each object and a `P` element for each property, its field in `F`, its type in `T`
//! and its value in `V`. A long array is `streamed`: base64 of a little-endian `i32` count and the
//! items, padded with zeros to the array's capacity. A short one lists each item as an `_Item`.
//!
//! - A `dwPolyline` is a string: world x, y and z and a bulge for each vertex (its extrusion vector
//!   does not move them), closed where its `Flag` is 1. A bulged segment becomes chords within
//!   [`crate::ARC_SAGITTA`] of the arc, the RL running straight along it.
//! - A `dwPolyface` is a mesh: x, y and z for each vertex, then five numbers to a face, four 1-based
//!   corners (a triangle's fourth repeats its first) and a colour. Deswik's own volume of it
//!   (`VolumeInternal`) is kept where Deswik worked one out.
//! - The `XProperties` of each are its attributes. Empty values are left out.
//!
//! Point sets, text, images and other entities are passed over and counted by type.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use zd_core::{AttrValue, Attributes, Polyline, TriMesh};
use zip::ZipArchive;
use zip::read::ZipReadOptions;

use crate::IoError;
use crate::dxf::arc_points;

/// How a layer path's levels are joined in the names of a `.dcf`'s entries.
const ENTRY_LEVELS: &str = "^^";
/// How a layer path's levels are joined in Deswik's layer table and in a project layer's name, so a
/// layer is named as Deswik shows it and as a string's "Projection Surface" names it.
pub const DESWIK_LEVELS: &str = "\\";

/// A layer of a `.dcf`, as its zip lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeswikLayer {
    /// The layer's path, its levels joined by [`DESWIK_LEVELS`].
    pub name: String,
    /// The size of its entry once unzipped, in bytes.
    pub bytes: u64,
}

/// What the layers read hold.
#[derive(Debug, Clone, Default)]
pub struct DeswikDrawing {
    pub strings: Vec<DeswikString>,
    pub meshes: Vec<DeswikMesh>,
    pub log: DeswikLog,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeswikString {
    pub layer: String,
    pub line: Polyline,
    pub attributes: Attributes,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeswikMesh {
    pub layer: String,
    pub mesh: TriMesh,
    pub attributes: Attributes,
    /// Deswik's volume of it, in m³, where Deswik worked one out.
    pub deswik_volume: Option<f64>,
}

/// What the reader did to get from entities to strings and meshes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeswikLog {
    pub layers_read: usize,
    /// The VectorDraw versions of the layers read.
    pub versions: BTreeSet<String>,
    pub strings_read: usize,
    /// Bulged segments replaced by chords within [`crate::ARC_SAGITTA`] of the arc.
    pub arcs_to_chords: usize,
    /// Strings with fewer than two vertices, passed over.
    pub short_strings: usize,
    /// Closed strings with only two vertices, read as open.
    pub closed_too_short: usize,
    pub meshes_read: usize,
    /// Meshes with no face of three different corners, passed over.
    pub empty_meshes: usize,
    pub faces_read: usize,
    /// Four-cornered faces split into two triangles.
    pub quads_split: usize,
    /// Faces dropped for having fewer than three different corners.
    pub degenerate_dropped: usize,
    /// Entities of other types, passed over, counted by type.
    pub skipped: BTreeMap<String, usize>,
    /// Layers whose zip checksum doesn't match what they hold, read anyway as their XML runs whole
    /// to its end.
    pub checksum_mismatches: Vec<String>,
}

impl DeswikLog {
    fn add(&mut self, other: DeswikLog) {
        self.layers_read += other.layers_read;
        self.versions.extend(other.versions);
        self.strings_read += other.strings_read;
        self.arcs_to_chords += other.arcs_to_chords;
        self.short_strings += other.short_strings;
        self.closed_too_short += other.closed_too_short;
        self.meshes_read += other.meshes_read;
        self.empty_meshes += other.empty_meshes;
        self.faces_read += other.faces_read;
        self.quads_split += other.quads_split;
        self.degenerate_dropped += other.degenerate_dropped;
        for (kind, n) in other.skipped {
            *self.skipped.entry(kind).or_default() += n;
        }
        self.checksum_mismatches.extend(other.checksum_mismatches);
    }
}

type Zip = ZipArchive<BufReader<File>>;

fn open(path: &Path) -> Result<Zip, IoError> {
    let named = |e: &dyn std::fmt::Display| IoError::Binary(format!("{}: {e}", path.display()));
    let file = File::open(path).map_err(|e| IoError::Io(std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))))?;
    ZipArchive::new(BufReader::new(file)).map_err(|e| named(&e))
}

/// Each layer's entry: its index in the zip, the layer's name and its size unzipped. Refused if the
/// zip has no `.dcf` document entry, as then it is not a Deswik drawing.
fn layer_entries(zip: &mut Zip, path: &Path) -> Result<Vec<(usize, DeswikLayer)>, IoError> {
    let names: Vec<String> = (0..zip.len()).filter_map(|i| zip.name_for_index(i).and_then(Result::ok).map(Cow::into_owned)).collect();
    let Some(document) = names.iter().find(|n| n.to_ascii_lowercase().ends_with(".dcf")) else {
        return Err(IoError::Unsupported(format!("{}: no .dcf document inside, so not a Deswik drawing", path.display())));
    };
    let prefix = format!("{document}_");
    let mut layers = Vec::new();
    for index in 0..zip.len() {
        let name = match zip.name_for_index(index) {
            Some(Ok(entry)) => layer_name(&entry, &prefix),
            _ => None,
        };
        let Some(name) = name else { continue };
        let bytes = zip.by_index_raw(index).map_err(|e| IoError::Binary(format!("{}: {e}", path.display())))?.size();
        layers.push((index, DeswikLayer { name, bytes }));
    }
    Ok(layers)
}

/// The layer a `.dcf` entry holds, or none for the document and its settings. An entry's name writes
/// the layer table's `\` between levels as `^^`, and a `.` as `@@`.
fn layer_name(entry: &str, prefix: &str) -> Option<String> {
    let path = entry.strip_prefix(prefix)?;
    if path.is_empty() || path.starts_with("_DW_") {
        return None;
    }
    Some(path.split(ENTRY_LEVELS).map(|level| level.replace("@@", ".")).collect::<Vec<_>>().join(DESWIK_LEVELS))
}

/// The layers of a `.dcf` in the order its zip lists them, from the zip's directory alone, so even a
/// file of gigabytes lists at once.
pub fn list_deswik_layers(path: &Path) -> Result<Vec<DeswikLayer>, IoError> {
    let mut zip = open(path)?;
    Ok(layer_entries(&mut zip, path)?.into_iter().map(|(_, layer)| layer).collect())
}

/// The strings and meshes of the layers named (every layer when none is named), read across the
/// machine's cores, in the zip's order of layers. A name that is not a layer of the file is refused.
pub fn read_deswik_layers(path: &Path, names: &[String]) -> Result<DeswikDrawing, IoError> {
    let mut zip = open(path)?;
    let layers = layer_entries(&mut zip, path)?;
    let chosen: Vec<(usize, DeswikLayer)> = if names.is_empty() {
        layers
    } else {
        let missing: Vec<&str> = names.iter().filter(|n| !layers.iter().any(|(_, l)| &l.name == *n)).map(String::as_str).collect();
        if !missing.is_empty() {
            return Err(IoError::Unsupported(format!("{} has no layer named {}", path.display(), missing.join(", "))));
        }
        layers.into_iter().filter(|(_, l)| names.contains(&l.name)).collect()
    };

    // Largest first, so one big layer doesn't start last and hold up the rest.
    let mut order: Vec<usize> = (0..chosen.len()).collect();
    order.sort_by_key(|&k| std::cmp::Reverse(chosen[k].1.bytes));
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(chosen.len()).max(1);
    let next = AtomicUsize::new(0);
    let mut results: Vec<(usize, Result<DeswikDrawing, IoError>)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut done = Vec::new();
                    let mut zip = match open(path) {
                        Ok(zip) => zip,
                        Err(e) => return vec![(0, Err(e))],
                    };
                    loop {
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&at) = order.get(k) else { break };
                        let (index, layer) = &chosen[at];
                        done.push((at, read_layer(&mut zip, *index, &layer.name, path)));
                    }
                    done
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_else(|_| vec![(0, Err(IoError::Binary("a layer reader stopped on an error".into())))])).collect()
    });
    results.sort_by_key(|(at, _)| *at);
    let mut drawing = DeswikDrawing::default();
    for (_, read) in results {
        let read = read?;
        drawing.strings.extend(read.strings);
        drawing.meshes.extend(read.meshes);
        drawing.log.add(read.log);
    }
    Ok(drawing)
}

/// The zip library's wording when an entry's contents don't match the checksum its header gives.
const CHECKSUM_MISMATCH: &str = "Invalid checksum";

/// One layer's entry. Deswik sometimes writes a checksum that doesn't match an entry it wrote whole,
/// so a layer refused for its checksum is read again without it, kept if its XML runs to its end,
/// and named in the log.
fn read_layer(zip: &mut Zip, index: usize, layer: &str, path: &Path) -> Result<DeswikDrawing, IoError> {
    let named = |e: IoError| match e {
        IoError::Binary(message) => IoError::Binary(format!("{}: {message}", path.display())),
        other => other,
    };
    let unreadable = |e: zip::result::ZipError| IoError::Binary(format!("{}: layer {layer}: {e}", path.display()));
    let entry = zip.by_index(index).map_err(unreadable)?;
    match parse_layer(layer, BufReader::with_capacity(1 << 20, entry)) {
        Err(IoError::Binary(message)) if message.contains(CHECKSUM_MISMATCH) => {
            let entry = zip.by_index_with_options(index, ZipReadOptions::new().ignore_crc32(true)).map_err(unreadable)?;
            let mut drawing = parse_layer(layer, BufReader::with_capacity(1 << 20, entry)).map_err(named)?;
            drawing.log.checksum_mismatches.push(layer.to_string());
            Ok(drawing)
        }
        read => read.map_err(named),
    }
}

/// One layer's entry, as VectorDraw XML: the entities of its top-level entity list.
pub fn parse_deswik_layer(layer: &str, xml: &[u8]) -> Result<DeswikDrawing, IoError> {
    parse_layer(layer, xml)
}

/// An open element: its tag (`Begin`, `Activator`, `O`, `P`), field and type.
struct Frame {
    tag: Box<[u8]>,
    field: Box<[u8]>,
    kind: Box<[u8]>,
}

/// An entity being read.
#[derive(Default)]
struct Entity {
    kind: String,
    closed: bool,
    volume: Option<f64>,
    attributes: Attributes,
    property: (Option<String>, Option<AttrValue>),
    vertices: Vec<[f64; 3]>,
    bulges: Vec<f64>,
    corners: Vec<i32>,
}

impl Entity {
    fn is_string(&self) -> bool {
        matches!(self.kind.as_str(), "dwPolyline" | "vdPolyline")
    }

    fn is_mesh(&self) -> bool {
        matches!(self.kind.as_str(), "dwPolyface" | "vdPolyface")
    }
}

fn parse_layer<R: BufRead>(layer: &str, source: R) -> Result<DeswikDrawing, IoError> {
    let mut reader = Reader::from_reader(source);
    let mut buf = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut entity: Option<Entity> = None;
    let mut drawing = DeswikDrawing { log: DeswikLog { layers_read: 1, ..DeswikLog::default() }, ..DeswikDrawing::default() };
    let bad = |reader: &Reader<R>, message: String| IoError::Binary(format!("layer {layer}, byte {}: {message}", reader.buffer_position()));
    let mut ended = false;
    loop {
        let event = reader.read_event_into(&mut buf).map_err(|e| bad(&reader, e.to_string()))?;
        match event {
            Event::Start(e) => {
                let frame = frame(&e).map_err(|m| bad(&reader, m))?;
                if entity.is_none() && is_entity(&stack, &frame) {
                    entity = Some(Entity { kind: String::from_utf8_lossy(&frame.kind).into_owned(), ..Entity::default() });
                }
                stack.push(frame);
            }
            Event::Empty(e) if e.name().as_ref() == b"P" => {
                property(&e, &stack, entity.as_mut(), &mut drawing.log).map_err(|m| bad(&reader, m))?;
            }
            Event::End(_) => {
                let Some(closing) = stack.pop() else { return Err(bad(&reader, "an element closes that never opened".into())) };
                ended |= stack.is_empty() && &*closing.tag == b"Begin";
                if stack.len() == 2 && entity.is_some() {
                    let done = entity.take().unwrap_or_default();
                    finish_entity(done, layer, &mut drawing).map_err(|m| bad(&reader, m))?;
                } else if let Some(e) = entity.as_mut()
                    && stack.len() == 4
                    && &*closing.kind == b"vdXProperty"
                    && let (Some(name), Some(value)) = std::mem::take(&mut e.property)
                {
                    e.attributes.insert(name, value);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if !ended {
        return Err(bad(&reader, "the layer stops before its end, so the file is cut short".into()));
    }
    Ok(drawing)
}

/// An entity starts as an `_Item` of the entity list at the top of the layer, not in a block.
fn is_entity(stack: &[Frame], frame: &Frame) -> bool {
    stack.len() == 2 && &*stack[1].kind == b"vdEntities" && &*frame.tag == b"O" && &*frame.field == b"_Item"
}

fn frame(e: &BytesStart<'_>) -> Result<Frame, String> {
    let (mut field, mut kind) = (Box::default(), Box::default());
    for attribute in e.attributes().with_checks(false) {
        let attribute = attribute.map_err(|e| e.to_string())?;
        match attribute.key.as_ref() {
            b"F" => field = attribute.value.as_ref().into(),
            b"T" => kind = attribute.value.as_ref().into(),
            _ => {}
        }
    }
    Ok(Frame { tag: e.name().as_ref().into(), field, kind })
}

/// A property's field, type and value, raw.
type Parts<'a> = (Cow<'a, [u8]>, Cow<'a, [u8]>, Cow<'a, [u8]>);

/// The `F`, `T` and `V` of a property, raw.
fn parts<'a>(e: &'a BytesStart<'_>) -> Result<Parts<'a>, String> {
    let (mut field, mut kind, mut value) = (Cow::Borrowed(&b""[..]), Cow::Borrowed(&b""[..]), Cow::Borrowed(&b""[..]));
    for attribute in e.attributes().with_checks(false) {
        let attribute = attribute.map_err(|e| e.to_string())?;
        match attribute.key.as_ref() {
            b"F" => field = attribute.value,
            b"T" => kind = attribute.value,
            b"V" => value = attribute.value,
            _ => {}
        }
    }
    Ok((field, kind, value))
}

fn property(e: &BytesStart<'_>, stack: &[Frame], entity: Option<&mut Entity>, log: &mut DeswikLog) -> Result<(), String> {
    if stack.len() == 2 && &*stack[1].tag == b"Activator" {
        let (field, _, value) = parts(e)?;
        if &*field == b"version" {
            log.versions.insert(text(&value)?);
        }
        return Ok(());
    }
    let Some(entity) = entity else { return Ok(()) };
    let reading = entity.is_string() || entity.is_mesh();
    match stack.len() {
        // The entity's own properties.
        3 => {
            let (field, kind, value) = parts(e)?;
            match &*field {
                b"Flag" if entity.is_string() => entity.closed = integer(&value)? & 1 == 1,
                b"VolumeInternal" if &*kind == b"Double" => entity.volume = Some(number(&value)?).filter(|v| v.is_finite() && *v > 0.0),
                _ => {}
            }
        }
        // An array's items.
        4 if reading => {
            let array = &stack[3];
            let (field, _, value) = parts(e)?;
            match (&*array.field, &*array.kind, &*field) {
                (b"VertexList", b"Vertexes", b"streamed") => {
                    let items = streamed(&value, 32)?;
                    entity.vertices.reserve(items.len() / 32);
                    for item in items[4..].as_chunks::<32>().0 {
                        entity.vertices.push([le_f64(&item[..8]), le_f64(&item[8..16]), le_f64(&item[16..24])]);
                        entity.bulges.push(le_f64(&item[24..]));
                    }
                }
                (b"VertexList", b"Vertexes", b"_Item") => {
                    let mut v = [0.0; 4];
                    match numbers(&value, &mut v)? {
                        3 | 4 => {}
                        n => return Err(format!("a vertex of {n} numbers")),
                    }
                    entity.vertices.push([v[0], v[1], v[2]]);
                    entity.bulges.push(v[3]);
                }
                (b"VertexList", b"gPoints", b"streamed") => {
                    let items = streamed(&value, 24)?;
                    entity.vertices.reserve(items.len() / 24);
                    entity.vertices.extend(items[4..].as_chunks::<24>().0.iter().map(|item| [le_f64(&item[..8]), le_f64(&item[8..16]), le_f64(&item[16..])]));
                }
                (b"VertexList", b"gPoints", b"_Item") => {
                    let mut v = [0.0; 3];
                    match numbers(&value, &mut v)? {
                        3 => entity.vertices.push(v),
                        n => return Err(format!("a point of {n} numbers")),
                    }
                }
                (b"FaceList", b"Int32Array", b"streamed") => {
                    let items = streamed(&value, 4)?;
                    entity.corners.extend(items[4..].as_chunks::<4>().0.iter().map(|b| i32::from_le_bytes(*b)));
                }
                (b"FaceList", b"Int32Array", b"_Item") => entity.corners.push(integer(&value)?),
                _ => {}
            }
        }
        // An attribute's name and value.
        5 if &*stack[3].field == b"XProperties" && &*stack[4].kind == b"vdXProperty" => {
            let (field, kind, value) = parts(e)?;
            match &*field {
                b"Name" => entity.property.0 = Some(text(&value)?),
                b"PropValue" => entity.property.1 = attribute(&kind, &value)?,
                _ => {}
            }
        }
        _ => {}
    }
    Ok(())
}

/// An attribute's value by its VectorDraw type: numbers as numbers, `Boolean` as true or false,
/// anything else as text, and none for empty text.
fn attribute(kind: &[u8], value: &[u8]) -> Result<Option<AttrValue>, String> {
    let value = text(value)?;
    Ok(match kind {
        b"Int16" | b"Int32" | b"Int64" | b"UInt16" | b"UInt32" | b"UInt64" | b"Byte" | b"Single" | b"Double" => {
            Some(value.trim().parse::<f64>().map_or(AttrValue::Text(value), AttrValue::Number))
        }
        b"Boolean" => Some(AttrValue::Bool(value.eq_ignore_ascii_case("true"))),
        _ if value.is_empty() => None,
        _ => Some(AttrValue::Text(value)),
    })
}

/// A streamed array: base64 of an `i32` count and the items, padded to the array's capacity. The
/// bytes come back with the count still in front and the padding cut off.
fn streamed(value: &[u8], item: usize) -> Result<Vec<u8>, String> {
    let mut bytes = STANDARD.decode(value).map_err(|e| format!("a streamed array that is not base64: {e}"))?;
    let [a, b, c, d, ..] = bytes[..] else { return Err("a streamed array without a count".into()) };
    let count = i32::from_le_bytes([a, b, c, d]);
    let Ok(count) = usize::try_from(count) else { return Err(format!("a streamed array of {count} items")) };
    let end = 4 + count * item;
    if bytes.len() < end {
        return Err(format!("a streamed array of {count} items holds only {} bytes", bytes.len()));
    }
    bytes.truncate(end);
    Ok(bytes)
}

fn le_f64(bytes: &[u8]) -> f64 {
    let mut word = [0; 8];
    word.copy_from_slice(&bytes[..8]);
    f64::from_le_bytes(word)
}

fn text(value: &[u8]) -> Result<String, String> {
    let raw = std::str::from_utf8(value).map_err(|e| e.to_string())?;
    quick_xml::escape::unescape(raw).map(Cow::into_owned).map_err(|e| e.to_string())
}

fn number(value: &[u8]) -> Result<f64, String> {
    let raw = std::str::from_utf8(value).map_err(|e| e.to_string())?;
    raw.trim().parse::<f64>().map_err(|_| format!("{raw:?} is not a number"))
}

/// A listed item's numbers, comma separated, into `into`; how many there were. Refused if more than fit.
fn numbers(value: &[u8], into: &mut [f64]) -> Result<usize, String> {
    let raw = std::str::from_utf8(value).map_err(|e| e.to_string())?;
    let (mut n, room) = (0, into.len());
    for part in raw.split(',') {
        let v = part.trim().parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{raw:?} is not a list of numbers"))?;
        *into.get_mut(n).ok_or_else(|| format!("{raw:?} holds more than {room} numbers"))? = v;
        n += 1;
    }
    Ok(n)
}

fn integer(value: &[u8]) -> Result<i32, String> {
    let raw = std::str::from_utf8(value).map_err(|e| e.to_string())?;
    raw.trim().parse::<i32>().map_err(|_| format!("{raw:?} is not a whole number"))
}

fn finish_entity(entity: Entity, layer: &str, drawing: &mut DeswikDrawing) -> Result<(), String> {
    if entity.is_string() {
        finish_string(entity, layer, drawing);
    } else if entity.is_mesh() {
        finish_mesh(entity, layer, drawing)?;
    } else {
        *drawing.log.skipped.entry(entity.kind).or_default() += 1;
    }
    Ok(())
}

fn finish_string(entity: Entity, layer: &str, drawing: &mut DeswikDrawing) {
    let Entity { mut closed, vertices, bulges, attributes, .. } = entity;
    let log = &mut drawing.log;
    let n = vertices.len();
    if n < 2 {
        log.short_strings += 1;
        return;
    }
    if closed && n < 3 {
        log.closed_too_short += 1;
        closed = false;
    }
    let segments = if closed { n } else { n - 1 };
    let mut line = Vec::with_capacity(n);
    for i in 0..n {
        let a = vertices[i];
        line.push(a);
        if i < segments && bulges[i] != 0.0 {
            let b = vertices[(i + 1) % n];
            let chords = arc_points([a[0], a[1]], [b[0], b[1]], bulges[i]);
            if !chords.is_empty() {
                log.arcs_to_chords += 1;
            }
            let steps = (chords.len() + 1) as f64;
            line.extend(chords.iter().enumerate().map(|(k, p)| [p[0], p[1], a[2] + (b[2] - a[2]) * (k + 1) as f64 / steps]));
        }
    }
    log.strings_read += 1;
    drawing.strings.push(DeswikString { layer: layer.to_string(), line: Polyline { vertices: line, closed }, attributes });
}

fn finish_mesh(entity: Entity, layer: &str, drawing: &mut DeswikDrawing) -> Result<(), String> {
    let Entity { vertices, corners, attributes, volume, .. } = entity;
    if corners.len() % 5 != 0 {
        return Err(format!("a polyface's face list holds {} numbers, not five to a face", corners.len()));
    }
    let log = &mut drawing.log;
    let count = vertices.len();
    let mut triangles = Vec::with_capacity(corners.len() / 5);
    for face in corners.as_chunks::<5>().0 {
        log.faces_read += 1;
        let mut c: Vec<u32> = Vec::with_capacity(4);
        for &i in &face[..4] {
            if i == 0 {
                continue;
            }
            let k = i.unsigned_abs();
            if k as usize > count {
                return Err(format!("a polyface face refers to vertex {k} of {count}"));
            }
            c.push(k - 1);
        }
        c.dedup();
        if c.len() > 1 && c.first() == c.last() {
            c.pop();
        }
        match c[..] {
            [a, b, c] => triangles.push([a, b, c]),
            [a, b, c, d] => {
                log.quads_split += 1;
                triangles.push([a, b, c]);
                triangles.push([a, c, d]);
            }
            _ => log.degenerate_dropped += 1,
        }
    }
    if triangles.is_empty() {
        log.empty_meshes += 1;
        return Ok(());
    }
    log.meshes_read += 1;
    drawing.meshes.push(DeswikMesh { layer: layer.to_string(), mesh: TriMesh::new(vertices, triangles), attributes, deswik_volume: volume });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ARC_SAGITTA;

    const E: f64 = 650_000.0;
    const N: f64 = 7_450_000.0;

    /// A streamed array as Deswik writes it: count, items, then zeros to a capacity of a power of two.
    fn stream(count: usize, items: &[u8]) -> String {
        let mut bytes = (count as i32).to_le_bytes().to_vec();
        bytes.extend_from_slice(items);
        bytes.resize(bytes.len().next_power_of_two().max(64), 0);
        STANDARD.encode(bytes)
    }

    fn f64s(values: &[f64]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn i32s(values: &[i32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn xproperties(items: &[(&str, &str, &str)]) -> String {
        let mut x = format!(r#"<O F="XProperties" T="vdXProperties"><P F="_Count" T="Int32" V="{}" />"#, items.len());
        for (name, kind, value) in items {
            let kind = if kind.is_empty() { String::new() } else { format!(r#" T="{kind}""#) };
            x += &format!(r#"<O F="_Item" T="vdXProperty"><P F="Name" V="{name}" /><P F="PropValue"{kind} V="{value}" /></O>"#);
        }
        x + "</O>"
    }

    fn layer_xml(entities: &[String]) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?><Begin><Activator><P F="version" V="10.1001.0.1" /><T type="vdPolyline" /></Activator><O T="vdEntities"><P F="_Count" T="Int32" V="{}" />{}</O></Begin>"#,
            entities.len(),
            entities.concat()
        )
    }

    /// A 100 m square toe at RL 100 with its vertices listed, closed, carrying projection details.
    fn listed_square() -> String {
        let props = xproperties(&[
            ("Projection Details", "", "Inside/Right at 45.Up to level 140"),
            ("Projection Surface", "", ""),
            ("Elevation", "Int32", "0"),
            ("Volume", "Double", "12.5"),
            ("_dw_IsHole", "Boolean", "False"),
            ("Note", "", "toe &amp; crest"),
        ]);
        let corners = [[E, N], [E + 100.0, N], [E + 100.0, N + 100.0], [E, N + 100.0]];
        let items: String = corners.iter().map(|c| format!(r#"<P F="_Item" T="Vertex" V="{},{},100 ,0" />"#, c[0], c[1])).collect();
        format!(
            r#"<O F="_Item" T="dwPolyline"><P F="Handle" T="vdHandle" V="116" />{props}<H F="Layer" V="114" /><P F="ExtrusionVector" T="Vector" V="-0.74,-0.26,0.62" /><P F="Flag" T="Enum" V="1" /><O F="VertexList" T="Vertexes"><P F="_Count" T="Int32" V="4" />{items}</O><P F="Defaults" V="" /></O>"#
        )
    }

    /// An open string with three streamed vertices climbing at a grade.
    fn streamed_line() -> String {
        let v = f64s(&[E, N, 100.0, 0.0, E + 50.0, N, 105.0, 0.0, E + 100.0, N + 10.0, 110.0, 0.0]);
        format!(r#"<O F="_Item" T="dwPolyline"><P F="Handle" T="vdHandle" V="117" /><O F="VertexList" T="Vertexes"><P F="streamed" V="{}" /></O></O>"#, stream(3, &v))
    }

    /// A tetrahedron with legs of `l` at a corner, streamed, five numbers to a face as Deswik writes
    /// them (the fourth corner repeats the first), with Deswik's volume of it.
    fn tetrahedron(l: f64) -> String {
        let v = f64s(&[E, N, 0.0, E + l, N, 0.0, E, N + l, 0.0, E, N, l]);
        let f = i32s(&[1, 3, 2, 1, -1, 1, 2, 4, 1, -1, 1, 4, 3, 1, -1, 2, 3, 4, 2, -1]);
        format!(
            r#"<O F="_Item" T="dwPolyface"><P F="Handle" T="vdHandle" V="118" />{}<O F="VertexList" T="gPoints"><P F="streamed" V="{}" /></O><O F="FaceList" T="Int32Array"><P F="streamed" V="{}" /></O><P F="VolumeInternal" T="Double" V="{}" /><P F="AreaInternal" T="Double" V="-1" /></O>"#,
            xproperties(&[("BENCH", "", "B1")]),
            stream(4, &v),
            stream(20, &f),
            l * l * l / 6.0
        )
    }

    #[test]
    fn a_layer_reads_its_strings_meshes_and_attributes_exactly() {
        let points = r#"<O F="_Item" T="dwPoints"><O F="Points" T="gPoints"><P F="streamed" V="AAAAAA==" /></O></O>"#.to_string();
        let text = r#"<O F="_Item" T="vdText"><P F="TextString" V="label" /></O>"#.to_string();
        let xml = layer_xml(&[listed_square(), streamed_line(), tetrahedron(30.0), points, text]);
        let drawing = parse_deswik_layer(r"PIT\DESIGN", xml.as_bytes()).unwrap();

        assert_eq!(drawing.strings.len(), 2);
        let square = &drawing.strings[0];
        assert_eq!(square.layer, r"PIT\DESIGN");
        assert!(square.line.closed, "Flag 1 closes it");
        assert_eq!(square.line.vertices, vec![[E, N, 100.0], [E + 100.0, N, 100.0], [E + 100.0, N + 100.0, 100.0], [E, N + 100.0, 100.0]], "world coordinates, bit for bit, whatever the extrusion vector");
        let a = &square.attributes;
        assert_eq!(a.get("Projection Details"), Some(&AttrValue::Text("Inside/Right at 45.Up to level 140".into())));
        assert_eq!(a.get("Elevation"), Some(&AttrValue::Number(0.0)));
        assert_eq!(a.get("Volume"), Some(&AttrValue::Number(12.5)));
        assert_eq!(a.get("_dw_IsHole"), Some(&AttrValue::Bool(false)));
        assert_eq!(a.get("Note"), Some(&AttrValue::Text("toe & crest".into())), "XML escapes undone");
        assert!(!a.contains_key("Projection Surface"), "empty values are left out");

        let line = &drawing.strings[1];
        assert!(!line.line.closed);
        assert_eq!(line.line.vertices, vec![[E, N, 100.0], [E + 50.0, N, 105.0], [E + 100.0, N + 10.0, 110.0]], "the padding after the count is not read");
        assert!(line.attributes.is_empty());

        assert_eq!(drawing.meshes.len(), 1);
        let tet = &drawing.meshes[0];
        assert_eq!(tet.mesh.triangles, vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]], "1-based corners, the repeated fourth dropped");
        assert_eq!(tet.attributes.get("BENCH"), Some(&AttrValue::Text("B1".into())));
        // Outward winding, so the signed volume is the tetrahedron's: 30³/6 = 4,500 m³.
        assert!((tet.mesh.signed_volume() - 4500.0).abs() < 1e-6, "{}", tet.mesh.signed_volume());
        assert_eq!(tet.deswik_volume, Some(4500.0));

        let log = &drawing.log;
        assert_eq!((log.strings_read, log.meshes_read, log.faces_read, log.quads_split, log.degenerate_dropped), (2, 1, 4, 0, 0));
        assert_eq!(log.versions, BTreeSet::from(["10.1001.0.1".to_string()]));
        assert_eq!(log.skipped, BTreeMap::from([("dwPoints".to_string(), 1), ("vdText".to_string(), 1)]));
    }

    #[test]
    fn a_bulged_segment_becomes_chords_on_its_arc_with_the_rl_running_straight() {
        // A semicircle (bulge 1) of radius 50 from (E, N) to (E + 100, N), RL 10 to 20.
        let v = f64s(&[E, N, 10.0, 1.0, E + 100.0, N, 20.0, 0.0]);
        let xml = layer_xml(&[format!(r#"<O F="_Item" T="dwPolyline"><O F="VertexList" T="Vertexes"><P F="streamed" V="{}" /></O></O>"#, stream(2, &v))]);
        let drawing = parse_deswik_layer("ARC", xml.as_bytes()).unwrap();
        let line = &drawing.strings[0].line.vertices;
        assert_eq!(drawing.log.arcs_to_chords, 1);
        assert!(line.len() > 50, "{} vertices", line.len());
        assert_eq!((line[0], *line.last().unwrap()), ([E, N, 10.0], [E + 100.0, N, 20.0]));
        for (k, p) in line.iter().enumerate() {
            let off = ((p[0] - E - 50.0).hypot(p[1] - N) - 50.0).abs();
            assert!(off < 1e-9, "vertex {k} is {off} m off the arc");
            // Positive bulge turns counter-clockwise from the start, so the arc runs south of the chord.
            assert!(p[1] <= N + 1e-9);
            let t = k as f64 / (line.len() - 1) as f64;
            assert!((p[2] - (10.0 + 10.0 * t)).abs() < 1e-9, "RL {} at step {k}", p[2]);
        }
        // Every chord within the sagitta of the arc: the middle of each chord is no more than 0.01 m in.
        for w in line.windows(2) {
            let mid = [(w[0][0] + w[1][0]) / 2.0 - E - 50.0, (w[0][1] + w[1][1]) / 2.0 - N];
            assert!(50.0 - mid[0].hypot(mid[1]) <= ARC_SAGITTA + 1e-12);
        }
    }

    #[test]
    fn a_quad_splits_a_degenerate_face_drops_and_a_listed_face_list_reads() {
        let points: String = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]].iter().map(|p| format!(r#"<P F="_Item" T="gPoint" V="{},{},50" />"#, E + p[0], N + p[1])).collect();
        let faces: String = [1, 2, 3, 4, -1, 1, 2, 2, 1, -1].iter().map(|i| format!(r#"<P F="_Item" T="Int32" V="{i}" />"#)).collect();
        let xml = layer_xml(&[format!(
            r#"<O F="_Item" T="dwPolyface"><O F="VertexList" T="gPoints"><P F="_Count" T="Int32" V="4" />{points}</O><O F="FaceList" T="Int32Array"><P F="_Count" T="Int32" V="10" />{faces}</O><P F="VolumeInternal" T="Double" V="-1" /></O>"#
        )]);
        let drawing = parse_deswik_layer("PAD", xml.as_bytes()).unwrap();
        let pad = &drawing.meshes[0];
        assert_eq!(pad.mesh.triangles, vec![[0, 1, 2], [0, 2, 3]]);
        assert!((pad.mesh.area() - 100.0).abs() < 1e-9);
        assert_eq!(pad.deswik_volume, None, "Deswik writes -1 where it worked out no volume");
        let log = &drawing.log;
        assert_eq!((log.faces_read, log.quads_split, log.degenerate_dropped), (2, 1, 1));
    }

    #[test]
    fn a_face_past_the_vertices_or_a_short_face_list_is_refused() {
        let v = f64s(&[E, N, 0.0, E + 1.0, N, 0.0, E, N + 1.0, 0.0]);
        let mesh = |faces: &[i32]| {
            layer_xml(&[format!(
                r#"<O F="_Item" T="dwPolyface"><O F="VertexList" T="gPoints"><P F="streamed" V="{}" /></O><O F="FaceList" T="Int32Array"><P F="streamed" V="{}" /></O></O>"#,
                stream(3, &v),
                stream(faces.len(), &i32s(faces))
            )])
        };
        let past = parse_deswik_layer("L", mesh(&[1, 2, 4, 1, -1]).as_bytes()).unwrap_err().to_string();
        assert!(past.contains("vertex 4 of 3") && past.contains("layer L"), "{past}");
        let short = parse_deswik_layer("L", mesh(&[1, 2, 3, 1]).as_bytes()).unwrap_err().to_string();
        assert!(short.contains("five to a face"), "{short}");
        let truncated = layer_xml(&[format!(r#"<O F="_Item" T="dwPolyline"><O F="VertexList" T="Vertexes"><P F="streamed" V="{}" /></O></O>"#, STANDARD.encode(i32s(&[3])))]);
        let truncated = parse_deswik_layer("L", truncated.as_bytes()).unwrap_err().to_string();
        assert!(truncated.contains("3 items holds only 4 bytes"), "{truncated}");
        let xml = layer_xml(&[listed_square()]);
        let cut = parse_deswik_layer("L", xml.strip_suffix("</O></Begin>").unwrap().as_bytes()).unwrap_err().to_string();
        assert!(cut.contains("cut short"), "{cut}");
    }

    #[test]
    fn entities_inside_a_block_are_definitions_and_not_read() {
        let block = format!(r#"<O F="_Item" T="vdBlock"><O F="Entities" T="vdEntities">{}</O></O>"#, streamed_line());
        let xml = format!(r#"<Begin><O T="vdBlocks">{block}</O><O T="vdEntities">{}</O></Begin>"#, listed_square());
        let drawing = parse_deswik_layer("L", xml.as_bytes()).unwrap();
        assert_eq!(drawing.strings.len(), 1);
        assert!(drawing.strings[0].line.closed, "only the square at the top of the layer");
    }

    /// A `.dcf` written as Deswik lays one out: the document, settings, and a layer to an entry.
    fn write_dcf(path: &Path, layers: &[(&str, String)]) {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, xml) in [("Design v1.dcf", layer_xml(&[]).as_str()), ("Design v1.dcf__DW_SETTINGS", layer_xml(&[]).as_str())] {
            zip.start_file(name, options).unwrap();
            zip.write_all(xml.as_bytes()).unwrap();
        }
        for (layer, xml) in layers {
            zip.start_file(format!("Design v1.dcf_{layer}"), options).unwrap();
            zip.write_all(xml.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn a_dcf_lists_its_layers_and_reads_only_those_named() {
        let dir = std::env::temp_dir().join(format!("zd-deswik-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // The file was renamed after Deswik saved it: its entries keep the old name.
        let path = dir.join("Design v2.dcf");
        let design = layer_xml(&[listed_square(), streamed_line()]);
        let solids = layer_xml(&[tetrahedron(30.0)]);
        write_dcf(&path, &[("0", layer_xml(&[])), ("3@@0 PIT^^STRIP 1^^DESIGN", design.clone()), ("3@@0 PIT^^STRIP 1^^SOLIDS", solids)]);

        let layers = list_deswik_layers(&path).unwrap();
        let names: Vec<&str> = layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["0", r"3.0 PIT\STRIP 1\DESIGN", r"3.0 PIT\STRIP 1\SOLIDS"], "as Deswik shows them; the document and its settings are not layers");
        assert_eq!(layers[1].bytes, design.len() as u64);

        let read = read_deswik_layers(&path, &[r"3.0 PIT\STRIP 1\SOLIDS".to_string()]).unwrap();
        assert_eq!((read.strings.len(), read.meshes.len(), read.log.layers_read), (0, 1, 1));
        assert_eq!(read.meshes[0].layer, r"3.0 PIT\STRIP 1\SOLIDS");

        let all = read_deswik_layers(&path, &[]).unwrap();
        assert_eq!((all.strings.len(), all.meshes.len(), all.log.layers_read), (2, 1, 3));
        assert_eq!(all.strings[0].layer, r"3.0 PIT\STRIP 1\DESIGN", "in the zip's order of layers, whichever read first");

        let missing = read_deswik_layers(&path, &[r"PIT\STRIP 2".to_string()]).unwrap_err().to_string();
        assert!(missing.contains(r"no layer named PIT\STRIP 2"), "{missing}");
        let not_deswik = dir.join("plain.zip");
        {
            use std::io::Write;
            let mut zip = zip::ZipWriter::new(File::create(&not_deswik).unwrap());
            zip.start_file("solid.00t", zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        assert!(list_deswik_layers(&not_deswik).unwrap_err().to_string().contains("not a Deswik drawing"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Changes the checksum an entry's headers give (local and central), leaving its contents alone.
    fn break_checksum(path: &Path, entry: &str) {
        let mut bytes = std::fs::read(path).unwrap();
        let u16_at = |b: &[u8], at: usize| u16::from_le_bytes([b[at], b[at + 1]]) as usize;
        let mut changed = 0;
        for at in 0..bytes.len().saturating_sub(46) {
            let (crc, name) = match &bytes[at..at + 4] {
                b"PK\x03\x04" => (at + 14, at + 30..at + 30 + u16_at(&bytes, at + 26)),
                b"PK\x01\x02" => (at + 16, at + 46..at + 46 + u16_at(&bytes, at + 28)),
                _ => continue,
            };
            if bytes.get(name) == Some(entry.as_bytes()) {
                bytes[crc] ^= 0xff;
                changed += 1;
            }
        }
        assert_eq!(changed, 2, "the local and central headers");
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_layer_whose_checksum_is_wrong_reads_if_it_runs_whole_and_is_named() {
        let dir = std::env::temp_dir().join(format!("zd-deswik-crc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Design v1.dcf");
        write_dcf(&path, &[("DESIGN", layer_xml(&[listed_square()])), ("SOLIDS", layer_xml(&[tetrahedron(30.0)]))]);
        break_checksum(&path, "Design v1.dcf_SOLIDS");
        let read = read_deswik_layers(&path, &[]).unwrap();
        assert_eq!((read.strings.len(), read.meshes.len()), (1, 1));
        assert!((read.meshes[0].mesh.signed_volume() - 4500.0).abs() < 1e-6, "the same mesh as with a right checksum");
        assert_eq!(read.log.checksum_mismatches, ["SOLIDS"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
