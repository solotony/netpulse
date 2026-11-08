//! Normalized connection events produced by collectors.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ip::IpAddress;
use crate::process::ProcessInstanceKey;

/// Kind of change observed for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionEventType {
    NewConnection,
    ConnectionClosed,
    ConnectionStateChanged,
    TrafficUpdate,
}

/// Transport protocol for a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Tcp,
    Udp,
    Icmp,
    Other,
}

/// Traffic direction relative to the local host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Outbound,
    Inbound,
    Unknown,
}

/// Operating system a collector targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    MacOs,
    Linux,
    Windows,
}

/// Normalized connection event shared across collectors and the core pipeline.
///
/// Optional fields stay `None` when a collector cannot provide them. Callers must
/// not invent defaults for missing process metadata or byte counters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: ConnectionEventType,

    pub process_id: u32,
    pub process_name: Option<String>,
    pub process_path: Option<String>,
    pub process_start_time: Option<DateTime<Utc>>,

    pub protocol: Protocol,

    pub local_ip: IpAddress,
    pub local_port: u16,

    pub remote_ip: IpAddress,
    pub remote_port: u16,

    pub direction: Direction,

    pub bytes_sent: Option<u64>,
    pub bytes_received: Option<u64>,
}

impl ConnectionEvent {
    /// Builds a process-instance key when start time is known.
    ///
    /// Returns `None` when `process_start_time` is missing so callers do not
    /// fabricate an identity from PID alone.
    pub fn process_instance_key(&self) -> Option<ProcessInstanceKey> {
        self.process_start_time
            .map(|process_start_time| ProcessInstanceKey::new(self.process_id, process_start_time))
    }
}
