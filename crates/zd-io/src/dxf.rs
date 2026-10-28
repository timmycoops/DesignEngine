//! DXF `3DFACE` triangulations (ASCII DXF only).
//!
//! A 3DFACE stores its own corner coordinates, so shared corners are welded by exact f64
//! equality on the way in. That is the format's representation, not a repair; tolerance welding
//! belongs to the import repair step.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use zd_core::{TriMesh, is_zero_area};

use crate::IoError;

#[derive(Debug, Clone)]
pub struct DxfImport {
    pub mesh: TriMesh,
    pub log: DxfLog,
}

/// What the reader did to get from 3DFACE entities to a triangle mesh.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DxfLog {
    pub faces_read: usize,
    /// Four-cornered faces split into two triangles.
    pub quads_split: usize,
    /// Triangles dropped for a repeated corner or zero area.
    pub degenerate_dropped: usize,
    /// Triangles dropped because another triangle already used the same three vertices.
    pub duplicates_dropped: usize,
}

pub fn read_dxf_3dfaces(path: &Path) -> Result<DxfImport, IoError> {
    let bytes = std::fs::read(path)?;
    parse_dxf_3dfaces(&bytes)
}

/// Parses every `3DFACE` in the ENTITIES section. Faces inside BLOCKS are definitions, not
/// geometry, and are skipped.
pub fn parse_dxf_3dfaces(bytes: &[u8]) -> Result<DxfImport, IoError> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err(IoError::Unsupported("binary DXF; save as ASCII DXF".into()));
    }

    let mut builder = FaceBuilder::default();
    let mut lines = bytes.split(|&b| b == b'\n').map(trim_line).enumerate();
    let mut section: Option<Vec<u8>> = None;
    let mut expecting_section_name = false;
    let mut face: Option<[[f64; 3]; 4]> = None;
    let mut corners_seen = [false; 4];

    while let Some((code_line, code)) = lines.next() {
        // Group codes are never blank, so a blank code line is only allowed as trailing padding.
        if code.is_empty() {
            if lines.all(|(_, rest)| rest.is_empty()) {
                break;
            }
            return Err(IoError::Format { line: code_line + 1, message: "blank group code".into() });
        }
        let code: i32 = parse_ascii(code).ok_or_else(|| IoError::Format {
            line: code_line + 1,
            message: format!("expected a group code, found '{}'", String::from_utf8_lossy(code)),
        })?;
        let Some((value_line, value)) = lines.next() else {
            return Err(IoError::Format { line: code_line + 1, message: "group code without a value".into() });
        };

        if code == 0 {
            if let Some(corners) = face.take() {
                if !(corners_seen[0] && corners_seen[1] && corners_seen[2]) {
                    builder.log.faces_read += 1;
                    builder.log.degenerate_dropped += 1;
                } else if !corners_seen[3] {
                    builder.push_face(corners[0], corners[1], corners[2], corners[2]);
                } else {
                    builder.push_face(corners[0], corners[1], corners[2], corners[3]);
                }
            }
            match value {
                b"SECTION" => expecting_section_name = true,
                b"ENDSEC" => section = None,
                b"EOF" => break,
                b"3DFACE" if section.as_deref() == Some(b"ENTITIES".as_slice()) => {
                    face = Some([[0.0; 3]; 4]);
                    corners_seen = [false; 4];
                }
                _ => {}
            }
            continue;
        }

        if code == 2 && expecting_section_name {
            section = Some(value.to_vec());
            expecting_section_name = false;
            continue;
        }

        if let Some(corners) = face.as_mut() {
            // 10/20/30 = corner 0 x/y/z, 11/21/31 = corner 1, ... 13/23/33 = corner 3.
            if (10..=33).contains(&code) && (code % 10) <= 3 {
                let corner = (code % 10) as usize;
                let axis = (code / 10 - 1) as usize;
                let v: f64 = parse_ascii(value).ok_or_else(|| IoError::Format {
                    line: value_line + 1,
                    message: format!("bad coordinate '{}'", String::from_utf8_lossy(value)),
                })?;
                corners[corner][axis] = v;
                corners_seen[corner] = true;
            }
        }
    }

    Ok(builder.finish())
}

