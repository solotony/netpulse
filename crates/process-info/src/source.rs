//! Process metadata types and lookup trait.

use chrono::{DateTime, Utc};
use common::ProcessInstanceKey;

/// Resolved process metadata for a single PID.
///
/// Fields that cannot be read are left as `None`. Callers must not invent
/// names, paths, or start times when a lookup fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: Option<String>,
    pub executable_path: Option<String>,
    pub process_start_time: Option<DateTime<Utc>>,
}

impl ProcessInfo {
    /// PID-only placeholder used when no metadata is available.
    pub fn unknown(pid: u32) -> Self {
        Self {
            pid,
            name: None,
            executable_path: None,
            process_start_time: None,
        }
    }

    /// Builds a [`ProcessInstanceKey`] when start time is known.
    pub fn instance_key(&self) -> Option<ProcessInstanceKey> {
        process_instance_key(self.pid, self.process_start_time)
    }
}

/// Builds a [`ProcessInstanceKey`] when `process_start_time` is known.
///
/// Returns `None` when start time is missing so callers do not fabricate an
/// identity from PID alone (PIDs are reused by the OS).
pub fn process_instance_key(
    pid: u32,
    process_start_time: Option<DateTime<Utc>>,
) -> Option<ProcessInstanceKey> {
    process_start_time.map(|process_start_time| ProcessInstanceKey::new(pid, process_start_time))
}

/// Resolves process name, executable path, and start time for a PID.
pub trait ProcessSource {
    /// Look up metadata for `pid`.
    ///
    /// Missing or inaccessible fields must be `None` — never guessed.
    fn info(&self, pid: u32) -> ProcessInfo;
}
