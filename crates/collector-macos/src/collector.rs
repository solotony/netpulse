//! [`NetworkCollector`] implementation for the macOS polling collector.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use collector_api::{CollectorCapabilities, CollectorError, NetworkCollector, Platform};
use common::ConnectionEvent;
use process_info::ProcessSource;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::diff::DiffEngine;
use crate::snapshot::SocketSnapshotSource;
use crate::{macos_capabilities, POLL_INTERVAL_MS, UDP_IDLE_CLOSE_SECS};

/// Live / testable macOS network collector.
///
/// Snapshot enumeration (`E`) is separated from process lookup (`P`) and from the
/// [`DiffEngine`] so fixture-based tests never call libproc.
#[derive(Debug)]
pub struct MacOsCollector<E, P> {
    enumerator: Arc<E>,
    processes: Arc<P>,
    running: bool,
    stop_flag: Arc<AtomicBool>,
    poll_task: Option<JoinHandle<()>>,
}

impl<E, P> MacOsCollector<E, P>
where
    E: SocketSnapshotSource + 'static,
    P: ProcessSource + Send + Sync + 'static,
{
    /// Builds a collector from explicit snapshot and process sources.
    pub fn with_sources(enumerator: E, processes: P) -> Self {
        Self {
            enumerator: Arc::new(enumerator),
            processes: Arc::new(processes),
            running: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
            poll_task: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(POLL_INTERVAL_MS)
    }

    pub fn udp_idle_timeout(&self) -> Duration {
        Duration::from_secs(UDP_IDLE_CLOSE_SECS)
    }
}

#[cfg(target_os = "macos")]
impl MacOsCollector<crate::libproc::LibprocSnapshotSource, process_info::MacOsProcessSource> {
    /// Live collector: libproc socket enumeration + macOS process lookup.
    pub fn live() -> Self {
        Self::with_sources(
            crate::libproc::LibprocSnapshotSource,
            process_info::MacOsProcessSource,
        )
    }
}

impl<E, P> NetworkCollector for MacOsCollector<E, P>
where
    E: SocketSnapshotSource + 'static,
    P: ProcessSource + Send + Sync + 'static,
{
    fn platform(&self) -> Platform {
        Platform::MacOs
    }

    fn capabilities(&self) -> CollectorCapabilities {
        macos_capabilities()
    }

    async fn start(
        &mut self,
        event_tx: mpsc::Sender<ConnectionEvent>,
    ) -> Result<(), CollectorError> {
        if self.running {
            return Err(CollectorError::AlreadyStarted);
        }

        self.stop_flag.store(false, Ordering::SeqCst);
        let stop_flag = Arc::clone(&self.stop_flag);
        let enumerator = Arc::clone(&self.enumerator);
        let processes = Arc::clone(&self.processes);
        let poll_interval = self.poll_interval();
        let udp_idle = self.udp_idle_timeout();

        self.poll_task = Some(tokio::spawn(async move {
            let mut engine = DiffEngine::new(udp_idle);
            let mut interval = tokio::time::interval(poll_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                interval.tick().await;
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }

                let observed = match enumerator.snapshot() {
                    Ok(sockets) => sockets,
                    Err(_) => {
                        // Best-effort: skip this tick; next poll may succeed.
                        continue;
                    }
                };

                let now = Utc::now();
                let events = engine.apply(&observed, now, processes.as_ref());
                for event in events {
                    if stop_flag.load(Ordering::SeqCst) {
                        return;
                    }
                    if event_tx.send(event).await.is_err() {
                        return;
                    }
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

        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.poll_task.take() {
            handle.abort();
            let _ = handle.await;
        }
        self.running = false;
        Ok(())
    }
}
