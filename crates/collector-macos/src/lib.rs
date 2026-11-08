//! macOS network connection collector.
//!
//! MVP uses 1s `libproc` polling (not Network Extension). See `docs/assumptions.md`.

use collector_api::{CollectorCapabilities, Platform};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "collector-macos";

/// Polling interval for socket enumeration (milliseconds).
pub const POLL_INTERVAL_MS: u64 = 1000;

/// UDP flows with no activity are considered closed after this many seconds.
pub const UDP_IDLE_CLOSE_SECS: u64 = 30;

/// macOS collector stub for the MVP workspace.
#[derive(Debug, Default)]
pub struct MacOsCollector;

impl MacOsCollector {
    pub fn platform(&self) -> Platform {
        Platform::MacOs
    }

    pub fn capabilities(&self) -> CollectorCapabilities {
        CollectorCapabilities {
            new_connection_detection: true,
            existing_connection_enumeration: true,
            connection_termination_detection: true,
            tcp: true,
            udp: true,
            traffic_byte_counters: false,
            process_identification: true,
            incoming_connection_detection: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_collector_macos() {
        assert_eq!(CRATE_NAME, "collector-macos");
    }

    #[test]
    fn platform_is_macos() {
        assert_eq!(MacOsCollector.platform(), Platform::MacOs);
    }

    #[test]
    fn byte_counters_unavailable() {
        assert!(!MacOsCollector.capabilities().traffic_byte_counters);
    }
}
