//! Many files written as one: a zip, or a folder (decision 0057). Solids go out by the thousand as they come in
//! (decision 0019): one file a solid.

use std::io::Write;
use std::path::Path;

use crate::IoError;

/// Writes named files into a zip, where `path` ends in `.zip`, or into a folder made at `path` if it isn't there.
/// A zip is written whole, replacing one already there; in a folder each file replaces one of its name and others
/// are left. Returns the bytes on disk: the zip's size, or the files' sizes together.
pub fn write_bundle(path: &Path, files: &[(String, Vec<u8>)]) -> Result<u64, IoError> {
    if is_zip(path) {
        let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(std::fs::File::create(path)?));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(files.iter().any(|f| f.1.len() > u32::MAX as usize / 2));
        for (name, bytes) in files {
            zip.start_file(name.as_str(), options).map_err(|e| IoError::Unsupported(format!("{name} in the zip: {e}")))?;
            zip.write_all(bytes)?;
        }
        let mut out = zip.finish().map_err(|e| IoError::Unsupported(format!("closing the zip: {e}")))?;
        out.flush()?;
        drop(out);
        Ok(std::fs::metadata(path)?.len())
    } else {
        std::fs::create_dir_all(path)?;
        let mut written = 0;
        for (name, bytes) in files {
            std::fs::write(path.join(name), bytes)?;
            written += bytes.len() as u64;
        }
        Ok(written)
    }
}

/// Whether a bundle at `path` is a zip: its name ends in `.zip`, in any case.
pub fn is_zip(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zip_and_a_folder_hold_the_same_files() {
        let dir = std::env::temp_dir().join(format!("zd-bundle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let files = vec![("a.txt".to_string(), b"first".to_vec()), ("b b.txt".to_string(), vec![7u8; 100_000])];
        assert_eq!(write_bundle(&dir.join("out"), &files).unwrap(), 100_005);
        assert_eq!(std::fs::read(dir.join("out").join("b b.txt")).unwrap(), files[1].1);
        let size = write_bundle(&dir.join("out.ZIP"), &files).unwrap();
        assert!(size < 2_000, "deflated: {size} bytes");
        let mut zip = zip::ZipArchive::new(std::fs::File::open(dir.join("out.ZIP")).unwrap()).unwrap();
        assert_eq!(zip.len(), 2);
        let mut read = Vec::new();
        std::io::Read::read_to_end(&mut zip.by_name("a.txt").unwrap(), &mut read).unwrap();
        assert_eq!(read, b"first");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
