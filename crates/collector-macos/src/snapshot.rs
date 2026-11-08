//! Platform-neutral socket snapshot types used by the diff engine.

use common::{Direction, IpAddress, Protocol};

/// One observed socket / flow at a single poll instant.
///
/// Built either from libproc enumeration or from test fixtures. The diff engine
/// never calls the OS directly.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SocketObservation {
    pub pid: u32,
    pub protocol: Protocol,
    pub local_ip: IpAddress,
    pub local_port: u16,
    pub remote_ip: IpAddress,
    pub remote_port: u16,
    pub direction: Direction,
}

impl SocketObservation {
    /// 5-tuple plus owning PID (PID distinguishes rare local-port collisions).
    pub fn key(&self) -> SocketKey {
        SocketKey {
            pid: self.pid,
            protocol: self.protocol,
            local_ip: self.local_ip,
            local_port: self.local_port,
            remote_ip: self.remote_ip,
            remote_port: self.remote_port,
        }
    }
}

/// Identity used to match the same flow across successive snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SocketKey {
    pub pid: u32,
    pub protocol: Protocol,
    pub local_ip: IpAddress,
    pub local_port: u16,
    pub remote_ip: IpAddress,
    pub remote_port: u16,
}

/// Source of socket snapshots (real libproc or fixtures).
pub trait SocketSnapshotSource: Send + Sync {
    /// Enumerate currently visible sockets / flows.
    fn snapshot(&self) -> Result<Vec<SocketObservation>, SnapshotError>;
}

/// Failure while reading a socket snapshot from the OS (or a fixture source).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotError {
    message: String,
}

impl SnapshotError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "socket snapshot failed: {}", self.message)
    }
}

impl std::error::Error for SnapshotError {}

/// In-memory snapshot source for unit tests (never calls libproc).
#[derive(Debug, Default)]
pub struct FixtureSnapshotSource {
    sockets: std::sync::Mutex<Vec<SocketObservation>>,
}

impl FixtureSnapshotSource {
    pub fn new(sockets: Vec<SocketObservation>) -> Self {
        Self {
            sockets: std::sync::Mutex::new(sockets),
        }
    }

    pub fn set(&self, sockets: Vec<SocketObservation>) {
        *self.sockets.lock().expect("fixture mutex") = sockets;
    }
}

impl SocketSnapshotSource for FixtureSnapshotSource {
    fn snapshot(&self) -> Result<Vec<SocketObservation>, SnapshotError> {
        Ok(self.sockets.lock().expect("fixture mutex").clone())
    }
}
