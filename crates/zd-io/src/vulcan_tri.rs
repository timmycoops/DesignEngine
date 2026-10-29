//! Vulcan triangulations (`.00t`), in the plain binary layout or the compressed one (`vulZ`,
//! shared with Vulcan grids). Decision 0011.
//!
//! There is no public specification. The layout was worked out from a client's files (surfaces of
//! up to 18.5 million triangles and closed coal solids: kept local, never committed). Big-endian:
//!
//! | Bytes | Content |
//! |---|---|
//! | 0..64 | free text (blank in surfaces; "Created External" and a variant name in solids) |
//! | 64..120 | 14 i32: the vertex count at 72 and the triangle count at 96; the rest zero in every sample |
//! | 120.. | per vertex: f64 x, y, z |
//! | then | per triangle: i32 a, b, c (1-based vertex numbers), then three i32, zero in every sample |
//! | then | optional: display properties (colour, shading, the file it was saved as), not read |
//!
//! Triangles are wound counter-clockwise about their outward normal: the closed samples all have a
//! positive volume.

use std::borrow::Cow;
use std::path::Path;

use zd_core::TriMesh;

use crate::IoError;
use crate::vulcan::{VULZ, unpack_vulz};

/// Bytes before the first vertex.
const HEADER: usize = 120;
/// Where the vertex and triangle counts sit among the header's i32 words (bytes 64..120).
const COUNT_WORDS: [usize; 2] = [2, 8];

#[derive(Debug, Clone)]
pub struct VulcanTriangulation {
    /// Triangles numbered from zero.
    pub mesh: TriMesh,
    /// The free-text header (bytes 0..64), trimmed; often blank.
    pub description: String,
    pub log: VulcanTriLog,
}

/// What the reader found and set aside.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VulcanTriLog {
    pub vertices: usize,
    pub triangles: usize,
    /// Triangles whose three trailing words are not zero (their meaning is unknown; ignored).
    pub triangle_extras_set: usize,
    /// Bytes after the last triangle: display properties, not read.
    pub property_bytes: usize,
    /// Blocks unpacked from the compressed layout; zero for a plain file.
    pub compressed_blocks: usize,
}

pub fn read_vulcan_triangulation(path: &Path) -> Result<VulcanTriangulation, IoError> {
    let bytes = std::fs::read(path)?;
    parse_vulcan_triangulation(&bytes).map_err(|e| match e {
        IoError::Binary(message) => IoError::Binary(format!("{}: {message}", path.display())),
        other => other,
    })
}

/// Reads either layout. Anything that does not check out is refused: counts that disagree with the
/// file's size, header words not seen before, a vertex number out of range or a coordinate that is
/// not finite.
pub fn parse_vulcan_triangulation(bytes: &[u8]) -> Result<VulcanTriangulation, IoError> {
    let (plain, blocks): (Cow<'_, [u8]>, usize) = if bytes.len() >= 8 && &bytes[4..8] == VULZ {
        let (plain, blocks) = unpack_vulz(bytes, "triangulation")?;
        (Cow::Owned(plain), blocks)
    } else {
        (Cow::Borrowed(bytes), 0)
    };
    let mut read = parse_plain(&plain)?;
    read.log.compressed_blocks = blocks;
    Ok(read)
}

fn parse_plain(bytes: &[u8]) -> Result<VulcanTriangulation, IoError> {
    let bad = |message: String| IoError::Binary(format!("triangulation: {message}"));
    if bytes.len() < HEADER {
        return Err(bad(format!("{} bytes is too short for the {HEADER}-byte header", bytes.len())));
    }
    let words: Vec<i32> = bytes[64..HEADER].as_chunks::<4>().0.iter().map(|&w| i32::from_be_bytes(w)).collect();
    if let Some((k, w)) = words.iter().enumerate().find(|(k, w)| !COUNT_WORDS.contains(k) && **w != 0) {
        return Err(IoError::Unsupported(format!("a triangulation header not seen before: word {k} (byte {}) is {w}", 64 + 4 * k)));
    }
    let (vertices, triangles) = (words[COUNT_WORDS[0]], words[COUNT_WORDS[1]]);
    if vertices < 3 || triangles < 1 {
        return Err(bad(format!("{vertices} vertices and {triangles} triangles is not a triangulation")));
    }
    let (vertices, triangles) = (vertices as usize, triangles as usize);
    let tri_at = HEADER + 24 * vertices;
    let end = tri_at + 24 * triangles;
    if bytes.len() < end {
        return Err(bad(format!("{vertices} vertices and {triangles} triangles need {end} bytes; the file holds {}", bytes.len())));
    }

    let mut points = Vec::with_capacity(vertices);
    for (k, v) in bytes[HEADER..tri_at].as_chunks::<24>().0.iter().enumerate() {
        let [x, y, z] = v.as_chunks::<8>().0 else { unreachable!("24 bytes are three 8-byte words") };
        let p = [f64::from_be_bytes(*x), f64::from_be_bytes(*y), f64::from_be_bytes(*z)];
        if !p.iter().all(|c| c.is_finite()) {
            return Err(bad(format!("vertex {} is not finite: {p:?}", k + 1)));
        }
        points.push(p);
    }
    let mut faces = Vec::with_capacity(triangles);
    let mut extras = 0;
    for (k, t) in bytes[tri_at..end].as_chunks::<24>().0.iter().enumerate() {
        let words = t.as_chunks::<4>().0;
        let corners = [i32::from_be_bytes(words[0]), i32::from_be_bytes(words[1]), i32::from_be_bytes(words[2])];
        if let Some(c) = corners.iter().find(|&&c| c < 1 || c as usize > vertices) {
            return Err(bad(format!("triangle {} refers to vertex {c} of {vertices}", k + 1)));
        }
        extras += usize::from(t[12..24].iter().any(|&b| b != 0));
        faces.push(corners.map(|c| (c - 1) as u32));
    }

    let description = String::from_utf8_lossy(&bytes[..64]).trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_string();
    let log = VulcanTriLog { vertices, triangles, triangle_extras_set: extras, property_bytes: bytes.len() - end, compressed_blocks: 0 };
    Ok(VulcanTriangulation { mesh: TriMesh::new(points, faces), description, log })
}

