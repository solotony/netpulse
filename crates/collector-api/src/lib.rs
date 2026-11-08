//! Common interface for platform network collectors.

use std::fmt;
use std::future::Future;

use common::ConnectionEvent;
use tokio::sync::mpsc;

pub use common::Platform;

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "collector-api";

/// Capabilities reported by a collector implementation.
///
/// The UI must not advertise capabilities that are `false`.
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

    /// Full capability set (useful for scripted / test collectors).
    pub const fn all() -> Self {
        Self {
            new_connection_detection: true,
            existing_connection_enumeration: true,
            connection_termination_detection: true,
            tcp: true,
            udp: true,
            traffic_byte_counters: true,
            process_identification: true,
            incoming_connection_detection: true,
        }
    }
}

/// Typed failures from collector lifecycle operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectorError {
    /// `start` was called while the collector was already running.
    AlreadyStarted,
    /// `stop` was called while the collector was not running.
    NotStarted,
    /// The collector failed during operation.
    Failed(String),
    /// The platform implementation is not available yet.
    NotImplemented,
}

impl fmt::Display for CollectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyStarted => write!(f, "collector already started"),
            Self::NotStarted => write!(f, "collector is not started"),
            Self::Failed(message) => write!(f, "collector failed: {message}"),
            Self::NotImplemented => write!(f, "collector not implemented on this platform"),
        }
    }
}

impl std::error::Error for CollectorError {}

/// Platform-agnostic network connection collector.
///
/// Implementations emit [`ConnectionEvent`] values on the channel passed to
/// [`NetworkCollector::start`]. Async methods use RPITIT so callers can `.await`
/// them without the `async-trait` crate.
pub trait NetworkCollector: Send + Sync {
    /// Operating system this collector targets.
    fn platform(&self) -> Platform;

    /// Capabilities this collector actually provides.
    fn capabilities(&self) -> CollectorCapabilities;

    /// Begin collecting and emit events on `event_tx`.
    ///
    /// Returns once monitoring has been started. Events continue to be sent
    /// asynchronously until [`NetworkCollector::stop`] or the channel closes.
    fn start(
        &mut self,
        event_tx: mpsc::Sender<ConnectionEvent>,
    ) -> impl Future<Output = Result<(), CollectorError>> + Send;

    /// Stop collecting and release resources.
    fn stop(&mut self) -> impl Future<Output = Result<(), CollectorError>> + Send;
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
        assert!(!caps.existing_connection_enumeration);
        assert!(!caps.connection_termination_detection);
        assert!(!caps.tcp);
        assert!(!caps.udp);
        assert!(!caps.traffic_byte_counters);
        assert!(!caps.process_identification);
        assert!(!caps.incoming_connection_detection);
    }

    #[test]
    fn all_capabilities_are_all_true() {
        let caps = CollectorCapabilities::all();
        assert!(caps.new_connection_detection);
        assert!(caps.existing_connection_enumeration);
        assert!(caps.connection_termination_detection);
        assert!(caps.tcp);
        assert!(caps.udp);
        assert!(caps.traffic_byte_counters);
        assert!(caps.process_identification);
        assert!(caps.incoming_connection_detection);
    }

    #[test]
    fn collector_error_display_is_stable() {
        assert_eq!(
            CollectorError::Failed("boom".into()).to_string(),
            "collector failed: boom"
        );
        assert_eq!(
            CollectorError::AlreadyStarted.to_string(),
            "collector already started"
        );
    }
}
