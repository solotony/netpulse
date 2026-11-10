//! Monitoring lifecycle controller (start / stop / pause / resume / status).
//!
//! Pause stops collector emission and pipeline processing without clearing
//! persisted connection history. Collector failures are logged via `tracing`
//! and surfaced as [`MonitoringState::Failed`] without deleting rows.

use std::fmt;
use std::sync::Arc;

use collector_api::{CollectorError, NetworkCollector};
use serde::Serialize;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing;

use crate::pipeline::{EventPipeline, PipelineError};

/// High-level monitoring lifecycle state exposed to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MonitoringState {
    Stopped,
    Running,
    Paused,
    Failed,
}

/// Snapshot the UI (and tests) can read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringStatus {
    pub state: MonitoringState,
    /// Last collector / pipeline error message when `state` is [`MonitoringState::Failed`].
    pub last_error: Option<String>,
}

impl Default for MonitoringStatus {
    fn default() -> Self {
        Self {
            state: MonitoringState::Stopped,
            last_error: None,
        }
    }
}

/// Failures from monitoring lifecycle operations.
#[derive(Debug)]
pub enum MonitoringError {
    /// The requested transition is not valid from the current state.
    InvalidState {
        current: MonitoringState,
        action: &'static str,
    },
    Collector(CollectorError),
    Pipeline(PipelineError),
}

impl fmt::Display for MonitoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState { current, action } => {
                write!(f, "cannot {action} while monitoring is {current:?}")
            }
            Self::Collector(err) => write!(f, "monitoring collector error: {err}"),
            Self::Pipeline(err) => write!(f, "monitoring pipeline error: {err}"),
        }
    }
}

impl std::error::Error for MonitoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Collector(err) => Some(err),
            Self::Pipeline(err) => Some(err),
            Self::InvalidState { .. } => None,
        }
    }
}

impl From<CollectorError> for MonitoringError {
    fn from(value: CollectorError) -> Self {
        Self::Collector(value)
    }
}

impl From<PipelineError> for MonitoringError {
    fn from(value: PipelineError) -> Self {
        Self::Pipeline(value)
    }
}

struct SharedStatus {
    status: MonitoringStatus,
}

/// Owns a [`NetworkCollector`] and drives an [`EventPipeline`].
pub struct MonitoringController<C: NetworkCollector> {
    collector: C,
    pipeline: EventPipeline,
    shared: Arc<Mutex<SharedStatus>>,
    pipeline_task: Option<JoinHandle<()>>,
}