/// A triangulation in the plain layout, for tests here and elsewhere (no properties block).
#[doc(hidden)]
pub fn plain_triangulation_bytes(description: &str, mesh: &TriMesh) -> Vec<u8> {
    let mut out = vec![0u8; HEADER];
    let text = description.as_bytes();
    out[..text.len().min(64)].copy_from_slice(&text[..text.len().min(64)]);
    out[72..76].copy_from_slice(&(mesh.vertices.len() as i32).to_be_bytes());
    out[96..100].copy_from_slice(&(mesh.triangles.len() as i32).to_be_bytes());
    for p in &mesh.vertices {
        p.iter().for_each(|c| out.extend_from_slice(&c.to_be_bytes()));
    }
    for t in &mesh.triangles {
        t.iter().for_each(|&i| out.extend_from_slice(&(i as i32 + 1).to_be_bytes()));
        out.extend_from_slice(&[0u8; 12]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulcan::tests::vulz_grid;

    const E: f64 = 650_000.123_456_789;
    const N: f64 = 7_450_000.987_654_321;

    /// A 10 x 20 x 5 m box at mine grid coordinates, wound outward.
    fn block() -> TriMesh {
        let v = (0..8).map(|i| [E + (i & 1) as f64 * 10.0, N + (i >> 1 & 1) as f64 * 20.0, 250.0 + (i >> 2) as f64 * 5.0]).collect();
        let t = vec![[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]];
        TriMesh::new(v, t)
    }

    #[test]
    fn the_plain_layout_reads_every_coordinate_exactly() {
        let mut bytes = plain_triangulation_bytes("Created External", &block());
        bytes.extend_from_slice(&[1, 0, 0, 0, 2, 3]); // a properties block, not read
        let read = parse_vulcan_triangulation(&bytes).unwrap();
        assert_eq!(read.mesh, block());
        assert_eq!(read.description, "Created External");
        assert_eq!(read.log, VulcanTriLog { vertices: 8, triangles: 12, triangle_extras_set: 0, property_bytes: 6, compressed_blocks: 0 });
        assert!(read.mesh.validity().is_watertight());
        assert!((read.mesh.signed_volume() - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn the_compressed_layout_unpacks_to_the_same_mesh() {
        let plain = plain_triangulation_bytes("", &block());
        let packed = vulz_grid(&plain, 256, 4, 0);
        let read = parse_vulcan_triangulation(&packed).unwrap();
        assert_eq!(read.mesh, block());
        assert!(read.log.compressed_blocks > 1);
    }

    #[test]
    fn files_that_do_not_check_out_are_refused() {
        let good = plain_triangulation_bytes("", &block());
        assert!(matches!(parse_vulcan_triangulation(&good[..good.len() - 1]), Err(IoError::Binary(_))), "truncated");
        let mut odd = good.clone();
        odd[64..68].copy_from_slice(&7i32.to_be_bytes());
        assert!(matches!(parse_vulcan_triangulation(&odd), Err(IoError::Unsupported(_))), "a header word not seen before");
        let mut past = good.clone();
        let first_tri = HEADER + 24 * 8;
        past[first_tri..first_tri + 4].copy_from_slice(&9i32.to_be_bytes());
        let error = parse_vulcan_triangulation(&past).unwrap_err().to_string();
        assert!(error.contains("triangle 1 refers to vertex 9 of 8"), "{error}");
        let mut nan = good.clone();
        nan[HEADER..HEADER + 8].copy_from_slice(&f64::NAN.to_be_bytes());
        assert!(parse_vulcan_triangulation(&nan).is_err());
        let mut extras = good;
        extras[first_tri + 12] = 1;
        assert_eq!(parse_vulcan_triangulation(&extras).unwrap().log.triangle_extras_set, 1);
    }
}
