//! Process identity resolution (PID together with process start time).

mod fake;
mod source;

#[cfg(target_os = "macos")]
mod macos;

pub use fake::FakeProcessSource;
pub use source::{process_instance_key, ProcessInfo, ProcessSource};

#[cfg(target_os = "macos")]
pub use macos::MacOsProcessSource;

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "process-info";

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use common::ProcessInstanceKey;

    #[test]
    fn crate_name_is_process_info() {
        assert_eq!(CRATE_NAME, "process-info");
    }

    #[test]
    fn process_instance_key_requires_start_time() {
        let start = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 0).unwrap();

        assert_eq!(
            process_instance_key(100, Some(start)),
            Some(ProcessInstanceKey::new(100, start))
        );
        assert_eq!(process_instance_key(100, None), None);
    }

    #[test]
    fn process_info_instance_key_matches_helper() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let info = ProcessInfo {
            pid: 7,
            name: Some("demo".into()),
            executable_path: Some("/usr/bin/demo".into()),
            process_start_time: Some(start),
        };

        assert_eq!(info.instance_key(), process_instance_key(7, Some(start)));
        assert_eq!(ProcessInfo::unknown(7).instance_key(), None);
    }

    #[test]
    fn fake_source_pid_reuse_with_different_start_time_is_new_instance() {
        let start_a = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let start_b = Utc.with_ymd_and_hms(2024, 6, 1, 12, 30, 0).unwrap();

        let mut source = FakeProcessSource::new();
        source.insert(ProcessInfo {
            pid: 4242,
            name: Some("first".into()),
            executable_path: Some("/tmp/first".into()),
            process_start_time: Some(start_a),
        });

        let first_key = source
            .info(4242)
            .instance_key()
            .expect("start time should yield an identity key");

        // OS reused the PID for a different process instance.
        source.insert(ProcessInfo {
            pid: 4242,
            name: Some("second".into()),
            executable_path: Some("/tmp/second".into()),
            process_start_time: Some(start_b),
        });

        let reused_key = source
            .info(4242)
            .instance_key()
            .expect("start time should yield an identity key");

        assert_eq!(first_key.pid, reused_key.pid);
        assert_ne!(first_key, reused_key);
        assert_ne!(first_key.process_start_time, reused_key.process_start_time);
    }

    #[test]
    fn fake_source_unknown_pid_has_no_guessed_fields() {
        let source = FakeProcessSource::new();
        let info = source.info(999_999);

        assert_eq!(info.pid, 999_999);
        assert_eq!(info.name, None);
        assert_eq!(info.executable_path, None);
        assert_eq!(info.process_start_time, None);
        assert_eq!(info.instance_key(), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_lookup_current_process_returns_some_fields() {
        let pid = std::process::id();
        let info = MacOsProcessSource.info(pid);

        assert_eq!(info.pid, pid);
        assert!(
            info.process_start_time.is_some(),
            "current process should expose a start time"
        );
        // Name and path are best-effort; at least one should resolve for this process.
        assert!(
            info.name.is_some() || info.executable_path.is_some(),
            "current process should expose a name or executable path"
        );
        assert!(info.instance_key().is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_lookup_missing_pid_does_not_guess() {
        let info = MacOsProcessSource.info(u32::MAX);
        assert_eq!(info.pid, u32::MAX);
        assert_eq!(info.name, None);
        assert_eq!(info.executable_path, None);
        assert_eq!(info.process_start_time, None);
    }
}
