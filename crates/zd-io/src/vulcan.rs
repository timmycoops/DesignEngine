//! Vulcan grid files, `<proj><seam>.<variable>g` (e.g. `pitupper.srg` roof, `.sfg` floor, `.rag` raw
//! ash), in the plain binary layout or the compressed one (`vulZ`).
//!
//! There is no public specification. The layouts below were worked out from sample files (decision
//! 0006). A plain grid read this way matches Vulcan's own text export of it at every node; a compressed
//! grid unpacks to the plain layout. The plain layout is big-endian throughout:
//!
//! | Bytes   | Content                                                             |
//! |---------|---------------------------------------------------------------------|
//! | 0..32   | free text: name and timestamp, or how a derived grid was made       |
//! | 32..64  | f64 x0, x1, y0, y1: extents of the node lattice                     |
//! | 64..68  | i32 node count (cols × rows)                                        |
//! | 68..72  | i32, zero in every sample                                           |
//! | 72..80  | f32 dx, dy                                                          |
//! | 80..88  | i32 cols, rows                                                      |
//! | 88..    | per node: f32 value, i32 flag (1 = live; anything else is null)     |
//!
//! Nodes run row by row, x fastest, from the south-west corner. Files may end in zero padding.
//! Values are single precision and widen to f64 exactly.
//!
//! The compressed layout holds the plain layout's bytes cut into fixed-size blocks, each compressed
//! with LZF (liblzf's format). Little-endian:
//!
//! | Bytes   | Content                                                              |
//! |---------|----------------------------------------------------------------------|
//! | 0..8    | magic `ea fb a7 8a` then `vulZ`                                      |
//! | 8..12   | i32, 2 in every sample                                               |
//! | 12..16  | i32 index depth: levels of index pages, the root included (4 seen)   |
//! | 16..20  | i32, 1 in every sample                                               |
//! | 20..24  | i32 block size: plain bytes per block (25,600 and 40,960 seen)       |
//! | 24..28  | i32 offset of the root index page (60 seen)                          |
//! | 32..40  | u64 size of the plain layout                                         |
//!
//! An index page is 256 u64 file offsets (zero = unused). Entries of the root and inner pages point
//! to the next level's pages; entries of the deepest pages point to the blocks, in order. A block is
//! a u32 compressed length, a u32 slot length, then the compressed bytes. The last block may unpack
//! past the end of the grid into zeros.

use std::collections::BTreeMap;
use std::path::Path;

use zd_core::Grid;

use crate::IoError;

const HEADER: usize = 88;
pub(crate) const VULZ: &[u8; 4] = b"vulZ";
const VULZ_HEADER: usize = 40;
const VULZ_PAGE_ENTRIES: usize = 256;

#[derive(Debug, Clone)]
pub struct VulcanGrid {
    /// Null nodes are NaN.
    pub grid: Grid,
    /// The free-text header (bytes 0..32), trimmed.
    pub description: String,
    pub log: VulcanLog,
}

/// What the reader found and set aside, so nothing is dropped silently.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VulcanLog {
    pub nodes: usize,
    pub live: usize,
    pub nulls: usize,
    /// Null nodes that still hold a non-zero value, which was ignored.
    pub stale_nulls: usize,
    /// Flags other than 0 and 1, read as null.
    pub odd_flags: usize,
    /// Live nodes holding NaN or infinity, read as null.
    pub non_finite: usize,
    /// Zero bytes after the last node.
    pub padding_bytes: usize,
    /// Blocks unpacked from the compressed layout; zero for a plain file.
    pub compressed_blocks: usize,
}

pub fn read_vulcan_grid(path: &Path) -> Result<VulcanGrid, IoError> {
    let bytes = std::fs::read(path)?;
    parse_vulcan_grid(&bytes).map_err(|e| match e {
        IoError::Binary(message) => IoError::Binary(format!("{}: {message}", path.display())),
        IoError::Unsupported(message) => IoError::Unsupported(format!("{}: {message}", path.display())),
        other => other,
    })
}

/// Reads either layout.
pub fn parse_vulcan_grid(bytes: &[u8]) -> Result<VulcanGrid, IoError> {
    if bytes.len() >= 8 && &bytes[4..8] == VULZ {
        let (plain, blocks) = unpack_vulz(bytes, "grid")?;
        let mut read = parse_plain_grid(&plain)?;
        read.log.compressed_blocks = blocks;
        return Ok(read);
    }
    parse_plain_grid(bytes)
}