/// Writes a mesh as DXF R12 `3DFACE` entities (triangles repeat their third corner).
/// Coordinates are written with Rust's shortest round-trip formatting, so f64 values survive.
pub fn write_dxf_3dfaces(path: &Path, mesh: &TriMesh, layer: &str) -> Result<(), IoError> {
    let mut out = String::with_capacity(mesh.triangles.len() * 200 + 64);
    out.push_str("0\nSECTION\n2\nENTITIES\n");
    for tri in &mesh.triangles {
        let [a, b, c] = tri.map(|i| mesh.vertices[i as usize]);
        let _ = write!(out, "0\n3DFACE\n8\n{layer}\n");
        for (corner, p) in [a, b, c, c].iter().enumerate() {
            let _ = write!(out, "1{corner}\n{}\n2{corner}\n{}\n3{corner}\n{}\n", p[0], p[1], p[2]);
        }
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    std::fs::write(path, out)?;
    Ok(())
}

#[derive(Default)]
struct FaceBuilder {
    vertices: Vec<[f64; 3]>,
    lookup: HashMap<[u64; 3], u32>,
    triangles: Vec<[u32; 3]>,
    seen: HashSet<[u32; 3]>,
    log: DxfLog,
}

impl FaceBuilder {
    fn push_face(&mut self, a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) {
        self.log.faces_read += 1;
        let ia = self.vertex(a);
        let ib = self.vertex(b);
        let ic = self.vertex(c);
        let id = self.vertex(d);
        if id == ic {
            self.push_triangle([ia, ib, ic]);
        } else {
            self.log.quads_split += 1;
            self.push_triangle([ia, ib, ic]);
            self.push_triangle([ia, ic, id]);
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

    fn push_triangle(&mut self, tri: [u32; 3]) {
        let [a, b, c] = tri;
        if a == b || b == c || a == c || is_zero_area(tri.map(|i| self.vertices[i as usize])) {
            self.log.degenerate_dropped += 1;
            return;
        }
        let mut sorted = tri;
        sorted.sort_unstable();
        if !self.seen.insert(sorted) {
            self.log.duplicates_dropped += 1;
            return;
        }
        self.triangles.push(tri);
    }

    fn finish(self) -> DxfImport {
        DxfImport { mesh: TriMesh::new(self.vertices, self.triangles), log: self.log }
    }
}

fn trim_line(line: &[u8]) -> &[u8] {
    line.trim_ascii()
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

    fn dxf(faces: &[[[f64; 3]; 4]]) -> String {
        let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
        for f in faces {
            s.push_str(&face(*f));
        }
        s.push_str("0\nENDSEC\n0\nEOF\n");
        s
    }

    const A: [f64; 3] = [650_000.125, 7_450_000.5, 101.25];
    const B: [f64; 3] = [650_010.125, 7_450_000.5, 102.0];
    const C: [f64; 3] = [650_010.125, 7_450_010.5, 103.0];
    const D: [f64; 3] = [650_000.125, 7_450_010.5, 104.0];

    #[test]
    fn triangles_and_quads_weld_shared_corners() {
        let text = dxf(&[[A, B, C, C], [A, C, D, D]]);
        let import = parse_dxf_3dfaces(text.as_bytes()).unwrap();
        assert_eq!(import.mesh.vertices.len(), 4);
        assert_eq!(import.mesh.triangles.len(), 2);
        assert_eq!(import.mesh.vertices[0], A, "coordinates must survive exactly as f64");
        assert_eq!(import.log.faces_read, 2);
        assert_eq!(import.log.quads_split, 0);

        let quad = parse_dxf_3dfaces(dxf(&[[A, B, C, D]]).as_bytes()).unwrap();
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
        let import = parse_dxf_3dfaces(text.as_bytes()).unwrap();
        assert_eq!(import.mesh.triangles.len(), 1);
        assert_eq!(import.log.duplicates_dropped, 1);
        assert_eq!(import.log.degenerate_dropped, 2);
    }

    #[test]
    fn faces_in_blocks_are_ignored_and_crlf_is_accepted() {
        let block = format!("0\nSECTION\n2\nBLOCKS\n{}0\nENDSEC\n", face([A, B, C, C]));
        let text = format!("{block}{}", dxf(&[[A, C, D, D]])).replace('\n', "\r\n");
        let import = parse_dxf_3dfaces(text.as_bytes()).unwrap();
        assert_eq!(import.mesh.triangles.len(), 1);
    }

    #[test]
    fn write_then_read_round_trips_exactly() {
        let mesh = TriMesh::new(vec![A, B, C, D], vec![[0, 1, 2], [0, 2, 3]]);
        let dir = std::env::temp_dir().join(format!("zd-io-dxf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.dxf");
        write_dxf_3dfaces(&path, &mesh, "TEST").unwrap();
        let back = read_dxf_3dfaces(&path).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(back.mesh, mesh);
    }

    #[test]
    fn bad_coordinate_reports_line() {
        let text = dxf(&[[A, B, C, C]]).replacen("650000.125", "65O000.125", 1);
        match parse_dxf_3dfaces(text.as_bytes()) {
            Err(IoError::Format { line, .. }) => assert_eq!(line, 10),
            other => panic!("expected a format error, got {other:?}"),
        }
    }
}
