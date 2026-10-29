//! ASCII DXF (decision 0009). Reads triangulations (`3DFACE` and polyface meshes) and strings
//! (`POLYLINE`, `LWPOLYLINE` and `LINE`) from the ENTITIES section of a file of any version,
//! passing over handles, subclass markers and extended data; writes DXF R12.
//!
//! A 3DFACE stores its own corner coordinates, so shared corners are welded by exact f64
//! equality on the way in. That is the format's representation, not a repair; tolerance welding
//! belongs to the import repair step. Everything else the reader converts (arcs to chords, tilted
//! planes to world coordinates) or passes over is counted in the log.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::f64::consts::PI;
use std::io::Write;
use std::path::Path;

use zd_core::{Polyline, TriMesh, is_zero_area};

use crate::IoError;

/// Arcs (bulged segments) become chords lying no farther than this from the arc, in metres.
pub const ARC_SAGITTA: f64 = 0.01;
/// The most chords one arc becomes.
const ARC_MAX_CHORDS: usize = 4096;
const UP: [f64; 3] = [0.0, 0.0, 1.0];

/// What a DXF file holds: one triangulation of all its faces, and its strings.
#[derive(Debug, Clone)]
pub struct DxfDrawing {
    pub mesh: TriMesh,
    pub strings: Vec<DxfString>,
    pub log: DxfLog,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfString {
    pub layer: String,
    pub line: Polyline,
}

/// What the reader did to get from entities to faces and strings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DxfLog {
    /// 3DFACEs and polyface faces read.
    pub faces_read: usize,
    /// Four-cornered faces split into two triangles.
    pub quads_split: usize,
    /// Triangles dropped for a repeated corner or zero area.
    pub degenerate_dropped: usize,
    /// Triangles dropped because another triangle already used the same three vertices.
    pub duplicates_dropped: usize,
    /// Polyface meshes (POLYLINE flag 64); their faces are counted in `faces_read`.
    pub polyface_meshes: usize,
    pub strings_read: usize,
    /// Bulged segments replaced by chords within [`ARC_SAGITTA`] of the arc.
    pub arcs_to_chords: usize,
    /// Strings drawn in a tilted plane (an extrusion direction other than +Z), moved to world
    /// coordinates.
    pub tilted_moved: usize,
    /// Strings with fewer than two vertices, passed over.
    pub short_strings: usize,
    /// Closed strings with only two vertices, read as open.
    pub closed_too_short: usize,
    /// Entities of other types, passed over, counted by type.
    pub skipped: BTreeMap<String, usize>,
}

pub fn read_dxf(path: &Path) -> Result<DxfDrawing, IoError> {
    parse_dxf(&std::fs::read(path)?)
}

/// Parses the ENTITIES section. Entities inside BLOCKS are definitions, not geometry, and are
/// passed over.
pub fn parse_dxf(bytes: &[u8]) -> Result<DxfDrawing, IoError> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err(IoError::Unsupported("binary DXF; save as ASCII DXF".into()));
    }

    let mut reader = Reader::default();
    let mut pairs = Pairs::new(bytes);
    let mut section: Option<&[u8]> = None;
    let mut expecting_section_name = false;
    let mut entity: Option<&[u8]> = None;
    let mut groups: Vec<Group<'_>> = Vec::new();

    while let Some(group) = pairs.next()? {
        if group.code == 0 {
            if let Some(kind) = entity.take() {
                reader.entity(kind, &groups)?;
            }
            groups.clear();
            match group.value {
                b"SECTION" => expecting_section_name = true,
                b"ENDSEC" => {
                    reader.end_polyline();
                    section = None;
                }
                b"EOF" => break,
                kind if section == Some(b"ENTITIES".as_slice()) => entity = Some(kind),
                _ => {}
            }
            continue;
        }
        if group.code == 2 && expecting_section_name {
            section = Some(group.value);
            expecting_section_name = false;
            continue;
        }
        if entity.is_some() {
            groups.push(group);
        }
    }
    if let Some(kind) = entity {
        reader.entity(kind, &groups)?;
    }
    reader.end_polyline();
    Ok(reader.finish())
}

