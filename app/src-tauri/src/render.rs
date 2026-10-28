//! Render buffers for the viewport: the only place engine f64 coordinates become f32.
//!
//! Layout (little-endian): `f32` positions (x, y, z per vertex, offset from the scene origin),
//! `f32` unit vertex normals, `u32` triangle indices, then a UTF-8 JSON header and finally the
//! header's length as a `u32`. The header goes last so the encoder can time itself without
//! copying the arrays, and the arrays start 4-byte aligned at offset 0.

use std::time::Instant;

use zd_core::TriMesh;

pub fn encode_render_buffer(mesh: &TriMesh, origin: [f64; 3], mut header: serde_json::Value) -> Vec<u8> {
    let start = Instant::now();
    let local = |v: [f64; 3]| [v[0] - origin[0], v[1] - origin[1], v[2] - origin[2]];

    let data_len = mesh.vertices.len() * 24 + mesh.triangles.len() * 12;
    let mut out = Vec::with_capacity(data_len + 1024);
    for &v in &mesh.vertices {
        for c in local(v) {
            out.extend_from_slice(&(c as f32).to_le_bytes());
        }
    }
    for n in vertex_normals(mesh) {
        for c in n {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    for tri in &mesh.triangles {
        for i in tri {
            out.extend_from_slice(&i.to_le_bytes());
        }
    }

    header["vertices"] = mesh.vertices.len().into();
    header["triangles"] = mesh.triangles.len().into();
    header["origin"] = serde_json::json!(origin);
    if let Some(b) = mesh.bounds() {
        header["bounds_local"] = serde_json::json!({ "min": local(b.min), "max": local(b.max) });
    }
    if let Some(timings) = header.get_mut("timings").and_then(|t| t.as_object_mut()) {
        timings.insert("encode_ms".into(), (start.elapsed().as_secs_f64() * 1e3).into());
    }
    let header = serde_json::to_vec(&header).expect("header serialises");
    out.extend_from_slice(&header);
    out.extend_from_slice(&(header.len() as u32).to_le_bytes());
    out
}

/// Area-weighted vertex normals, turned upward triangle by triangle so a surface lights from
/// above whatever its winding.
fn vertex_normals(mesh: &TriMesh) -> Vec<[f32; 3]> {
    let mut sums = vec![[0.0f64; 3]; mesh.vertices.len()];
    for &tri in &mesh.triangles {
        let mut n = mesh.triangle_cross(tri);
        if n[2] < 0.0 {
            n = [-n[0], -n[1], -n[2]];
        }
        for i in tri {
            let s = &mut sums[i as usize];
            s[0] += n[0];
            s[1] += n[1];
            s[2] += n[2];
        }
    }
    sums.into_iter()
        .map(|s| {
            let len = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
            if len > 0.0 { [(s[0] / len) as f32, (s[1] / len) as f32, (s[2] / len) as f32] } else { [0.0, 0.0, 1.0] }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_arrays_then_header_then_length() {
        let mesh = TriMesh::new(
            vec![[650_000.0, 7_450_000.0, 100.0], [650_010.0, 7_450_000.0, 100.0], [650_000.0, 7_450_010.0, 100.0]],
            vec![[0, 1, 2]],
        );
        let buf = encode_render_buffer(&mesh, [650_000.0, 7_450_000.0, 100.0], serde_json::json!({ "timings": {} }));
        let n = buf.len();
        let header_len = u32::from_le_bytes(buf[n - 4..].try_into().unwrap()) as usize;
        let header: serde_json::Value = serde_json::from_slice(&buf[n - 4 - header_len..n - 4]).unwrap();
        assert_eq!(header["triangles"], 1);
        assert!(header["timings"]["encode_ms"].is_number());
        assert_eq!(n - 4 - header_len, 3 * 12 + 3 * 12 + 12);
        let f = |o: usize| f32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
        assert_eq!((f(12), f(16), f(20)), (10.0, 0.0, 0.0), "second vertex, local");
        assert_eq!(f(36 + 8), 1.0, "first normal points up");
    }
}