/// Unpacks the compressed layout into the plain one; returns it with the number of blocks. `what`
/// names the file's kind in errors (grid, triangulation).
pub(crate) fn unpack_vulz(bytes: &[u8], what: &str) -> Result<(Vec<u8>, usize), IoError> {
    let bad = |message: String| IoError::Binary(format!("compressed {what}: {message}"));
    if bytes.len() < VULZ_HEADER {
        return Err(bad(format!("{} bytes is too short for the header", bytes.len())));
    }
    let u32_at = |o: usize| u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()) as usize;
    let u64_at = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
    let (depth, block, root, total) = (u32_at(12), u32_at(20), u32_at(24), u64_at(32));
    // Every block takes at least its 8-byte head in the file, which bounds the work on a bad header.
    let blocks = if block == 0 { 0 } else { total.div_ceil(block as u64) };
    if !(1..=8).contains(&depth) || !(1..=1 << 24).contains(&block) || total < HEADER as u64 || blocks > (bytes.len() / 8) as u64 {
        return Err(bad(format!("header gives index depth {depth}, {block}-byte blocks and {total} bytes, which do not fit a {}-byte file", bytes.len())));
    }
    let (blocks, total) = (blocks as usize, total as usize);

    let mut entries = vec![root as u64];
    for _ in 0..depth {
        let mut next = Vec::new();
        for &page in &entries {
            let page = usize::try_from(page).unwrap_or(usize::MAX);
            let Some(page_bytes) = page.checked_add(8 * VULZ_PAGE_ENTRIES).and_then(|end| bytes.get(page..end)) else {
                return Err(bad(format!("index page at byte {page} runs past the end of the file")));
            };
            next.extend(page_bytes.as_chunks::<8>().0.iter().map(|&e| u64::from_le_bytes(e)).filter(|&e| e != 0));
            if next.len() >= blocks {
                break;
            }
        }
        next.truncate(blocks);
        entries = next;
    }
    if entries.len() < blocks {
        return Err(bad(format!("the index lists {} blocks, the header needs {blocks}", entries.len())));
    }

    let mut plain = Vec::with_capacity(total.min(bytes.len().saturating_mul(16)));
    for (k, &offset) in entries.iter().enumerate() {
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        let packed = offset.checked_add(8).and_then(|start| {
            let len = u32::from_le_bytes(bytes.get(offset..start)?[..4].try_into().unwrap()) as usize;
            bytes.get(start..start.checked_add(len)?)
        });
        let Some(packed) = packed else {
            return Err(bad(format!("block {k} at byte {offset} runs past the end of the file")));
        };
        let unpacked = lzf_decompress(packed, block).map_err(|m| bad(format!("block {k}: {m}")))?;
        let want = block.min(total - k * block);
        if unpacked.len() < want {
            return Err(bad(format!("block {k} unpacks to {} bytes, the grid needs {want}", unpacked.len())));
        }
        if unpacked[want..].iter().any(|&b| b != 0) {
            return Err(bad(format!("block {k} holds data past the end of the grid")));
        }
        plain.extend_from_slice(&unpacked[..want]);
    }
    Ok((plain, blocks))
}

/// Decompresses one LZF stream (liblzf's format) of at most `limit` bytes. A control byte below 32
/// starts a run of that many literal bytes plus one. Otherwise its top three bits are a match length
/// minus two (7 means: add the next byte), and its low five bits with the next byte are the distance
/// back minus one. Matches may overlap the bytes they produce.
fn lzf_decompress(input: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(limit);
    let mut i = 0;
    let next = |i: usize| input.get(i).map(|&b| usize::from(b)).ok_or_else(|| "stream ends inside a match".to_string());
    while i < input.len() {
        let control = usize::from(input[i]);
        i += 1;
        if control < 32 {
            let run = input.get(i..i + control + 1).ok_or("literal run past the end of the stream")?;
            if out.len() + run.len() > limit {
                return Err(format!("unpacks past {limit} bytes"));
            }
            out.extend_from_slice(run);
            i += run.len();
            continue;
        }
        let mut len = control >> 5;
        if len == 7 {
            len += next(i)?;
            i += 1;
        }
        let len = len + 2;
        let distance = ((control & 0x1f) << 8) + next(i)? + 1;
        i += 1;
        let from = out.len().checked_sub(distance).ok_or("match reaches back before the start of the block")?;
        if out.len() + len > limit {
            return Err(format!("unpacks past {limit} bytes"));
        }
        if distance >= len {
            out.extend_from_within(from..from + len);
        } else {
            for k in from..from + len {
                out.push(out[k]);
            }
        }
    }
    Ok(out)
}

