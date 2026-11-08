//! Process instance identity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Stable process identity: PID plus process start time.
///
/// Operating systems reuse PIDs, so the same numeric PID at two different start
/// times refers to two different process instances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcessInstanceKey {
    pub pid: u32,
    pub process_start_time: DateTime<Utc>,
}

impl ProcessInstanceKey {
    pub fn new(pid: u32, process_start_time: DateTime<Utc>) -> Self {
        Self {
            pid,
            process_start_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn process_instance_differs_when_pid_reused() {
        let start_a = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let start_b = Utc.with_ymd_and_hms(2024, 6, 1, 12, 30, 0).unwrap();

        let first = ProcessInstanceKey::new(4242, start_a);
        let reused = ProcessInstanceKey::new(4242, start_b);

        assert_ne!(first, reused);
        assert_eq!(first.pid, reused.pid);
    }
}
