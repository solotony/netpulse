//! Row and write-input types for SQLite persistence.

use chrono::{DateTime, Utc};
use sqlx::FromRow;

/// Input for inserting a process instance.
#[derive(Debug, Clone)]
pub struct NewProcess {
    pub pid: u32,
    pub process_name: Option<String>,
    pub executable_path: Option<String>,
    pub process_start_time: DateTime<Utc>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// Partial update for an existing process row.
///
/// Nested `Option` fields: outer `None` leaves the column unchanged; inner `None` clears it.
#[derive(Debug, Clone, Default)]
pub struct ProcessUpdate {
    pub process_name: Option<Option<String>>,
    pub executable_path: Option<Option<String>>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

/// Stored process row.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ProcessRow {
    pub id: i64,
    pub pid: i64,
    pub process_name: Option<String>,
    pub executable_path: Option<String>,
    pub process_start_time: DateTime<Utc>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// Input for inserting a connection session.
#[derive(Debug, Clone)]
pub struct NewConnection {
    pub process_id: i64,
    pub protocol: String,
    pub direction: String,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_ip: String,
    pub remote_port: u16,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub bytes_sent: Option<i64>,
    pub bytes_received: Option<i64>,
    pub collector_id: Option<String>,
}

/// Partial update for an existing connection row.
#[derive(Debug, Clone, Default)]
pub struct ConnectionUpdate {
    pub ended_at: Option<Option<DateTime<Utc>>>,
    pub bytes_sent: Option<Option<i64>>,
    pub bytes_received: Option<Option<i64>>,
    pub collector_id: Option<Option<String>>,
}

/// Stored connection row.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ConnectionRow {
    pub id: i64,
    pub process_id: i64,
    pub protocol: String,
    pub direction: String,
    pub local_ip: String,
    pub local_port: i64,
    pub remote_ip: String,
    pub remote_port: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub bytes_sent: Option<i64>,
    pub bytes_received: Option<i64>,
    pub collector_id: Option<String>,
}

/// Input for upserting a network registration prefix.
#[derive(Debug, Clone)]
pub struct NewNetwork {
    pub cidr: String,
    pub net_range: Option<String>,
    pub net_name: Option<String>,
    pub organization: Option<String>,
    pub asn: Option<i64>,
    pub country: Option<String>,
    pub source: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Stored network row.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct NetworkRow {
    pub id: i64,
    pub cidr: String,
    pub net_range: Option<String>,
    pub net_name: Option<String>,
    pub organization: Option<String>,
    pub asn: Option<i64>,
    pub country: Option<String>,
    pub source: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Stored IP address row.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct IpAddressRow {
    pub id: i64,
    pub ip: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// Stored domain name row.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct DomainRow {
    pub id: i64,
    pub domain: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// Association between an IP address and a domain name.
#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct IpDomainRow {
    pub ip_id: i64,
    pub domain_id: i64,
    pub source: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub confidence: Option<f64>,
}

/// Input for linking an IP to a domain with discovery metadata.
#[derive(Debug, Clone)]
pub struct NewIpDomain {
    pub ip: String,
    pub domain: String,
    pub source: String,
    pub confidence: Option<f64>,
    pub seen_at: DateTime<Utc>,
}
