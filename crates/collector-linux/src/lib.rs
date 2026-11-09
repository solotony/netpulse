//! Linux network connection collector (not yet implemented).

use collector_api::{
    CollectorCapabilities, CollectorError, NetworkCollector, Platform,
};
use common::ConnectionEvent;
use tokio::sync::mpsc;

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "collector-linux";

/// Linux collector stub — capabilities are all false; `start` is unsupported.
#[derive(Debug, Default)]
pub struct LinuxCollector;

impl LinuxCollector {
    pub fn new() -> Self {
        Self
    }
}

impl NetworkCollector for LinuxCollector {
    fn platform(&self) -> Platform {
        Platform::Linux
    }

    fn capabilities(&self) -> CollectorCapabilities {
        CollectorCapabilities::none()
    }

    async fn start(
        &mut self,
        _event_tx: mpsc::Sender<ConnectionEvent>,
    ) -> Result<(), CollectorError> {
        Err(CollectorError::NotImplemented)
    }

    async fn stop(&mut self) -> Result<(), CollectorError> {
        Err(CollectorError::NotStarted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use collector_api::NetworkCollector;

    #[test]
    fn crate_name_is_collector_linux() {
        assert_eq!(CRATE_NAME, "collector-linux");
    }

    #[test]
    fn capabilities_are_all_false() {
        let collector = LinuxCollector::new();
        let caps = collector.capabilities();
        assert_eq!(caps, CollectorCapabilities::none());
        assert!(!caps.new_connection_detection);
        assert!(!caps.existing_connection_enumeration);
        assert!(!caps.connection_termination_detection);
        assert!(!caps.tcp);
        assert!(!caps.udp);
        assert!(!caps.traffic_byte_counters);
        assert!(!caps.process_identification);
        assert!(!caps.incoming_connection_detection);
    }

    #[tokio::test]
    async fn start_returns_not_implemented() {
        let mut collector = LinuxCollector::new();
        assert_eq!(collector.platform(), Platform::Linux);

        let (tx, _rx) = mpsc::channel(1);
        let err = collector.start(tx).await.expect_err("start must fail");
        assert_eq!(err, CollectorError::NotImplemented);
    }
}
