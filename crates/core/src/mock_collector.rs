//! Scripted [`NetworkCollector`] for tests and early UI wiring.
//!
//! This is intentionally **not** an OS collector: it only replays a caller-provided
//! event list and does not observe the host network stack.

use std::sync::Arc;

use collector_api::{
    CollectorCapabilities, CollectorError, NetworkCollector, Platform,
};
use common::ConnectionEvent;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

/// Replays a scripted [`ConnectionEvent`] list through the collector API.
///
/// Duplicates in the script are delivered as-is; event deduplication belongs to
/// the core pipeline, not this mock.
#[derive(Debug)]
pub struct MockCollector {
    platform: Platform,
    capabilities: CollectorCapabilities,
    events: Vec<ConnectionEvent>,
    start_failure: Option<CollectorError>,
    running: bool,
    /// Shared so a prior emit task can observe stop and exit cleanly.
    stop_flag: Arc<Mutex<bool>>,
    emit_task: Option<JoinHandle<()>>,
}

impl MockCollector {
    /// Creates a mock that will replay `events` in order on each successful start.
    pub fn new(events: Vec<ConnectionEvent>) -> Self {
        Self {
            platform: Platform::MacOs,
            capabilities: CollectorCapabilities::all(),
            events,
            start_failure: None,
            running: false,
            stop_flag: Arc::new(Mutex::new(false)),
            emit_task: None,
        }
    }

