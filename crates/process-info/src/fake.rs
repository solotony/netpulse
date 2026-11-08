//! In-memory process source for tests and scripted scenarios.

use std::collections::HashMap;

use crate::source::{ProcessInfo, ProcessSource};

/// Fake [`ProcessSource`] backed by an explicit PID → info map.
///
/// Unknown PIDs resolve to [`ProcessInfo::unknown`] (no guessed names).
#[derive(Debug, Default, Clone)]
pub struct FakeProcessSource {
    processes: HashMap<u32, ProcessInfo>,
}

impl FakeProcessSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace the recorded metadata for a PID.
    pub fn insert(&mut self, info: ProcessInfo) {
        self.processes.insert(info.pid, info);
    }

    /// Remove recorded metadata for a PID.
    pub fn remove(&mut self, pid: u32) {
        self.processes.remove(&pid);
    }
}

impl ProcessSource for FakeProcessSource {
    fn info(&self, pid: u32) -> ProcessInfo {
        self.processes
            .get(&pid)
            .cloned()
            .unwrap_or_else(|| ProcessInfo::unknown(pid))
    }
}
