//! Snapshot diff / connection lifecycle engine.
//!
//! Compares successive [`SocketObservation`] lists and emits
//! [`ConnectionEvent`] values. Unit tests drive this with fixtures — no libproc.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use common::{ConnectionEvent, ConnectionEventType, Protocol};
use process_info::ProcessSource;

use crate::snapshot::{SocketKey, SocketObservation};
use crate::UDP_IDLE_CLOSE_SECS;

/// Tracks active flows and emits new / closed events from snapshot diffs.
#[derive(Debug)]
pub struct DiffEngine {
    active: HashMap<SocketKey, TrackedFlow>,
    udp_idle: Duration,
}

#[derive(Debug, Clone)]
struct TrackedFlow {
    observation: SocketObservation,
    last_seen: DateTime<Utc>,
}

impl Default for DiffEngine {
    fn default() -> Self {
        Self::new(Duration::from_secs(UDP_IDLE_CLOSE_SECS))
    }
}

impl DiffEngine {
    pub fn new(udp_idle: Duration) -> Self {
        Self {
            active: HashMap::new(),
            udp_idle,
        }
    }

    pub fn udp_idle(&self) -> Duration {
        self.udp_idle
    }

    /// Diff `observed` against tracked state at `now`, enriching via `processes`.
    ///
    /// - TCP: appear → `NewConnection`; disappear → `ConnectionClosed` immediately.
    /// - UDP: appear → `NewConnection`; reappear refreshes idle timer; absent for
    ///   `udp_idle` → `ConnectionClosed`.
    /// - Identical successive snapshots emit nothing.
    pub fn apply(
        &mut self,
        observed: &[SocketObservation],
        now: DateTime<Utc>,
        processes: &impl ProcessSource,
    ) -> Vec<ConnectionEvent> {
        let mut events = Vec::new();
        let mut seen = HashSet::with_capacity(observed.len());

        for obs in observed {
            let key = obs.key();
            seen.insert(key.clone());
            if let Some(tracked) = self.active.get_mut(&key) {
                tracked.last_seen = now;
                tracked.observation = obs.clone();
            } else {
                events.push(event_from(obs, ConnectionEventType::NewConnection, now, processes));
                self.active.insert(
                    key,
                    TrackedFlow {
                        observation: obs.clone(),
                        last_seen: now,
                    },
                );
            }
        }

        let mut to_close = Vec::new();
        for (key, tracked) in &self.active {
            if seen.contains(key) {
                continue;
            }
            match key.protocol {
                Protocol::Udp => {
                    let idle = now
                        .signed_duration_since(tracked.last_seen)
                        .to_std()
                        .unwrap_or(Duration::ZERO);
                    if idle >= self.udp_idle {
                        to_close.push(key.clone());
                    }
                }
                // TCP (and any other protocol) close as soon as they leave the snapshot.
                _ => to_close.push(key.clone()),
            }
        }

        for key in to_close {
            if let Some(tracked) = self.active.remove(&key) {
                events.push(event_from(
                    &tracked.observation,
                    ConnectionEventType::ConnectionClosed,
                    now,
                    processes,
                ));
            }
        }

        events
    }
}

fn event_from(
    obs: &SocketObservation,
    event_type: ConnectionEventType,
    timestamp: DateTime<Utc>,
    processes: &impl ProcessSource,
) -> ConnectionEvent {
    let info = processes.info(obs.pid);
    ConnectionEvent {
        timestamp,
        event_type,
        process_id: obs.pid,
        process_name: info.name,
        process_path: info.executable_path,
        process_start_time: info.process_start_time,
        protocol: obs.protocol,
        local_ip: obs.local_ip,
        local_port: obs.local_port,
        remote_ip: obs.remote_ip,
        remote_port: obs.remote_port,
        direction: obs.direction,
        // libproc polling cannot supply byte counters.
        bytes_sent: None,
        bytes_received: None,
    }
}
