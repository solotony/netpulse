//! Common interface for platform network collectors.

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "collector-api";

/// Target operating system for a collector implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Linux,
    Windows,
}

/// Capabilities reported by a collector implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CollectorCapabilities {
    pub new_connection_detection: bool,
    pub existing_connection_enumeration: bool,
    pub connection_termination_detection: bool,
    pub tcp: bool,
    pub udp: bool,
    pub traffic_byte_counters: bool,
    pub process_identification: bool,
    pub incoming_connection_detection: bool,
}

impl CollectorCapabilities {
    /// All capabilities disabled (used by unimplemented platform stubs).
    pub const fn none() -> Self {
        Self {
            new_connection_detection: false,
            existing_connection_enumeration: false,
            connection_termination_detection: false,
            tcp: false,
            udp: false,
            traffic_byte_counters: false,
            process_identification: false,
            incoming_connection_detection: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_is_collector_api() {
        assert_eq!(CRATE_NAME, "collector-api");
    }

    #[test]
    fn none_capabilities_are_all_false() {
        let caps = CollectorCapabilities::none();
        assert!(!caps.new_connection_detection);
        assert!(!caps.tcp);
        assert!(!caps.udp);
    }
}