fn parse_plain_grid(bytes: &[u8]) -> Result<VulcanGrid, IoError> {
    let bad = |message: String| IoError::Binary(message);
    if bytes.len() < HEADER {
        return Err(bad(format!("{} bytes is too short for a Vulcan grid header", bytes.len())));
    }
    let f64_at = |o: usize| f64::from_be_bytes(bytes[o..o + 8].try_into().unwrap());
    let f32_at = |o: usize| f32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());
    let i32_at = |o: usize| i32::from_be_bytes(bytes[o..o + 4].try_into().unwrap());

    let description = bytes[..32].iter().map(|&b| b as char).collect::<String>().trim_matches(['\0', ' ']).to_string();
    let [x0, x1, y0, y1] = [32, 40, 48, 56].map(f64_at);
    let (count, dx, dy, cols, rows) = (i32_at(64), f32_at(72), f32_at(76), i32_at(80), i32_at(84));
    if cols < 2 || rows < 2 || i64::from(count) != i64::from(cols) * i64::from(rows) {
        return Err(bad(format!("header gives {count} nodes but a {cols} x {rows} lattice")));
    }
    if !(dx.is_finite() && dy.is_finite() && dx > 0.0 && dy > 0.0) || ![x0, x1, y0, y1].iter().all(|v| v.is_finite()) {
        return Err(bad(format!("header cell size {dx} x {dy} or extents are not usable")));
    }
    let (dx, dy) = (f64::from(dx), f64::from(dy));
    let spans = [((x1 - x0) / dx + 1.0, cols), ((y1 - y0) / dy + 1.0, rows)];
    if spans.iter().any(|&(span, n)| (span - f64::from(n)).abs() > 1e-3) {
        return Err(bad(format!(
            "extents {x0}..{x1}, {y0}..{y1} at {dx} x {dy} m do not give {cols} x {rows} nodes"
        )));
    }
    let (cols, rows) = (cols as usize, rows as usize);
    let nodes = cols * rows;
    let end = HEADER + 8 * nodes;
    if bytes.len() < end {
        return Err(bad(format!("truncated: {nodes} nodes need {end} bytes, the file has {}", bytes.len())));
    }
    if bytes[end..].iter().any(|&b| b != 0) {
        return Err(bad(format!("{} bytes after the last node are not zero padding", bytes.len() - end)));
    }

    let mut log = VulcanLog { nodes, padding_bytes: bytes.len() - end, ..VulcanLog::default() };
    let mut z = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let value = f32_at(HEADER + 8 * i);
        let flag = i32_at(HEADER + 8 * i + 4);
        if flag == 1 && value.is_finite() {
            log.live += 1;
            z.push(f64::from(value));
            continue;
        }
        log.nulls += 1;
        match flag {
            1 => log.non_finite += 1,
            0 => {}
            _ => log.odd_flags += 1,
        }
        if flag != 1 && value != 0.0 {
            log.stale_nulls += 1;
        }
        z.push(f64::NAN);
    }
    Ok(VulcanGrid { grid: Grid::new([x0, y0], [dx, dy], cols, rows, z), description, log })
}

/// Splits a Vulcan grid file name into its stem (project prefix plus seam) and variable, lower case:
/// `pitupper.rag` gives `("pitupper", "ra")`. `None` unless the extension is letters or digits ending
/// in `g`.
pub fn vulcan_grid_name(file_name: &str) -> Option<(String, String)> {
    let (stem, ext) = file_name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    let variable = ext.strip_suffix('g')?;
    if stem.is_empty() || variable.is_empty() || variable.len() > 3 || !variable.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((stem.to_ascii_lowercase(), variable.to_string()))
}

/// Every grid of one seam: `<prefix><seam>.<variable>g`, matched case-insensitively on the whole
/// stem (so seam `upper` does not pick up `upperp15`).
#[derive(Debug, Clone)]
pub struct VulcanSeam {
    pub seam: String,
    /// By variable code (`sr`, `sf`, `tk`, `ra`, ...).
    pub grids: BTreeMap<String, VulcanGrid>,
    /// Variables left out, with the reason (e.g. a compressed file).
    pub skipped: Vec<(String, String)>,
}

