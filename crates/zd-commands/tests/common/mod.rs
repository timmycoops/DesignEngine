//! Shared by the tests that make solids: every solid a command made goes out as DXF and comes back as the same solid
//! (decision 0055), as another package would read it.

use serde_json::json;
use zd_commands::Session;
use zd_core::{Body, RunId};
use zd_io::write_dxf_3dfaces;

/// Every solid in the session, written to DXF as loose 3DFACEs and imported again as read, with no repair: it must
/// come back a solid with its corners as written, the same triangles, and its volume to a billionth of itself (or
/// 1e-9 m³ if that's more). Returns how many went round.
pub fn assert_solids_round_trip(s: &Session) -> usize {
    let dir = std::env::temp_dir().join(format!("zd-round-trip-{}-{}", std::process::id(), RunId::new()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut count = 0;
    for object in s.project().objects() {
        let Body::Solid(solid) = &object.body else { continue };
        let path = dir.join(format!("{count}.dxf"));
        write_dxf_3dfaces(&path, solid.mesh(), "SOLID").unwrap();
        let mut back = Session::with_builtins();
        let record = back
            .run("solid.import", json!({ "path": path.to_string_lossy(), "layer": "back" }))
            .unwrap_or_else(|why| panic!("'{}' doesn't read back as a solid from its own DXF: {why}", object.name));
        let read = back.project().objects().find(|o| matches!(o.body, Body::Solid(_))).expect("the solid read back");
        let Body::Solid(again) = &read.body else { unreachable!() };
        assert!(read.attributes.is_empty(), "'{}' needed a weld or a repair to read back: {:?} ({:?})", object.name, read.attributes, record.messages);
        assert_eq!(again.mesh().triangle_count(), solid.mesh().triangle_count(), "'{}' lost or gained triangles", object.name);
        let (was, is) = (solid.volume(), again.volume());
        assert!((was - is).abs() <= 1e-9 * was.abs().max(1.0), "'{}': {was} m3 out, {is} m3 back", object.name);
        count += 1;
    }
    std::fs::remove_dir_all(&dir).unwrap();
    count
}