impl<C: NetworkCollector + Send + 'static> MonitoringController<C> {
    /// Creates a controller in the [`MonitoringState::Stopped`] state.
    pub fn new(pipeline: EventPipeline, collector: C) -> Self {
        Self {
            collector,
            pipeline,
            shared: Arc::new(Mutex::new(SharedStatus {
                status: MonitoringStatus::default(),
            })),
            pipeline_task: None,
        }
    }

    /// Shared database handle used by the pipeline.
    pub fn database(&self) -> &database::Database {
        self.pipeline.database()
    }

    /// Current status snapshot (safe to call from the UI).
    pub async fn status(&self) -> MonitoringStatus {
        self.shared.lock().await.status.clone()
    }

    /// Mutable access to the underlying collector (tests / recovery helpers).
    pub fn collector_mut(&mut self) -> &mut C {
        &mut self.collector
    }

    /// Starts the collector and pipeline. Failures set [`MonitoringState::Failed`].
    pub async fn start(&mut self) -> Result<(), MonitoringError> {
        let current = self.shared.lock().await.status.state;
        match current {
            MonitoringState::Stopped | MonitoringState::Failed => {}
            MonitoringState::Running | MonitoringState::Paused => {
                return Err(MonitoringError::InvalidState {
                    current,
                    action: "start",
                });
            }
        }

        self.spawn_collector_pipeline().await
    }

    /// Stops emission/processing. History is retained.
    pub async fn stop(&mut self) -> Result<(), MonitoringError> {
        let current = self.shared.lock().await.status.state;
        match current {
            MonitoringState::Running | MonitoringState::Paused | MonitoringState::Failed => {}
            MonitoringState::Stopped => {
                return Err(MonitoringError::InvalidState {
                    current,
                    action: "stop",
                });
            }
        }

        self.halt_runtime().await?;
        let mut shared = self.shared.lock().await;
        shared.status.state = MonitoringState::Stopped;
        // Keep last_error visible after an intentional stop from Failed, but clear
        // when leaving a healthy Running/Paused session.
        if current != MonitoringState::Failed {
            shared.status.last_error = None;
        }
        Ok(())
    }

    /// Pauses emission/processing without clearing connection history.
    pub async fn pause(&mut self) -> Result<(), MonitoringError> {
        let current = self.shared.lock().await.status.state;
        if current != MonitoringState::Running {
            return Err(MonitoringError::InvalidState {
                current,
                action: "pause",
            });
        }

        self.halt_runtime().await?;
        let mut shared = self.shared.lock().await;
        shared.status.state = MonitoringState::Paused;
        shared.status.last_error = None;
        Ok(())
    }

    /// Resumes after [`MonitoringController::pause`].
    pub async fn resume(&mut self) -> Result<(), MonitoringError> {
        let current = self.shared.lock().await.status.state;
        if current != MonitoringState::Paused {
            return Err(MonitoringError::InvalidState {
                current,
                action: "resume",
            });
        }

        self.spawn_collector_pipeline().await
    }

    async fn spawn_collector_pipeline(&mut self) -> Result<(), MonitoringError> {
        let (tx, rx) = mpsc::channel(256);
        match self.collector.start(tx).await {
            Ok(()) => {}
            Err(err) => {
                tracing::error!(error = %err, "collector failed to start");
                let mut shared = self.shared.lock().await;
                shared.status.state = MonitoringState::Failed;
                shared.status.last_error = Some(err.to_string());
                return Err(MonitoringError::Collector(err));
            }
        }

        let pipeline = self.pipeline.clone();
        let shared = Arc::clone(&self.shared);
        self.pipeline_task = Some(tokio::spawn(async move {
            if let Err(err) = pipeline.run(rx).await {
                tracing::error!(error = %err, "event pipeline failed");
                let mut guard = shared.lock().await;
                // Do not overwrite an intentional stop/pause transition.
                if guard.status.state == MonitoringState::Running {
                    guard.status.state = MonitoringState::Failed;
                    guard.status.last_error = Some(err.to_string());
                }
            }
        }));

        let mut shared = self.shared.lock().await;
        shared.status.state = MonitoringState::Running;
        shared.status.last_error = None;
        Ok(())
    }

    async fn halt_runtime(&mut self) -> Result<(), MonitoringError> {
        // Stop emission first; dropping the collector sender closes the pipeline channel.
        match self.collector.stop().await {
            Ok(()) => {}
            Err(CollectorError::NotStarted) => {}
            Err(err) => {
                tracing::error!(error = %err, "collector failed to stop");
                let mut shared = self.shared.lock().await;
                shared.status.state = MonitoringState::Failed;
                shared.status.last_error = Some(err.to_string());
                return Err(MonitoringError::Collector(err));
            }
        }

        if let Some(handle) = self.pipeline_task.take() {
            let _ = handle.await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use collector_api::CollectorError;
    use common::{ConnectionEvent, ConnectionEventType, Direction, IpAddress, Protocol};
    use database::Database;

    use crate::MockCollector;

    fn sample_event(remote_port: u16, process_name: &str) -> ConnectionEvent {
        let process_start = Utc.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap();
        ConnectionEvent {
            timestamp: Utc.with_ymd_and_hms(2026, 11, 9, 0, 1, 0).unwrap(),
            event_type: ConnectionEventType::NewConnection,
            process_id: 4242,
            process_name: Some(process_name.into()),
            process_path: Some("/usr/bin/curl".into()),
            process_start_time: Some(process_start),
            protocol: Protocol::Tcp,
            local_ip: IpAddress::parse("192.168.1.10").unwrap(),
            local_port: 54321,
            remote_ip: IpAddress::parse("93.184.216.34").unwrap(),
            remote_port,
            direction: Direction::Outbound,
            bytes_sent: Some(128),
            bytes_received: Some(4096),
            remote_hostname: None,
        }
    }

    async fn connection_count(db: &Database) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM connections")
            .fetch_one(db.pool())
            .await
            .expect("count")
    }

    #[tokio::test]
    async fn pause_resume_state_machine_preserves_history() {
        let db = Database::open_in_memory().await.expect("db");
        let pipeline = EventPipeline::new(db.clone()).with_collector_id("mock");

        // Seed history before the controller runs.
        pipeline
            .process(sample_event(443, "curl"))
            .await
            .expect("seed");
        assert_eq!(connection_count(&db).await, 1);

        let collector = MockCollector::new(vec![sample_event(80, "browser")])
            .with_emit_delay(std::time::Duration::from_millis(30));
        let mut controller = MonitoringController::new(pipeline, collector);

        controller.start().await.expect("start");
        assert_eq!(controller.status().await.state, MonitoringState::Running);

        // Wait until the delayed mock event is persisted.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
        while connection_count(&db).await < 2 {
            if tokio::time::Instant::now() > deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(connection_count(&db).await, 2);

        controller.pause().await.expect("pause");
        assert_eq!(controller.status().await.state, MonitoringState::Paused);
        assert_eq!(
            connection_count(&db).await,
            2,
            "pause must not clear history"
        );

        // While paused, no further processing (collector is stopped).
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert_eq!(connection_count(&db).await, 2);

        controller.resume().await.expect("resume");
        assert_eq!(controller.status().await.state, MonitoringState::Running);
        assert_eq!(
            connection_count(&db).await,
            2,
            "resume must not clear history"
        );

        controller.stop().await.expect("stop");
        assert_eq!(controller.status().await.state, MonitoringState::Stopped);
        assert_eq!(connection_count(&db).await, 2);
    }

    #[tokio::test]
    async fn collector_error_sets_failed_and_does_not_delete_connections() {
        let db = Database::open_in_memory().await.expect("db");
        let pipeline = EventPipeline::new(db.clone()).with_collector_id("mock");
        pipeline
            .process(sample_event(443, "curl"))
            .await
            .expect("seed");
        pipeline
            .process(sample_event(80, "browser"))
            .await
            .expect("seed 2");
        assert_eq!(connection_count(&db).await, 2);

        let collector = MockCollector::new(Vec::new())
            .fail_next_start(CollectorError::Failed("injected collector failure".into()));
        let mut controller = MonitoringController::new(pipeline, collector);

        let err = controller.start().await.expect_err("start must fail");
        assert!(matches!(
            err,
            MonitoringError::Collector(CollectorError::Failed(_))
        ));

        let status = controller.status().await;
        assert_eq!(status.state, MonitoringState::Failed);
        assert!(
            status
                .last_error
                .as_deref()
                .is_some_and(|m| m.contains("injected collector failure")),
            "status should expose the failure: {:?}",
            status.last_error
        );
        assert_eq!(
            connection_count(&db).await,
            2,
            "collector failure must not delete connections"
        );
    }

    #[tokio::test]
    async fn invalid_transitions_are_rejected() {
        let db = Database::open_in_memory().await.expect("db");
        let pipeline = EventPipeline::new(db);
        let mut controller = MonitoringController::new(pipeline, MockCollector::new(Vec::new()));

        assert!(matches!(
            controller.pause().await,
            Err(MonitoringError::InvalidState { .. })
        ));
        assert!(matches!(
            controller.resume().await,
            Err(MonitoringError::InvalidState { .. })
        ));
        assert!(matches!(
            controller.stop().await,
            Err(MonitoringError::InvalidState { .. })
        ));
    }
}