    /// Overrides the platform value reported by [`NetworkCollector::platform`].
    ///
    /// The mock itself remains a scripted test double regardless of this value.
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = platform;
        self
    }

    /// Overrides reported capabilities.
    pub fn with_capabilities(mut self, capabilities: CollectorCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Configures the next [`NetworkCollector::start`] call to fail with `error`.
    ///
    /// Cleared after the failure is returned so a subsequent start can succeed
    /// (restart / recovery scenarios).
    pub fn fail_next_start(mut self, error: CollectorError) -> Self {
        self.start_failure = Some(error);
        self
    }

    /// Replaces the scripted event list (for example between restart cycles).
    pub fn set_events(&mut self, events: Vec<ConnectionEvent>) {
        self.events = events;
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}

impl NetworkCollector for MockCollector {
    fn platform(&self) -> Platform {
        self.platform
    }

    fn capabilities(&self) -> CollectorCapabilities {
        self.capabilities
    }

    async fn start(
        &mut self,
        event_tx: mpsc::Sender<ConnectionEvent>,
    ) -> Result<(), CollectorError> {
        if let Some(error) = self.start_failure.take() {
            return Err(error);
        }

        if self.running {
            return Err(CollectorError::AlreadyStarted);
        }

        *self.stop_flag.lock().await = false;
        let stop_flag = Arc::clone(&self.stop_flag);
        let events = self.events.clone();

        self.emit_task = Some(tokio::spawn(async move {
            for event in events {
                if *stop_flag.lock().await {
                    break;
                }
                if event_tx.send(event).await.is_err() {
                    break;
                }
            }
        }));

        self.running = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), CollectorError> {
        if !self.running {
            return Err(CollectorError::NotStarted);
        }

        *self.stop_flag.lock().await = true;

        if let Some(handle) = self.emit_task.take() {
            // Cooperative stop via stop_flag; abort covers a blocked send.
            handle.abort();
            let _ = handle.await;
        }

        self.running = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use collector_api::NetworkCollector;
    use common::{ConnectionEventType, Direction, IpAddress, Protocol};

    fn sample_event(
        event_type: ConnectionEventType,
        remote_port: u16,
        process_name: Option<&str>,
        bytes_sent: Option<u64>,
    ) -> ConnectionEvent {
        ConnectionEvent {
            timestamp: Utc.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap(),
            event_type,
            process_id: 4242,
            process_name: process_name.map(str::to_owned),
            process_path: None,
            process_start_time: None,
            protocol: Protocol::Tcp,
            local_ip: IpAddress::parse("127.0.0.1").unwrap(),
            local_port: 54321,
            remote_ip: IpAddress::parse("192.0.2.10").unwrap(),
            remote_port,
            direction: Direction::Outbound,
            bytes_sent,
            bytes_received: None,
        }
    }

    async fn drain_all(rx: &mut mpsc::Receiver<ConnectionEvent>) -> Vec<ConnectionEvent> {
        let mut events = Vec::new();
        // Bound the wait so a hung emitter cannot stall the suite forever.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(event)) => events.push(event),
                Ok(None) | Err(_) => break,
            }
        }
        events
    }

    #[tokio::test]
    async fn start_and_stop() {
        let mut collector = MockCollector::new(vec![sample_event(
            ConnectionEventType::NewConnection,
            443,
            Some("curl"),
            None,
        )]);
        let (tx, mut rx) = mpsc::channel(8);

        assert!(!collector.is_running());
        collector.start(tx).await.expect("start");
        assert!(collector.is_running());

        let _ = drain_all(&mut rx).await;

        collector.stop().await.expect("stop");
        assert!(!collector.is_running());
        assert_eq!(collector.stop().await, Err(CollectorError::NotStarted));
    }

    #[tokio::test]
    async fn delivers_scripted_events() {
        let script = vec![
            sample_event(ConnectionEventType::NewConnection, 443, Some("curl"), None),
            sample_event(
                ConnectionEventType::ConnectionClosed,
                443,
                Some("curl"),
                None,
            ),
        ];
        let mut collector = MockCollector::new(script.clone());
        let (tx, mut rx) = mpsc::channel(8);

        collector.start(tx).await.expect("start");
        let delivered = drain_all(&mut rx).await;
        collector.stop().await.expect("stop");

        assert_eq!(delivered, script);
    }

    #[tokio::test]
    async fn start_failure_surfaces_collector_error() {
        let mut collector = MockCollector::new(Vec::new()).fail_next_start(CollectorError::Failed(
            "injected failure".into(),
        ));
        let (tx, _rx) = mpsc::channel(1);

        let err = collector.start(tx.clone()).await.expect_err("must fail");
        assert_eq!(err, CollectorError::Failed("injected failure".into()));
        assert!(!collector.is_running());

        // Failure is one-shot; restart succeeds.
        collector.start(tx).await.expect("restart after failure");
        assert!(collector.is_running());
        collector.stop().await.expect("stop");
    }

    #[tokio::test]
    async fn duplicate_scripted_events_are_not_dropped() {
        let duplicate =
            sample_event(ConnectionEventType::NewConnection, 80, Some("browser"), None);
        let script = vec![duplicate.clone(), duplicate.clone()];
        let mut collector = MockCollector::new(script.clone());
        let (tx, mut rx) = mpsc::channel(8);

        collector.start(tx).await.expect("start");
        let delivered = drain_all(&mut rx).await;
        collector.stop().await.expect("stop");

        assert_eq!(delivered.len(), 2);
        assert_eq!(delivered, script);
    }

    #[tokio::test]
    async fn missing_optional_fields_are_preserved() {
        let event = sample_event(ConnectionEventType::TrafficUpdate, 53, None, None);
        assert!(event.process_name.is_none());
        assert!(event.process_path.is_none());
        assert!(event.process_start_time.is_none());
        assert!(event.bytes_sent.is_none());
        assert!(event.bytes_received.is_none());

        let mut collector = MockCollector::new(vec![event.clone()]);
        let (tx, mut rx) = mpsc::channel(4);

        collector.start(tx).await.expect("start");
        let delivered = drain_all(&mut rx).await;
        collector.stop().await.expect("stop");

        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0], event);
        assert!(delivered[0].process_name.is_none());
        assert!(delivered[0].process_path.is_none());
        assert!(delivered[0].process_start_time.is_none());
        assert!(delivered[0].bytes_sent.is_none());
        assert!(delivered[0].bytes_received.is_none());
    }

    #[tokio::test]
    async fn restart_replays_script_again() {
        let script = vec![sample_event(
            ConnectionEventType::NewConnection,
            22,
            Some("ssh"),
            Some(128),
        )];
        let mut collector = MockCollector::new(script.clone());

        let (tx1, mut rx1) = mpsc::channel(4);
        collector.start(tx1).await.expect("start 1");
        let first = drain_all(&mut rx1).await;
        collector.stop().await.expect("stop 1");

        let (tx2, mut rx2) = mpsc::channel(4);
        collector.start(tx2).await.expect("start 2");
        let second = drain_all(&mut rx2).await;
        collector.stop().await.expect("stop 2");

        assert_eq!(first, script);
        assert_eq!(second, script);
    }

    #[tokio::test]
    async fn reports_configured_platform_and_capabilities() {
        let collector = MockCollector::new(Vec::new())
            .with_platform(Platform::Linux)
            .with_capabilities(CollectorCapabilities::none());

        assert_eq!(collector.platform(), Platform::Linux);
        assert_eq!(collector.capabilities(), CollectorCapabilities::none());
    }

    #[tokio::test]
    async fn double_start_is_an_error() {
        let mut collector = MockCollector::new(Vec::new());
        let (tx, _rx) = mpsc::channel::<ConnectionEvent>(1);
        collector.start(tx).await.expect("first start");
        let (tx2, _rx2) = mpsc::channel(1);
        assert_eq!(
            collector.start(tx2).await.expect_err("second start"),
            CollectorError::AlreadyStarted
        );
        collector.stop().await.expect("stop");
    }
}
