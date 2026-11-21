//! Which licence this copy runs under, as its host says (decision 0098): written in each project's manifest and on
//! each report. A line of text, never a lock: a project saved under another licence opens as any other.

use std::sync::RwLock;

static STAMP: RwLock<Option<String>> = RwLock::new(None);

/// Sets the stamp, such as `ZD-0001, licensed to Example Coal Pty Ltd`, or none.
pub fn set_licence_stamp(stamp: Option<String>) {
    *STAMP.write().unwrap_or_else(|e| e.into_inner()) = stamp;
}

/// The stamp now, if the host has set one.
pub fn licence_stamp() -> Option<String> {
    STAMP.read().unwrap_or_else(|e| e.into_inner()).clone()
}