/// One thing to write: a string as a 3D POLYLINE, or a mesh as 3DFACEs.
#[derive(Debug, Clone, Copy)]
pub enum DxfEntity<'a> {
    String(&'a Polyline),
    Mesh(&'a TriMesh),
}

/// Writes DXF R12: a header with the drawing's extents, a layer table, then each entity on its
/// layer. Coordinates use Rust's shortest round-trip formatting, so every f64 reads back exactly.
pub fn write_dxf(path: &Path, entities: &[(&str, DxfEntity<'_>)]) -> Result<(), IoError> {
    check_layer_names(entities)?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    write_dxf_to(&mut out, entities)?;
    out.flush()?;
    Ok(())
}

/// Writes a mesh as DXF R12 `3DFACE` entities (triangles repeat their third corner).
pub fn write_dxf_3dfaces(path: &Path, mesh: &TriMesh, layer: &str) -> Result<(), IoError> {
    write_dxf(path, &[(layer, DxfEntity::Mesh(mesh))])
}

/// The DXF R12 text of [`write_dxf`], for callers that keep it in memory.
pub fn format_dxf(entities: &[(&str, DxfEntity<'_>)]) -> Result<String, IoError> {
    check_layer_names(entities)?;
    let mut out = Vec::new();
    write_dxf_to(&mut out, entities)?;
    Ok(String::from_utf8(out).expect("the writer writes text"))
}

/// A blank name goes on layer 0; a line break would end the value early, so it is refused.
fn check_layer_names(entities: &[(&str, DxfEntity<'_>)]) -> Result<(), IoError> {
    match entities.iter().find(|(layer, _)| layer.chars().any(char::is_control)) {
        Some((layer, _)) => Err(IoError::Unsupported(format!("layer name {layer:?} holds a control character"))),
        None => Ok(()),
    }
}

fn layer_name(layer: &str) -> &str {
    if layer.trim().is_empty() { "0" } else { layer }
}

fn write_dxf_to(out: &mut impl Write, entities: &[(&str, DxfEntity<'_>)]) -> std::io::Result<()> {
    let mut layers: Vec<&str> = Vec::new();
    let mut seen = HashSet::new();
    for (layer, _) in entities {
        if seen.insert(layer_name(layer)) {
            layers.push(layer_name(layer));
        }
    }

    write!(out, "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n")?;
    if let Some((min, max)) = extents(entities) {
        write!(out, "9\n$EXTMIN\n10\n{}\n20\n{}\n30\n{}\n", min[0], min[1], min[2])?;
        write!(out, "9\n$EXTMAX\n10\n{}\n20\n{}\n30\n{}\n", max[0], max[1], max[2])?;
    }
    write!(out, "0\nENDSEC\n")?;

    write!(out, "0\nSECTION\n2\nTABLES\n")?;
    write!(out, "0\nTABLE\n2\nLTYPE\n70\n1\n0\nLTYPE\n2\nCONTINUOUS\n70\n0\n3\nSolid line\n72\n65\n73\n0\n40\n0.0\n0\nENDTAB\n")?;
    write!(out, "0\nTABLE\n2\nLAYER\n70\n{}\n", layers.len())?;
    for layer in &layers {
        write!(out, "0\nLAYER\n2\n{layer}\n70\n0\n62\n7\n6\nCONTINUOUS\n")?;
    }
    write!(out, "0\nENDTAB\n0\nENDSEC\n")?;

    write!(out, "0\nSECTION\n2\nENTITIES\n")?;
    for (layer, entity) in entities {
        let layer = layer_name(layer);
        match entity {
            DxfEntity::String(line) => write_polyline(out, layer, line)?,
            DxfEntity::Mesh(mesh) => write_faces(out, layer, mesh)?,
        }
    }
    write!(out, "0\nENDSEC\n0\nEOF\n")
}

/// A 3D POLYLINE (flag 8, plus 1 when closed) with one 3D vertex (flag 32) per string vertex.
fn write_polyline(out: &mut impl Write, layer: &str, line: &Polyline) -> std::io::Result<()> {
    let flags = 8 | u8::from(line.closed);
    write!(out, "0\nPOLYLINE\n8\n{layer}\n66\n1\n10\n0.0\n20\n0.0\n30\n0.0\n70\n{flags}\n")?;
    for p in &line.vertices {
        write!(out, "0\nVERTEX\n8\n{layer}\n10\n{}\n20\n{}\n30\n{}\n70\n32\n", p[0], p[1], p[2])?;
    }
    write!(out, "0\nSEQEND\n8\n{layer}\n")
}

fn write_faces(out: &mut impl Write, layer: &str, mesh: &TriMesh) -> std::io::Result<()> {
    for tri in &mesh.triangles {
        let [a, b, c] = tri.map(|i| mesh.vertices[i as usize]);
        write!(out, "0\n3DFACE\n8\n{layer}\n")?;
        for (corner, p) in [a, b, c, c].iter().enumerate() {
            write!(out, "1{corner}\n{}\n2{corner}\n{}\n3{corner}\n{}\n", p[0], p[1], p[2])?;
        }
    }
    Ok(())
}

fn extents(entities: &[(&str, DxfEntity<'_>)]) -> Option<([f64; 3], [f64; 3])> {
    let points = entities.iter().flat_map(|(_, entity)| match entity {
        DxfEntity::String(line) => line.vertices.as_slice(),
        DxfEntity::Mesh(mesh) => mesh.vertices.as_slice(),
    });
    points.fold(None, |acc, p| match acc {
        None => Some((*p, *p)),
        Some((min, max)) => Some(([0, 1, 2].map(|i| min[i].min(p[i])), [0, 1, 2].map(|i| max[i].max(p[i])))),
    })
}

/// One group: its code and value, and the 1-based line the value is on.
#[derive(Debug, Clone, Copy)]
struct Group<'a> {
    code: i32,
    value: &'a [u8],
    line: usize,
}

/// The file's lines, numbered from 0.
type Lines<'a> = std::iter::Enumerate<std::slice::Split<'a, u8, fn(&u8) -> bool>>;

struct Pairs<'a> {
    lines: Lines<'a>,
}

impl<'a> Pairs<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let newline: fn(&u8) -> bool = |b| *b == b'\n';
        Pairs { lines: bytes.split(newline).enumerate() }
    }

    fn next(&mut self) -> Result<Option<Group<'a>>, IoError> {
        let Some((code_line, code)) = self.lines.next() else {
            return Ok(None);
        };
        let code = code.trim_ascii();
        // Group codes are never blank, so a blank code line is only allowed as trailing padding.
        if code.is_empty() {
            if self.lines.all(|(_, rest)| rest.trim_ascii().is_empty()) {
                return Ok(None);
            }
            return Err(IoError::Format { line: code_line + 1, message: "blank group code".into() });
        }
        let code: i32 = parse_ascii(code).ok_or_else(|| IoError::Format {
            line: code_line + 1,
            message: format!("expected a group code, found '{}'", String::from_utf8_lossy(code)),
        })?;
        let Some((value_line, value)) = self.lines.next() else {
            return Err(IoError::Format { line: code_line + 1, message: "group code without a value".into() });
        };
        Ok(Some(Group { code, value: value.trim_ascii(), line: value_line + 1 }))
    }
}

/// The plane a 2D polyline is drawn in: its elevation and extrusion direction (the object
/// coordinate system of the DXF reference).
#[derive(Debug, Clone, Copy)]
struct Plane {
    elevation: f64,
    extrusion: [f64; 3],
}

/// A string being read: vertices with the bulge of the segment that starts at each.
#[derive(Debug)]
struct StringParts {
    layer: String,
    closed: bool,
    /// `None` for a 3D polyline, whose vertices are in world coordinates and never bulge.
    plane: Option<Plane>,
    vertices: Vec<[f64; 3]>,
    bulges: Vec<f64>,
}

#[derive(Debug)]
enum OpenPolyline {
    String(StringParts),
    Polyface { positions: Vec<[f64; 3]> },
    /// A polygon mesh (flag 16): its vertices are read past.
    Skipped,
}

#[derive(Default)]
struct Reader {
    faces: FaceBuilder,
    strings: Vec<DxfString>,
    polyline: Option<OpenPolyline>,
    log: DxfLog,
}

impl Reader {
    fn entity(&mut self, kind: &[u8], groups: &[Group<'_>]) -> Result<(), IoError> {
        match kind {
            b"VERTEX" if self.polyline.is_some() => return self.vertex(groups),
            b"SEQEND" if self.polyline.is_some() => {
                self.end_polyline();
                return Ok(());
            }
            // A polyline missing its SEQEND ends at the next entity.
            _ => self.end_polyline(),
        }
        match kind {
            b"3DFACE" => self.face(groups),
            b"POLYLINE" => self.polyline(groups),
            b"LWPOLYLINE" => self.lwpolyline(groups),
            b"LINE" => {
                let line = Polyline { vertices: vec![point(groups, 0)?, point(groups, 1)?], closed: false };
                self.push_string(layer(groups), line);
                Ok(())
            }
            other => {
                self.skip(&String::from_utf8_lossy(other));
                Ok(())
            }
        }
    }

    fn skip(&mut self, kind: &str) {
        *self.log.skipped.entry(kind.to_string()).or_default() += 1;
    }

    fn face(&mut self, groups: &[Group<'_>]) -> Result<(), IoError> {
        let mut corners = [[0.0; 3]; 4];
        let mut seen = [false; 4];
        for g in groups {
            // 10/20/30 = corner 0 x/y/z, 11/21/31 = corner 1, ... 13/23/33 = corner 3.
            if (10..=33).contains(&g.code) && g.code % 10 <= 3 {
                let corner = (g.code % 10) as usize;
                corners[corner][(g.code / 10 - 1) as usize] = number(g)?;
                seen[corner] = true;
            }
        }
        if !(seen[0] && seen[1] && seen[2]) {
            self.log.faces_read += 1;
            self.log.degenerate_dropped += 1;
            return Ok(());
        }
        let d = if seen[3] { corners[3] } else { corners[2] };
        self.faces.push_face([corners[0], corners[1], corners[2], d], &mut self.log);
        Ok(())
    }

    fn polyline(&mut self, groups: &[Group<'_>]) -> Result<(), IoError> {
        let flags = integer(groups, 70)?.unwrap_or(0);
        self.polyline = Some(if flags & 64 != 0 {
            self.log.polyface_meshes += 1;
            OpenPolyline::Polyface { positions: Vec::new() }
        } else if flags & 16 != 0 {
            self.skip("POLYLINE (polygon mesh)");
            OpenPolyline::Skipped
        } else {
            // A 2D polyline's elevation is the z of its own location (group 30).
            let plane = (flags & 8 == 0).then(|| -> Result<Plane, IoError> {
                Ok(Plane { elevation: value(groups, 30)?.unwrap_or(0.0), extrusion: extrusion(groups)? })
            });
            OpenPolyline::String(StringParts {
                layer: layer(groups),
                closed: flags & 1 != 0,
                plane: plane.transpose()?,
                vertices: Vec::new(),
                bulges: Vec::new(),
            })
        });
        Ok(())
    }

    fn vertex(&mut self, groups: &[Group<'_>]) -> Result<(), IoError> {
        let flags = integer(groups, 70)?.unwrap_or(0);
        match self.polyline.as_mut() {
            Some(OpenPolyline::String(parts)) => {
                // Spline frame control points (flag 16) are not on the curve.
                if flags & 16 == 0 {
                    parts.vertices.push(point(groups, 0)?);
                    parts.bulges.push(value(groups, 42)?.unwrap_or(0.0));
                }
            }
            Some(OpenPolyline::Polyface { positions }) if flags & 64 != 0 => positions.push(point(groups, 0)?),
            Some(OpenPolyline::Polyface { positions }) => {
                // A face record: up to four 1-based vertex numbers, negative where the edge is hidden.
                let mut corners = Vec::with_capacity(4);
                for code in 71..=74 {
                    let Some(index) = integer(groups, code)?.filter(|&i| i != 0) else { continue };
                    let at = groups.iter().find(|g| g.code == code).map_or(0, |g| g.line);
                    let p = positions.get(index.unsigned_abs() as usize - 1).ok_or_else(|| IoError::Format {
                        line: at,
                        message: format!("polyface face refers to vertex {} of {}", index.abs(), positions.len()),
                    })?;
                    corners.push(*p);
                }
                match corners[..] {
                    [a, b, c] => self.faces.push_face([a, b, c, c], &mut self.log),
                    [a, b, c, d] => self.faces.push_face([a, b, c, d], &mut self.log),
                    _ => {
                        self.log.faces_read += 1;
                        self.log.degenerate_dropped += 1;
                    }
                }
            }
            Some(OpenPolyline::Skipped) | None => {}
        }
        Ok(())
    }

    fn lwpolyline(&mut self, groups: &[Group<'_>]) -> Result<(), IoError> {
        let plane = Plane { elevation: value(groups, 38)?.unwrap_or(0.0), extrusion: extrusion(groups)? };
        let mut parts = StringParts {
            layer: layer(groups),
            closed: integer(groups, 70)?.unwrap_or(0) & 1 != 0,
            plane: Some(plane),
            vertices: Vec::new(),
            bulges: Vec::new(),
        };
        // Each 10 starts a vertex; its 20 and 42 follow it.
        for g in groups {
            match g.code {
                10 => {
                    parts.vertices.push([number(g)?, 0.0, 0.0]);
                    parts.bulges.push(0.0);
                }
                20 if !parts.vertices.is_empty() => parts.vertices.last_mut().unwrap()[1] = number(g)?,
                42 if !parts.bulges.is_empty() => *parts.bulges.last_mut().unwrap() = number(g)?,
                _ => {}
            }
        }
        self.end_string(parts);
        Ok(())
    }

    fn end_polyline(&mut self) {
        if let Some(OpenPolyline::String(parts)) = self.polyline.take() {
            self.end_string(parts);
        }
    }

    /// A 2D string's arcs become chords and its vertices go to world coordinates.
    fn end_string(&mut self, parts: StringParts) {
        let StringParts { layer, closed, plane, mut vertices, bulges } = parts;
        if let Some(plane) = plane {
            let n = vertices.len();
            let segments = if closed { n } else { n.saturating_sub(1) };
            let mut curve = Vec::with_capacity(n);
            for i in 0..n {
                let a = vertices[i];
                curve.push([a[0], a[1], plane.elevation]);
                if i < segments && bulges[i] != 0.0 {
                    let b = vertices[(i + 1) % n];
                    if a[..2] != b[..2] {
                        self.log.arcs_to_chords += 1;
                    }
                    let chords = arc_points([a[0], a[1]], [b[0], b[1]], bulges[i]);
                    curve.extend(chords.into_iter().map(|p| [p[0], p[1], plane.elevation]));
                }
            }
            if plane.extrusion != UP {
                self.log.tilted_moved += 1;
                curve = curve.into_iter().map(|p| ocs_to_world(plane.extrusion, p)).collect();
            }
            vertices = curve;
        }
        self.push_string(layer, Polyline { vertices, closed });
    }

    fn push_string(&mut self, layer: String, mut line: Polyline) {
        if line.vertices.len() < 2 {
            self.log.short_strings += 1;
            return;
        }
        if line.closed && line.vertices.len() < 3 {
            self.log.closed_too_short += 1;
            line.closed = false;
        }
        self.log.strings_read += 1;
        self.strings.push(DxfString { layer, line });
    }

    fn finish(self) -> DxfDrawing {
        DxfDrawing { mesh: TriMesh::new(self.faces.vertices, self.faces.triangles), strings: self.strings, log: self.log }
    }
}

/// The points strictly between `a` and `b` on the arc of `bulge` (the tangent of a quarter of the
/// included angle, positive counter-clockwise), with no chord farther than [`ARC_SAGITTA`] from
/// the arc. Worked about `a`, so mine grid coordinates keep their precision.
fn arc_points(a: [f64; 2], b: [f64; 2], bulge: f64) -> Vec<[f64; 2]> {
    let (ux, uy) = (b[0] - a[0], b[1] - a[1]);
    let chord = ux.hypot(uy);
    if chord == 0.0 || !bulge.is_finite() {
        return Vec::new();
    }
    let sweep = 4.0 * bulge.atan();
    let radius = chord * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
    // The centre lies on the chord's perpendicular bisector, to the left for a positive bulge
    // under a semicircle.
    let h = chord * (1.0 - bulge * bulge) / (4.0 * bulge);
    let (cx, cy) = (ux / 2.0 - uy / chord * h, uy / 2.0 + ux / chord * h);
    let start = (-cy).atan2(-cx);
    let step = if ARC_SAGITTA < radius { 2.0 * (1.0 - ARC_SAGITTA / radius).acos() } else { PI };
    let n = ((sweep.abs() / step).ceil() as usize).clamp(1, ARC_MAX_CHORDS);
    (1..n)
        .map(|k| {
            let t = start + sweep * k as f64 / n as f64;
            [a[0] + cx + radius * t.cos(), a[1] + cy + radius * t.sin()]
        })
        .collect()
}

/// A point in an entity's object coordinate system to world coordinates, by the DXF reference's
/// arbitrary axis algorithm.
fn ocs_to_world(extrusion: [f64; 3], p: [f64; 3]) -> [f64; 3] {
    let n = normalise(extrusion);
    let ax = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 { cross([0.0, 1.0, 0.0], n) } else { cross(UP, n) };
    let ax = normalise(ax);
    let ay = normalise(cross(n, ax));
    [0, 1, 2].map(|i| p[0] * ax[i] + p[1] * ay[i] + p[2] * n[i])
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalise(v: [f64; 3]) -> [f64; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v.map(|c| c / length)
}

/// A finite number, or a format error on its line.
fn number(g: &Group<'_>) -> Result<f64, IoError> {
    parse_ascii::<f64>(g.value).filter(|v| v.is_finite()).ok_or_else(|| IoError::Format {
        line: g.line,
        message: format!("bad number '{}'", String::from_utf8_lossy(g.value)),
    })
}

/// The first value of a group code, if the entity has it.
fn value(groups: &[Group<'_>], code: i32) -> Result<Option<f64>, IoError> {
    groups.iter().find(|g| g.code == code).map(number).transpose()
}

fn integer(groups: &[Group<'_>], code: i32) -> Result<Option<i64>, IoError> {
    let Some(g) = groups.iter().find(|g| g.code == code) else { return Ok(None) };
    parse_ascii::<i64>(g.value).map(Some).ok_or_else(|| IoError::Format {
        line: g.line,
        message: format!("bad integer '{}'", String::from_utf8_lossy(g.value)),
    })
}

/// Point `index` of an entity: x, y and z from groups 10, 20 and 30 plus the index; 0 where absent.
fn point(groups: &[Group<'_>], index: i32) -> Result<[f64; 3], IoError> {
    let mut p = [0.0; 3];
    for (axis, code) in [10, 20, 30].into_iter().enumerate() {
        p[axis] = value(groups, code + index)?.unwrap_or(0.0);
    }
    Ok(p)
}

fn extrusion(groups: &[Group<'_>]) -> Result<[f64; 3], IoError> {
    let mut n = UP;
    for (axis, code) in [210, 220, 230].into_iter().enumerate() {
        if let Some(v) = value(groups, code)? {
            n[axis] = v;
        }
    }
    if n == [0.0; 3] {
        let line = groups.iter().find(|g| g.code == 210).map_or(0, |g| g.line);
        return Err(IoError::Format { line, message: "extrusion direction of zero length".into() });
    }
    Ok(n)
}

/// The entity's layer (group 8), 0 when it has none. Names that are not UTF-8 are read as Latin-1.
fn layer(groups: &[Group<'_>]) -> String {
    match groups.iter().find(|g| g.code == 8) {
        Some(g) => match std::str::from_utf8(g.value) {
            Ok(name) => name.to_string(),
            Err(_) => g.value.iter().map(|&b| char::from(b)).collect(),
        },
        None => "0".into(),
    }
}

#[derive(Default)]
struct FaceBuilder {
    vertices: Vec<[f64; 3]>,
    lookup: HashMap<[u64; 3], u32>,
    triangles: Vec<[u32; 3]>,
    seen: HashSet<[u32; 3]>,
}

impl FaceBuilder {
    /// A face's corners; a triangle repeats its third.
    fn push_face(&mut self, [a, b, c, d]: [[f64; 3]; 4], log: &mut DxfLog) {
        log.faces_read += 1;
        let ia = self.vertex(a);
        let ib = self.vertex(b);
        let ic = self.vertex(c);
        let id = self.vertex(d);
        if id == ic {
            self.push_triangle([ia, ib, ic], log);
        } else {
            log.quads_split += 1;
            self.push_triangle([ia, ib, ic], log);
            self.push_triangle([ia, ic, id], log);
        }
    }

    fn vertex(&mut self, p: [f64; 3]) -> u32 {
        // Treat -0.0 and 0.0 as the same coordinate.
        let key = p.map(|v| if v == 0.0 { 0u64 } else { v.to_bits() });
        *self.lookup.entry(key).or_insert_with(|| {
            self.vertices.push(p);
            (self.vertices.len() - 1) as u32
        })
    }

    fn push_triangle(&mut self, tri: [u32; 3], log: &mut DxfLog) {
        let [a, b, c] = tri;
        if a == b || b == c || a == c || is_zero_area(tri.map(|i| self.vertices[i as usize])) {
            log.degenerate_dropped += 1;
            return;
        }
        let mut sorted = tri;
        sorted.sort_unstable();
        if !self.seen.insert(sorted) {
            log.duplicates_dropped += 1;
            return;
        }
        self.triangles.push(tri);
    }
}

fn parse_ascii<T: std::str::FromStr>(bytes: &[u8]) -> Option<T> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(corners: [[f64; 3]; 4]) -> String {
        let mut s = String::from("0\n3DFACE\n8\nTOPO\n");
        for (i, p) in corners.iter().enumerate() {
            s.push_str(&format!("1{i}\n{}\n2{i}\n{}\n3{i}\n{}\n", p[0], p[1], p[2]));
        }
        s
    }

    fn entities(body: &str) -> String {
        format!("0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n")
    }

    fn dxf(faces: &[[[f64; 3]; 4]]) -> String {
        entities(&faces.iter().map(|f| face(*f)).collect::<String>())
    }

    const A: [f64; 3] = [650_000.125, 7_450_000.5, 101.25];
    const B: [f64; 3] = [650_010.125, 7_450_000.5, 102.0];
    const C: [f64; 3] = [650_010.125, 7_450_010.5, 103.0];
    const D: [f64; 3] = [650_000.125, 7_450_010.5, 104.0];

    #[test]
    fn triangles_and_quads_weld_shared_corners() {
        let text = dxf(&[[A, B, C, C], [A, C, D, D]]);
        let import = parse_dxf(text.as_bytes()).unwrap();
        assert_eq!(import.mesh.vertices.len(), 4);
        assert_eq!(import.mesh.triangles.len(), 2);
        assert_eq!(import.mesh.vertices[0], A, "coordinates must survive exactly as f64");
        assert_eq!(import.log.faces_read, 2);
        assert_eq!(import.log.quads_split, 0);

        let quad = parse_dxf(dxf(&[[A, B, C, D]]).as_bytes()).unwrap();
        assert_eq!(quad.mesh.triangles.len(), 2);
        assert_eq!(quad.log.quads_split, 1);
        // Plan area of the 10 x 10 quad.
        let plan: f64 = quad.mesh.triangles.iter().map(|&t| 0.5 * quad.mesh.triangle_cross(t)[2].abs()).sum();
        assert!((plan - 100.0).abs() < 1e-9);
    }

    #[test]
    fn degenerate_and_duplicate_faces_are_dropped_and_logged() {
        let collinear = [A, B, [650_020.125, 7_450_000.5, 102.75], [650_020.125, 7_450_000.5, 102.75]];
        let text = dxf(&[[A, B, C, C], [C, B, A, A], [A, A, B, B], collinear]);
        let import = parse_dxf(text.as_bytes()).unwrap();
        assert_eq!(import.mesh.triangles.len(), 1);
        assert_eq!(import.log.duplicates_dropped, 1);
        assert_eq!(import.log.degenerate_dropped, 2);
    }

    #[test]
    fn faces_in_blocks_are_ignored_and_crlf_is_accepted() {
        let block = format!("0\nSECTION\n2\nBLOCKS\n{}0\nENDSEC\n", face([A, B, C, C]));
        let text = format!("{block}{}", dxf(&[[A, C, D, D]])).replace('\n', "\r\n");
        let import = parse_dxf(text.as_bytes()).unwrap();
        assert_eq!(import.mesh.triangles.len(), 1);
        assert!(import.log.skipped.is_empty());
    }

    #[test]
    fn mesh_write_then_read_round_trips_exactly() {
        let mesh = TriMesh::new(vec![A, B, C, D], vec![[0, 1, 2], [0, 2, 3]]);
        let dir = std::env::temp_dir().join(format!("zd-io-dxf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.dxf");
        write_dxf_3dfaces(&path, &mesh, "TEST").unwrap();
        let back = read_dxf(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(back.mesh, mesh);
    }

    #[test]
    fn bad_coordinate_reports_line() {
        let text = dxf(&[[A, B, C, C]]).replacen("650000.125", "65O000.125", 1);
        match parse_dxf(text.as_bytes()) {
            Err(IoError::Format { line, .. }) => assert_eq!(line, 10),
            other => panic!("expected a format error, got {other:?}"),
        }
        let infinite = dxf(&[[A, B, C, C]]).replacen("650000.125", "inf", 1);
        assert!(matches!(parse_dxf(infinite.as_bytes()), Err(IoError::Format { line: 10, .. })));
    }

    fn bits(line: &Polyline) -> Vec<u64> {
        line.vertices.iter().flatten().map(|v| v.to_bits()).collect()
    }

    #[test]
    fn strings_and_faces_write_as_r12_and_read_back_bit_for_bit() {
        // Coordinates with long binary fractions, which only shortest round-trip formatting keeps.
        let toe = Polyline { vertices: vec![[650_000.1, 7_450_000.2, 100.3], [650_100.0 / 3.0 * 3.0 + 0.7, 7_450_000.9, 99.1]], closed: false };
        let crest = Polyline { vertices: vec![A, B, C, [650_000.1 + 1e-9, 7_450_010.5, 1.0 / 3.0]], closed: true };
        let mesh = TriMesh::new(vec![A, B, C], vec![[0, 1, 2]]);
        let text = format_dxf(&[("design toes", DxfEntity::String(&toe)), ("design\\crests", DxfEntity::String(&crest)), ("", DxfEntity::Mesh(&mesh))]).unwrap();

        assert!(text.starts_with("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n"));
        assert!(text.contains("9\n$EXTMIN\n10\n650000.1\n20\n7450000.2\n30\n0.3333333333333333\n"));
        assert!(text.contains("0\nTABLE\n2\nLAYER\n70\n3\n0\nLAYER\n2\ndesign toes\n"));
        let back = parse_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.strings.len(), 2);
        assert_eq!((back.strings[0].layer.as_str(), back.strings[1].layer.as_str()), ("design toes", "design\\crests"));
        assert_eq!((back.strings[0].line.closed, back.strings[1].line.closed), (false, true));
        assert_eq!(bits(&back.strings[0].line), bits(&toe));
        assert_eq!(bits(&back.strings[1].line), bits(&crest));
        assert_eq!(back.mesh, mesh);
        assert_eq!(back.log.strings_read, 2);
        assert_eq!((back.log.arcs_to_chords, back.log.tilted_moved), (0, 0));

        let refused = format_dxf(&[("two\nlines", DxfEntity::String(&toe))]);
        assert!(matches!(refused, Err(IoError::Unsupported(_))));
    }

    #[test]
    fn a_newer_file_with_handles_subclasses_and_extended_data_reads() {
        // A 3D polyline as AutoCAD 2000 and later write it (and Deswik exports): handles (5),
        // owners (330), subclass markers (100), reactors (102) and extended data whose points use
        // codes 1010/1020/1030, none of which may be mistaken for a vertex.
        let text = entities(
            "0\nPOLYLINE\n5\n2F\n330\n1F\n100\nAcDbEntity\n8\nTOE\n100\nAcDb3dPolyline\n66\n1\n10\n0.0\n20\n0.0\n30\n0.0\n70\n9\n\
             0\nVERTEX\n5\n30\n330\n2F\n100\nAcDbEntity\n8\nTOE\n100\nAcDbVertex\n100\nAcDb3dPolylineVertex\n10\n650000.5\n20\n7450000.25\n30\n80.0\n70\n32\n\
             1001\nDESWIK\n1000\nBench 3\n1010\n1.0\n1020\n2.0\n1030\n3.0\n\
             0\nVERTEX\n5\n31\n102\n{ACAD_REACTORS\n330\n2F\n102\n}\n8\nTOE\n10\n650100.5\n20\n7450000.25\n30\n81.0\n70\n32\n\
             0\nVERTEX\n5\n32\n8\nTOE\n10\n650100.5\n20\n7450100.25\n30\n82.0\n70\n32\n\
             0\nSEQEND\n5\n33\n330\n2F\n100\nAcDbEntity\n8\nTOE\n",
        );
        let back = parse_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.strings.len(), 1);
        let s = &back.strings[0];
        assert_eq!(s.layer, "TOE");
        assert!(s.line.closed);
        assert_eq!(s.line.vertices, [[650_000.5, 7_450_000.25, 80.0], [650_100.5, 7_450_000.25, 81.0], [650_100.5, 7_450_100.25, 82.0]]);
    }

    #[test]
    fn a_two_vertex_bulged_lwpolyline_is_a_circle_of_chords_at_its_elevation() {
        // A circle of radius 50 drawn as two semicircles (bulge 1), centred at (650050, 7450000).
        let text = entities(
            "0\nLWPOLYLINE\n8\nRING\n90\n2\n70\n1\n38\n55.5\n10\n650000.0\n20\n7450000.0\n42\n1.0\n10\n650100.0\n20\n7450000.0\n42\n1.0\n",
        );
        let back = parse_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.log.arcs_to_chords, 2);
        let ring = &back.strings[0].line;
        assert!(ring.closed && ring.vertices.len() > 100, "{} vertices", ring.vertices.len());
        assert_eq!(ring.vertices[0], [650_000.0, 7_450_000.0, 55.5], "the original vertices stay exactly");
        for p in &ring.vertices {
            assert!(((p[0] - 650_050.0).hypot(p[1] - 7_450_000.0) - 50.0).abs() < 1e-9);
            assert_eq!(p[2], 55.5);
        }
        // Every chord is within 0.01 m of the arc, so the polygon's area is within 0.01 m times the
        // circumference below the circle's.
        let area = ring.plan_area().unwrap();
        let circle = PI * 2500.0;
        assert!(area < circle && area > circle - ARC_SAGITTA * 2.0 * PI * 50.0, "{area}");
        // The first semicircle runs counter-clockwise from the west point, so through the south.
        let half = ring.vertices.len() / 4;
        assert!(ring.vertices[half][1] < 7_450_000.0);
    }

    #[test]
    fn a_tilted_plane_is_moved_to_world_coordinates() {
        // Extrusion -Z mirrors x and z: (10, 5) at elevation 7 is world (-10, 5, -7). A LINE is in
        // world coordinates already.
        let text = entities(
            "0\nLWPOLYLINE\n8\nM\n90\n2\n38\n7.0\n10\n10.0\n20\n5.0\n10\n20.0\n20\n5.0\n210\n0.0\n220\n0.0\n230\n-1.0\n\
             0\nLINE\n8\nM\n10\n1.0\n20\n2.0\n30\n3.0\n11\n4.0\n21\n5.0\n31\n6.0\n210\n0.0\n220\n0.0\n230\n-1.0\n",
        );
        let back = parse_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.log.tilted_moved, 1);
        assert_eq!(back.strings[0].line.vertices, [[-10.0, 5.0, -7.0], [-20.0, 5.0, -7.0]]);
        assert_eq!(back.strings[1].line.vertices, [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);

        // Tipped up to face east, the plane's x runs north and y runs up.
        let east = ocs_to_world([1.0, 0.0, 0.0], [10.0, 5.0, 7.0]);
        assert!((east[0] - 7.0).abs() < 1e-12 && (east[1] - 10.0).abs() < 1e-12 && (east[2] - 5.0).abs() < 1e-12, "{east:?}");
    }

    #[test]
    fn a_polyface_cube_reads_as_a_closed_mesh() {
        // A 10 m cube: eight positions (flag 192), then six quads wound counter-clockwise from
        // outside (flag 128), one with hidden edges (negative numbers).
        let mut body = String::from("0\nPOLYLINE\n8\nSOLID\n66\n1\n70\n64\n71\n8\n72\n6\n");
        for i in 0..8 {
            let p = [(i & 1) as f64 * 10.0, (i >> 1 & 1) as f64 * 10.0, (i >> 2) as f64 * 10.0];
            body += &format!("0\nVERTEX\n8\nSOLID\n10\n{}\n20\n{}\n30\n{}\n70\n192\n", p[0], p[1], p[2]);
        }
        for [a, b, c, d] in [[1, 3, 4, 2], [5, 6, 8, 7], [1, 2, 6, 5], [3, 7, 8, 4], [1, 5, 7, 3], [2, 4, 8, -6]] {
            body += &format!("0\nVERTEX\n8\nSOLID\n10\n0\n20\n0\n30\n0\n70\n128\n71\n{a}\n72\n{b}\n73\n{c}\n74\n{d}\n");
        }
        body += "0\nSEQEND\n8\nSOLID\n";
        let back = parse_dxf(entities(&body).as_bytes()).unwrap();
        assert_eq!((back.log.polyface_meshes, back.log.faces_read, back.log.quads_split), (1, 6, 6));
        assert_eq!((back.mesh.vertices.len(), back.mesh.triangles.len()), (8, 12));
        assert!(back.mesh.validity().is_watertight());
        assert!((back.mesh.signed_volume() - 1000.0).abs() < 1e-9);
        assert!(back.strings.is_empty());

        let past = entities(&body.replacen("71\n1\n72\n3", "71\n9\n72\n3", 1));
        assert!(matches!(parse_dxf(past.as_bytes()), Err(IoError::Format { .. })));
    }

    #[test]
    fn two_d_polylines_short_strings_and_other_entities_are_counted() {
        let text = entities(
            // A 2D polyline at elevation 12 (its own 30), with a spline frame point to pass over.
            "0\nPOLYLINE\n8\nP\n66\n1\n10\n0.0\n20\n0.0\n30\n12.0\n70\n0\n\
             0\nVERTEX\n8\nP\n10\n1.0\n20\n1.0\n30\n0.0\n\
             0\nVERTEX\n8\nP\n10\n5.0\n20\n5.0\n70\n16\n\
             0\nVERTEX\n8\nP\n10\n2.0\n20\n1.0\n30\n99.0\n\
             0\nSEQEND\n\
             0\nLWPOLYLINE\n8\nP\n90\n1\n10\n3.0\n20\n3.0\n\
             0\nLWPOLYLINE\n8\nP\n90\n2\n70\n1\n10\n3.0\n20\n3.0\n10\n4.0\n20\n4.0\n\
             0\nTEXT\n8\nP\n1\nRamp 1\n0\nINSERT\n8\nP\n2\nTREE\n0\nINSERT\n8\nP\n2\nTREE\n0\nPOINT\n8\nP\n0\nVERTEX\n8\nP\n",
        );
        let back = parse_dxf(text.as_bytes()).unwrap();
        assert_eq!(back.strings[0].line.vertices, [[1.0, 1.0, 12.0], [2.0, 1.0, 12.0]]);
        assert_eq!((back.log.short_strings, back.log.closed_too_short, back.log.strings_read), (1, 1, 2));
        assert!(!back.strings[1].line.closed);
        let skipped: Vec<(&str, usize)> = back.log.skipped.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        assert_eq!(skipped, [("INSERT", 2), ("POINT", 1), ("TEXT", 1), ("VERTEX", 1)]);
    }
}
