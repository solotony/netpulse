//! macOS network connection collector.
//!
//! MVP uses 1-second `libproc` polling (not a Network Extension). Socket snapshot
//! enumeration is separated from the snapshot diff / lifecycle engine so the
//! latter is unit-testable with fixtures. See `docs/macos-collector.md` and
//! `docs/assumptions.md`.

mod collector;
mod diff;
mod snapshot;

#[cfg(target_os = "macos")]
mod libproc;

pub use collector::MacOsCollector;
pub use diff::DiffEngine;
pub use snapshot::{
    FixtureSnapshotSource, SocketKey, SocketObservation, SocketSnapshotSource, SnapshotError,
};

#[cfg(target_os = "macos")]
pub use libproc::LibprocSnapshotSource;

use collector_api::CollectorCapabilities;

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "collector-macos";

/// Polling interval for socket enumeration (milliseconds).
pub const POLL_INTERVAL_MS: u64 = 1000;

/// UDP flows with no sighting are considered closed after this many seconds.
pub const UDP_IDLE_CLOSE_SECS: u64 = 30;

/// Capabilities actually provided by the macOS polling collector.
///
/// `traffic_byte_counters` is false: libproc FD enumeration does not expose
/// per-flow byte counters for this MVP path.
pub const fn macos_capabilities() -> CollectorCapabilities {
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

#[cfg(test)]
mod tests;