impl VulcanSeam {
    pub fn roof(&self) -> &VulcanGrid {
        &self.grids["sr"]
    }

    pub fn floor(&self) -> &VulcanGrid {
        &self.grids["sf"]
    }
}

/// Reads a seam's grids from `dir`. The roof (`sr`) and floor (`sf`) must read; another variable that
/// cannot be read is skipped and listed. Every grid must share the roof's lattice.
pub fn read_vulcan_seam(dir: &Path, prefix: &str, seam: &str) -> Result<VulcanSeam, IoError> {
    let stem = format!("{prefix}{seam}").to_ascii_lowercase();
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some((s, variable)) = vulcan_grid_name(&name)
            && s == stem
            && entry.file_type()?.is_file()
        {
            files.push((variable, entry.path()));
        }
    }
    files.sort();

    let mut out = VulcanSeam { seam: seam.to_string(), grids: BTreeMap::new(), skipped: Vec::new() };
    for (variable, path) in files {
        match read_vulcan_grid(&path) {
            Ok(grid) => {
                out.grids.insert(variable, grid);
            }
            Err(e) if variable != "sr" && variable != "sf" => out.skipped.push((variable, e.to_string())),
            Err(e) => return Err(e),
        }
    }
    for needed in ["sr", "sf"] {
        if !out.grids.contains_key(needed) {
            return Err(IoError::Unsupported(format!("seam '{seam}' has no {needed} grid ({stem}.{needed}g) in {}", dir.display())));
        }
    }
    let lattice = out.roof().grid.clone();
    if let Some((variable, _)) = out.grids.iter().find(|(_, g)| !g.grid.same_lattice(&lattice)) {
        return Err(IoError::Binary(format!("{stem}.{variable}g is on a different lattice from {stem}.srg")));
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Writes the plain layout, for tests: `values` row by row from the south-west, `None` = null.
    pub fn plain_grid(description: &str, origin: [f64; 2], cell: f32, cols: usize, rows: usize, values: &[Option<f32>], padding: usize) -> Vec<u8> {
        let mut out = vec![b' '; 32];
        out[..description.len()].copy_from_slice(description.as_bytes());
        let x1 = origin[0] + (cols - 1) as f64 * f64::from(cell);
        let y1 = origin[1] + (rows - 1) as f64 * f64::from(cell);
        for v in [origin[0], x1, origin[1], y1] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        out.extend_from_slice(&((cols * rows) as i32).to_be_bytes());
        out.extend_from_slice(&0i32.to_be_bytes());
        out.extend_from_slice(&cell.to_be_bytes());
        out.extend_from_slice(&cell.to_be_bytes());
        out.extend_from_slice(&(cols as i32).to_be_bytes());
        out.extend_from_slice(&(rows as i32).to_be_bytes());
        for v in values {
            out.extend_from_slice(&v.unwrap_or(0.36).to_be_bytes()); // nulls keep a stale value
            out.extend_from_slice(&i32::from(v.is_some()).to_be_bytes());
        }
        out.extend(std::iter::repeat_n(0u8, padding));
        out
    }

    /// LZF for tests: a match one node (8 bytes) back wherever three or more bytes repeat, literal runs
    /// elsewhere. Long matches overlap the bytes they produce.
    fn lzf_compress(data: &[u8]) -> Vec<u8> {
        fn flush(out: &mut Vec<u8>, literals: &mut Vec<u8>) {
            for run in literals.chunks(32) {
                out.push(run.len() as u8 - 1);
                out.extend_from_slice(run);
            }
            literals.clear();
        }
        let (mut out, mut literals, mut i) = (Vec::new(), Vec::new(), 0);
        while i < data.len() {
            let mut len = 0;
            while i >= 8 && i + len < data.len() && len < 264 && data[i + len] == data[i + len - 8] {
                len += 1;
            }
            if len < 3 {
                literals.push(data[i]);
                i += 1;
                continue;
            }
            flush(&mut out, &mut literals);
            let (stored, distance) = (len - 2, 8 - 1);
            if stored < 7 {
                out.push((stored as u8) << 5);
            } else {
                out.extend_from_slice(&[7 << 5, (stored - 7) as u8]);
            }
            out.push(distance);
            i += len;
        }
        flush(&mut out, &mut literals);
        out
    }

    /// Wraps a plain-layout file in the compressed layout, for tests: `block`-byte blocks, `depth`
    /// levels of index pages (root first, at byte 60), and `tail` zero bytes unpacking past the grid.
    pub fn vulz_grid(plain: &[u8], block: usize, depth: usize, tail: usize) -> Vec<u8> {
        let mut chunks: Vec<Vec<u8>> = plain.chunks(block).map(<[u8]>::to_vec).collect();
        chunks.last_mut().unwrap().extend(std::iter::repeat_n(0u8, tail));
        let packed: Vec<Vec<u8>> = chunks.iter().map(|c| lzf_compress(c)).collect();

        // Pages per level, root first: each holds up to 256 entries of the level below.
        let mut pages = vec![packed.len().div_ceil(VULZ_PAGE_ENTRIES)];
        while pages.len() < depth {
            pages.insert(0, pages[0].div_ceil(VULZ_PAGE_ENTRIES));
        }
        assert_eq!(pages[0], 1, "one root page");
        let mut offset = 60;
        let mut place = |size: usize| {
            offset += size;
            offset - size
        };
        let levels: Vec<Vec<usize>> = pages.iter().map(|&n| (0..n).map(|_| place(8 * VULZ_PAGE_ENTRIES)).collect()).collect();
        let blocks: Vec<usize> = packed.iter().map(|p| place(8 + p.len())).collect();

        let mut out = vec![0u8; 60];
        out[..8].copy_from_slice(&[0xea, 0xfb, 0xa7, 0x8a, b'v', b'u', b'l', b'Z']);
        for (k, v) in [2, depth as i32, 1, block as i32, 60, 0].into_iter().enumerate() {
            out[8 + 4 * k..12 + 4 * k].copy_from_slice(&v.to_le_bytes());
        }
        out[32..40].copy_from_slice(&(plain.len() as u64).to_le_bytes());
        out[40..44].copy_from_slice(&2i32.to_le_bytes());
        for (level, offsets) in levels.iter().enumerate() {
            let children = levels.get(level + 1).unwrap_or(&blocks);
            for j in 0..offsets.len() {
                let mut page = vec![0u8; 8 * VULZ_PAGE_ENTRIES];
                for (k, child) in children.iter().skip(j * VULZ_PAGE_ENTRIES).take(VULZ_PAGE_ENTRIES).enumerate() {
                    page[8 * k..8 * k + 8].copy_from_slice(&(*child as u64).to_le_bytes());
                }
                out.extend_from_slice(&page);
            }
        }
        for p in &packed {
            out.extend_from_slice(&(p.len() as u32).to_le_bytes());
            out.extend_from_slice(&(p.len() as u32).to_le_bytes());
            out.extend_from_slice(p);
        }
        out
    }

    const E: f64 = 500_025.0;
    const N: f64 = 7_000_625.0;

    #[test]
    fn plain_layout_reads_values_nulls_and_lattice() {
        // 4 x 3 nodes, row 0 the southern edge; one null.
        let values: Vec<Option<f32>> = (0..12).map(|i| if i == 5 { None } else { Some(-133.9 + i as f32 * 0.25) }).collect();
        let bytes = plain_grid("PITUPPER.SRG    01Jan20200000", [E, N], 25.0, 4, 3, &values, 640);
        let read = parse_vulcan_grid(&bytes).unwrap();
        let g = &read.grid;
        assert_eq!((g.cols, g.rows, g.origin, g.cell_size), (4, 3, [E, N], [25.0, 25.0]));
        assert_eq!(read.description, "PITUPPER.SRG    01Jan20200000");
        assert_eq!(g.z_at(0, 0), f64::from(-133.9f32), "first node is the south-west corner, widened exactly");
        assert_eq!(g.z_at(3, 2), f64::from(-133.9f32 + 11.0 * 0.25));
        assert!(g.z_at(1, 1).is_nan());
        assert_eq!(
            read.log,
            VulcanLog { nodes: 12, live: 11, nulls: 1, stale_nulls: 1, odd_flags: 0, non_finite: 0, padding_bytes: 640, compressed_blocks: 0 }
        );
    }

    #[test]
    fn lzf_literals_and_matches() {
        // "abc", then a 3-byte match 3 back.
        assert_eq!(lzf_decompress(&[0x02, b'a', b'b', b'c', 0x20, 0x02], 64).unwrap(), b"abcabc");
        // "x", then a long match (7 + 5, plus 2 = 14 bytes) one back, overlapping itself.
        assert_eq!(lzf_decompress(&[0x00, b'x', 0xe0, 0x05, 0x00], 64).unwrap(), [b'x'; 15]);
        // Distance from the low five bits and the next byte: 0x01_00 + 0x2b + 1 = 300 back.
        let literals: Vec<u8> = (0..300u32).map(|i| (i * 7 % 251) as u8).collect();
        let mut long = Vec::new();
        for run in literals.chunks(32) {
            long.push(run.len() as u8 - 1);
            long.extend_from_slice(run);
        }
        long.extend_from_slice(&[0x41, 0x2b]); // 2 + 2 = 4 bytes from 300 back
        let out = lzf_decompress(&long, 400).unwrap();
        assert_eq!(&out[300..], &literals[..4]);

        assert!(lzf_decompress(&[0x20, 0x00], 64).unwrap_err().contains("before the start"));
        assert!(lzf_decompress(&[0x05, b'a'], 64).unwrap_err().contains("literal run"));
        assert!(lzf_decompress(&[0x00, b'x', 0xe0], 64).unwrap_err().contains("ends inside"));
        assert!(lzf_decompress(&[0x00, b'x', 0xe0, 0x05, 0x00], 10).unwrap_err().contains("past 10 bytes"));
    }

    /// A 50 x 42 lattice on a known plane, with a band of nulls.
    fn plane_grid() -> (Vec<Option<f32>>, Vec<u8>) {
        let (cols, rows) = (50, 42);
        let values: Vec<Option<f32>> = (0..cols * rows)
            .map(|i| {
                let (c, r) = (i % cols, i / cols);
                (r != 7).then_some(180.0 + 0.5 * c as f32 - 0.25 * r as f32)
            })
            .collect();
        let plain = plain_grid("PITUPPER.SFG    30Oct20260900", [E, N], 10.0, cols, rows, &values, 0);
        (values, plain)
    }

    #[test]
    fn compressed_layout_unpacks_to_the_plain_one() {
        let (values, plain) = plane_grid();
        let expected = parse_vulcan_grid(&plain).unwrap();
        // 64-byte blocks need two deepest index pages under the root; 4 KB blocks with Vulcan's four
        // levels make a chain of single-entry pages, and the last block unpacks into zeros past the grid.
        for (block, depth, tail) in [(64, 2, 0), (4096, 4, 4096 - plain.len() % 4096)] {
            let file = vulz_grid(&plain, block, depth, tail);
            let read = parse_vulcan_grid(&file).unwrap();
            let blocks = plain.len().div_ceil(block);
            assert_eq!(read.log, VulcanLog { compressed_blocks: blocks, ..expected.log.clone() }, "block {block}");
            assert_eq!(read.description, "PITUPPER.SFG    30Oct20260900");
            let g = &read.grid;
            assert_eq!((g.cols, g.rows, g.origin, g.cell_size), (50, 42, [E, N], [10.0, 10.0]));
            assert_eq!(g.z_at(49, 41), f64::from(180.0f32 + 0.5 * 49.0 - 0.25 * 41.0), "north-east node, exact");
            assert!(g.z_at(3, 7).is_nan());
            for (i, v) in values.iter().enumerate() {
                assert_eq!(g.z[i].to_bits(), v.map_or(f64::NAN, f64::from).to_bits(), "node {i}");
            }
        }
    }

    #[test]
    fn damaged_compressed_files_are_refused() {
        let (_, plain) = plane_grid();
        let good = vulz_grid(&plain, 1024, 4, 0);
        assert!(parse_vulcan_grid(&good).is_ok());
        let refused = |bytes: &[u8], needle: &str| match parse_vulcan_grid(bytes) {
            Err(IoError::Binary(m)) => assert!(m.contains(needle), "{m}"),
            other => panic!("expected a refusal mentioning '{needle}', got {other:?}"),
        };
        refused(&good[..good.len() - 5], "runs past the end");
        refused(&good[..30], "too short");
        let mut depth = good.clone();
        depth[12..16].copy_from_slice(&99i32.to_le_bytes());
        refused(&depth, "index depth 99");
        let mut bigger = good.clone();
        bigger[32..40].copy_from_slice(&(plain.len() as u64 + 4096).to_le_bytes());
        refused(&bigger, "the index lists");
        let mut root = good.clone();
        root[24..28].copy_from_slice(&(good.len() as i32).to_le_bytes());
        refused(&root, "index page");
        let mut smaller = good.clone(); // the last block now holds grid bytes past the stated size
        smaller[32..40].copy_from_slice(&(plain.len() as u64 - 8).to_le_bytes());
        refused(&smaller, "past the end of the grid");
        let mut magic_only = plain.clone();
        magic_only[4..8].copy_from_slice(b"vulZ");
        refused(&magic_only, "compressed grid");
    }

    #[test]
    fn inconsistent_or_unknown_files_are_refused() {
        let values = vec![Some(1.0f32); 6];
        let good = plain_grid("X", [E, N], 25.0, 3, 2, &values, 0);
        let mut wrong_count = good.clone();
        wrong_count[64..68].copy_from_slice(&7i32.to_be_bytes());
        assert!(matches!(parse_vulcan_grid(&wrong_count), Err(IoError::Binary(m)) if m.contains("7 nodes")));
        let mut wrong_extent = good.clone();
        wrong_extent[40..48].copy_from_slice(&(E + 75.0).to_be_bytes());
        assert!(matches!(parse_vulcan_grid(&wrong_extent), Err(IoError::Binary(m)) if m.contains("do not give")));
        assert!(matches!(parse_vulcan_grid(&good[..good.len() - 1]), Err(IoError::Binary(m)) if m.contains("truncated")));
        let mut trailing = good.clone();
        trailing.extend_from_slice(&[0, 0, 7]);
        assert!(matches!(parse_vulcan_grid(&trailing), Err(IoError::Binary(m)) if m.contains("not zero padding")));
    }

    #[test]
    fn grid_names_split_into_stem_and_variable() {
        assert_eq!(vulcan_grid_name("pitupper.rag"), Some(("pitupper".into(), "ra".into())));
        assert_eq!(vulcan_grid_name("PITRL400.SFG"), Some(("pitrl400".into(), "sf".into())));
        assert_eq!(vulcan_grid_name("pitlower.s1g"), Some(("pitlower".into(), "s1".into())));
        assert_eq!(vulcan_grid_name("topo.dxf"), None);
        assert_eq!(vulcan_grid_name("notes.g"), None);
    }

    #[test]
    fn a_seam_set_reads_by_name_and_shares_one_lattice() {
        let dir = std::env::temp_dir().join(format!("zd-vulcan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let grid = |v: f32| plain_grid("t", [E, N], 25.0, 3, 2, &[Some(v); 6], 0);
        std::fs::write(dir.join("pitupper.srg"), vulz_grid(&grid(120.0), 64, 4, 0)).unwrap(); // compressed roof
        std::fs::write(dir.join("PITUPPER.SFG"), grid(118.5)).unwrap();
        std::fs::write(dir.join("pitupper.rag"), grid(30.5)).unwrap();
        std::fs::write(dir.join("pitupper.rdg"), vulz_grid(&grid(1.5), 64, 4, 0)).unwrap();
        std::fs::write(dir.join("pitupperp15.sfg"), grid(135.0)).unwrap(); // another stem: ignored
        let mut damaged = grid(0.4);
        damaged[4..8].copy_from_slice(b"vulZ");
        std::fs::write(dir.join("pitupper.tsg"), damaged).unwrap();

        let seam = read_vulcan_seam(&dir, "pit", "UPPER").unwrap();
        assert_eq!(seam.grids.keys().collect::<Vec<_>>(), ["ra", "rd", "sf", "sr"]);
        assert_eq!(seam.roof().grid.z_at(2, 1) - seam.floor().grid.z_at(2, 1), 1.5);
        assert_eq!(seam.roof().log.compressed_blocks, 3, "136 bytes in 64-byte blocks");
        assert_eq!(seam.grids["rd"].grid.z_at(0, 0), 1.5);
        assert_eq!(seam.skipped.len(), 1, "damaged ts skipped and listed");
        assert_eq!(seam.skipped[0].0, "ts");

        std::fs::write(dir.join("pitupper.tkg"), plain_grid("t", [E, N], 50.0, 3, 2, &[Some(1.5); 6], 0)).unwrap();
        assert!(matches!(read_vulcan_seam(&dir, "pit", "upper"), Err(IoError::Binary(m)) if m.contains("different lattice")));
        assert!(read_vulcan_seam(&dir, "pit", "lower").is_err(), "no roof or floor");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
