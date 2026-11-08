//! Tauri-facing monitoring session: wraps [`MonitoringController`] with a
//! macOS or mock collector.

use collector_api::{
    CollectorCapabilities, CollectorError, NetworkCollector, Platform,
};
use common::ConnectionEvent;
use database::Database;
use monitor_core::{
    EventPipeline, MockCollector, MonitoringController, MonitoringState, MonitoringStatus,
};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

/// Collector backend selected when starting monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CollectorBackend {
    /// Platform collector (macOS libproc when available).
    Native,
    /// Scripted mock collector (useful for UI/dev without live sockets).
    Mock,
}

/// Serializable status returned by monitoring commands.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringStatusDto {
    pub state: MonitoringState,
    pub last_error: Option<String>,
    pub backend: Option<CollectorBackend>,
}

impl MonitoringStatusDto {
    fn from_status(status: MonitoringStatus, backend: Option<CollectorBackend>) -> Self {
        Self {
            state: status.state,
            last_error: status.last_error,
            backend,
        }
    }
}

enum RuntimeCollector {
    #[cfg(target_os = "macos")]
    MacOs(
        collector_macos::MacOsCollector<
            collector_macos::LibprocSnapshotSource,
            process_info::MacOsProcessSource,
        >,
    ),
    Mock(MockCollector),
}

impl NetworkCollector for RuntimeCollector {
    fn platform(&self) -> Platform {
        match self {
            #[cfg(target_os = "macos")]
            Self::MacOs(c) => c.platform(),
            Self::Mock(c) => c.platform(),
        }
    }

    fn capabilities(&self) -> CollectorCapabilities {
        match self {
            #[cfg(target_os = "macos")]
            Self::MacOs(c) => c.capabilities(),
            Self::Mock(c) => c.capabilities(),
        }
    }

    async fn start(
        &mut self,
        event_tx: mpsc::Sender<ConnectionEvent>,
    ) -> Result<(), CollectorError> {
        match self {
            #[cfg(target_os = "macos")]
            Self::MacOs(c) => c.start(event_tx).await,
            Self::Mock(c) => c.start(event_tx).await,
        }
    }

    async fn stop(&mut self) -> Result<(), CollectorError> {
        match self {
            #[cfg(target_os = "macos")]
            Self::MacOs(c) => c.stop().await,
            Self::Mock(c) => c.stop().await,
        }
    }
}

fn build_collector(backend: CollectorBackend) -> Result<(RuntimeCollector, &'static str), String> {
    match backend {
        CollectorBackend::Native => {
            #[cfg(target_os = "macos")]
            {
                Ok((
                    RuntimeCollector::MacOs(collector_macos::MacOsCollector::live()),
                    "macos",
                ))
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err("native collector is only available on macOS".into())
            }
        }
        CollectorBackend::Mock => Ok((
            RuntimeCollector::Mock(MockCollector::new(Vec::new())),
            "mock",
        )),
    }
}

/// Shared app monitoring session.
pub struct MonitoringSession {
    db: Database,
    controller: Mutex<Option<MonitoringController<RuntimeCollector>>>,
    backend: Mutex<Option<CollectorBackend>>,
}

impl MonitoringSession {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            controller: Mutex::new(None),
            backend: Mutex::new(None),
        }
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    pub async fn status(&self) -> MonitoringStatusDto {
        let backend = *self.backend.lock().await;
        let guard = self.controller.lock().await;
        match guard.as_ref() {
            Some(controller) => {
                MonitoringStatusDto::from_status(controller.status().await, backend)
            }
            None => MonitoringStatusDto::from_status(MonitoringStatus::default(), None),
        }
    }

    pub async fn start(&self, backend: CollectorBackend) -> Result<MonitoringStatusDto, String> {
        let mut guard = self.controller.lock().await;
        if let Some(controller) = guard.as_ref() {
            let state = controller.status().await.state;
            if matches!(
                state,
                MonitoringState::Running | MonitoringState::Paused
            ) {
                return Err(format!("cannot start while monitoring is {state:?}"));
            }
        }

        let (collector, collector_id) = build_collector(backend)?;
        let pipeline = EventPipeline::new(self.db.clone()).with_collector_id(collector_id);
        let mut controller = MonitoringController::new(pipeline, collector);
        controller
            .start()
            .await
            .map_err(|err| err.to_string())?;

        let status = controller.status().await;
        *guard = Some(controller);
        *self.backend.lock().await = Some(backend);
        Ok(MonitoringStatusDto::from_status(status, Some(backend)))
    }

    pub async fn stop(&self) -> Result<MonitoringStatusDto, String> {
        let mut guard = self.controller.lock().await;
        let Some(controller) = guard.as_mut() else {
            return Ok(MonitoringStatusDto::from_status(
                MonitoringStatus::default(),
                None,
            ));
        };
        let state = controller.status().await.state;
        if state != MonitoringState::Stopped {
            controller.stop().await.map_err(|err| err.to_string())?;
        }
        let status = controller.status().await;
        let backend = *self.backend.lock().await;
        Ok(MonitoringStatusDto::from_status(status, backend))
    }

    pub async fn pause(&self) -> Result<MonitoringStatusDto, String> {
        let mut guard = self.controller.lock().await;
        let controller = guard
            .as_mut()
            .ok_or_else(|| "monitoring is not started".to_owned())?;
        controller.pause().await.map_err(|err| err.to_string())?;
        let status = controller.status().await;
        let backend = *self.backend.lock().await;
        Ok(MonitoringStatusDto::from_status(status, backend))
    }

    pub async fn resume(&self) -> Result<MonitoringStatusDto, String> {
        let mut guard = self.controller.lock().await;
        let controller = guard
            .as_mut()
            .ok_or_else(|| "monitoring is not started".to_owned())?;
        controller.resume().await.map_err(|err| err.to_string())?;
        let status = controller.status().await;
        let backend = *self.backend.lock().await;
        Ok(MonitoringStatusDto::from_status(status, backend))
    }
}
