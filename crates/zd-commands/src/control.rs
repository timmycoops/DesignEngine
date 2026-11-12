//! How far a run has got, and how it's told to stop (decision 0060, workstream 2). The run and whoever started it
//! share one [`RunControl`]: the command reports through its [`crate::Inputs`], and the app reads progress and asks it
//! to stop from another thread. A cancelled run stops at the next solid, strip or file it checks at, and nothing it
//! made is applied.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::Serialize;

/// Shared between a run and whoever started it.
#[derive(Debug, Default)]
pub struct RunControl {
    cancelled: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,
    /// What's counted: "solids", "strips", "files".
    of: Mutex<&'static str>,
}

/// How far a run has got: `done` of `total` of what's counted. Nothing counted yet is `total` 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
    pub of: &'static str,
}

impl RunControl {
    pub fn new() -> Self {
        RunControl::default()
    }

    /// Asks the run to stop. It stops at the next place it checks, and nothing it made is applied.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn progress(&self) -> Progress {
        Progress { done: self.done.load(Ordering::Relaxed), total: self.total.load(Ordering::Relaxed), of: *self.of.lock().unwrap_or_else(|p| p.into_inner()) }
    }

    pub(crate) fn set(&self, done: usize, total: usize, of: &'static str) {
        *self.of.lock().unwrap_or_else(|p| p.into_inner()) = of;
        self.total.store(total as u64, Ordering::Relaxed);
        self.done.store(done as u64, Ordering::Relaxed);
    }

    /// One more done, for work spread over threads.
    pub(crate) fn step(&self) {
        self.done.fetch_add(1, Ordering::Relaxed);
    }
}
