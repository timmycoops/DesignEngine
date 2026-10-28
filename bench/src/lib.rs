//! Phase 0 stress tests: synthetic pit surfaces with known shapes, golden-data discovery, and
//! small reporting helpers shared by the bench targets.

pub mod golden;
pub mod synth;

use std::path::PathBuf;
use std::time::Instant;

/// `bench/out`, created if missing. Git-ignored.
pub fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out");
    std::fs::create_dir_all(&dir).expect("create bench/out");
    dir
}

/// Runs `f` and returns its result with the elapsed milliseconds.
pub fn timed<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let start = Instant::now();
    let value = f();
    (value, start.elapsed().as_secs_f64() * 1e3)
}

/// Thousands separators for triangle counts in reports.
pub fn count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}
