//! Connection event processing pipeline.
//!
//! Accepts [`ConnectionEvent`] values from a collector channel, normalizes them,
//! upserts process instances, opens or refreshes connections, records
//! `connection_events`, and closes sessions on [`ConnectionEventType::ConnectionClosed`].

use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use common::{ConnectionEvent, ConnectionEventType, Direction, Protocol};
use database::{
    find_open_connection, insert_connection, insert_connection_event, update_connection_exec,
    upsert_process, ConnectionUpdate, Database, DatabaseError, NewConnection, NewProcess,
};
use tokio::sync::mpsc;

/// Non-blocking observer invoked with each remote IP seen by the pipeline.
///
/// Used to queue DNS / RDAP enrichment without coupling `core` to those crates.
pub type RemoteIpObserver = Arc<dyn Fn(IpAddr) + Send + Sync>;

/// Outcome of processing a single connection event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessOutcome {
    pub process_id: i64,
    pub connection_id: Option<i64>,
    /// `true` when a new open connection row was inserted.
    pub opened: bool,
    /// `true` when an existing open connection was closed.
    pub closed: bool,
}

/// Failures while normalizing or persisting a connection event.
#[derive(Debug)]
pub enum PipelineError {
    /// Process start time is required for stable process-instance identity.
    MissingProcessStartTime,
    Database(DatabaseError),
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingProcessStartTime => {
                write!(f, "connection event is missing process_start_time")
            }
            Self::Database(err) => write!(f, "pipeline database error: {err}"),
        }
    }
}

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(err) => Some(err),
            Self::MissingProcessStartTime => None,
        }
    }
}

impl From<DatabaseError> for PipelineError {
    fn from(value: DatabaseError) -> Self {
        Self::Database(value)
    }
}

impl From<sqlx::Error> for PipelineError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(DatabaseError::from(value))
    }
}

/// Normalized fields used for persistence (never invents missing process metadata).
#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedEvent {
    timestamp: DateTime<Utc>,
    event_type: ConnectionEventType,
    pid: u32,
    process_name: Option<String>,
    process_path: Option<String>,
    process_start_time: DateTime<Utc>,
    protocol: String,
    local_ip: String,
    local_port: u16,
    remote_ip: String,
    remote_port: u16,
    direction: String,
    bytes_sent: Option<i64>,
    bytes_received: Option<i64>,
}

impl NormalizedEvent {
    fn from_event(event: &ConnectionEvent) -> Result<Self, PipelineError> {
        let process_start_time = event
            .process_start_time
            .ok_or(PipelineError::MissingProcessStartTime)?;

        Ok(Self {
            timestamp: event.timestamp,
            event_type: event.event_type,
            pid: event.process_id,
            // Preserve null; empty strings are treated as absent, not invented paths/names.
            process_name: normalize_optional_string(event.process_name.as_deref()),
            process_path: normalize_optional_string(event.process_path.as_deref()),
            process_start_time,
            protocol: protocol_to_db(event.protocol).to_owned(),
            local_ip: event.local_ip.to_string(),
            local_port: event.local_port,
            remote_ip: event.remote_ip.to_string(),
            remote_port: event.remote_port,
            direction: direction_to_db(event.direction).to_owned(),
            bytes_sent: event.bytes_sent.map(|v| v as i64),
            bytes_received: event.bytes_received.map(|v| v as i64),
        })
    }
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn protocol_to_db(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Tcp => "tcp",
        Protocol::Udp => "udp",
        Protocol::Icmp => "icmp",
        Protocol::Other => "other",
    }
}

fn direction_to_db(direction: Direction) -> &'static str {
    match direction {
        Direction::Outbound => "outbound",
        Direction::Inbound => "inbound",
        Direction::Unknown => "unknown",
    }
}

fn event_type_to_db(event_type: ConnectionEventType) -> &'static str {
    match event_type {
        ConnectionEventType::NewConnection => "new_connection",
        ConnectionEventType::ConnectionClosed => "connection_closed",
        ConnectionEventType::ConnectionStateChanged => "connection_state_changed",
        ConnectionEventType::TrafficUpdate => "traffic_update",
    }
}

/// Processes collector [`ConnectionEvent`] values into SQLite.
#[derive(Clone)]
pub struct EventPipeline {
    db: Database,
    collector_id: Option<String>,
    remote_ip_observer: Option<RemoteIpObserver>,
}

impl fmt::Debug for EventPipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventPipeline")
            .field("collector_id", &self.collector_id)
            .field("has_remote_ip_observer", &self.remote_ip_observer.is_some())
            .finish_non_exhaustive()
    }
}

impl EventPipeline {
    /// Creates a pipeline writing into `db`.
    pub fn new(db: Database) -> Self {
        Self {
            db,
            collector_id: None,
            remote_ip_observer: None,
        }
    }

    /// Attaches an optional collector identifier stored on new connection rows.
    pub fn with_collector_id(mut self, collector_id: impl Into<String>) -> Self {
        self.collector_id = Some(collector_id.into());
        self
    }

    /// Invokes `observer` with each remote IP after a successful persist.
    ///
    /// The observer must not block: enrichment workers enqueue via `try_send`.
    pub fn with_remote_ip_observer(mut self, observer: RemoteIpObserver) -> Self {
        self.remote_ip_observer = Some(observer);
        self
    }

    /// Shared database handle.
    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Normalizes and persists a single event.
    pub async fn process(&self, event: ConnectionEvent) -> Result<ProcessOutcome, PipelineError> {
        let normalized = NormalizedEvent::from_event(&event)?;
        let outcome = self.persist(&normalized).await?;
        if let Some(observer) = &self.remote_ip_observer {
            if let Ok(ip) = normalized.remote_ip.parse::<IpAddr>() {
                observer(ip);
            }
        }
        Ok(outcome)
    }

    /// Drains `event_rx` until the channel closes, processing each event in order.
    pub async fn run(
        &self,
        mut event_rx: mpsc::Receiver<ConnectionEvent>,
    ) -> Result<(), PipelineError> {
        while let Some(event) = event_rx.recv().await {
            self.process(event).await?;
        }
        Ok(())
    }

    async fn persist(&self, event: &NormalizedEvent) -> Result<ProcessOutcome, PipelineError> {
        let mut tx = self.db.pool().begin().await?;

        let process_id = upsert_process(
            &mut *tx,
            &NewProcess {
                pid: event.pid,
                process_name: event.process_name.clone(),
                executable_path: event.process_path.clone(),
                process_start_time: event.process_start_time,
                first_seen_at: event.timestamp,
                last_seen_at: event.timestamp,
            },
        )
        .await?;

        let open = find_open_connection(
            &mut *tx,
            process_id,
            &event.protocol,
            &event.local_ip,
            event.local_port,
            &event.remote_ip,
            event.remote_port,
        )
        .await?;

        let mut opened = false;
        let mut closed = false;

        let connection_id = match event.event_type {
            ConnectionEventType::ConnectionClosed => {
                if let Some(existing) = open {
                    update_connection_exec(
                        &mut *tx,
                        existing.id,
                        &ConnectionUpdate {
                            ended_at: Some(Some(event.timestamp)),
                            bytes_sent: event.bytes_sent.map(Some),
                            bytes_received: event.bytes_received.map(Some),
                            collector_id: None,
                        },
                    )
                    .await?;
                    closed = true;
                    Some(existing.id)
                } else {
                    // Cannot close a connection that was never opened; skip connection write.
                    None
                }
            }
            ConnectionEventType::NewConnection
            | ConnectionEventType::ConnectionStateChanged
            | ConnectionEventType::TrafficUpdate => {
                if let Some(existing) = open {
                    // Dedup: refresh the existing open interval instead of inserting again.
                    update_connection_exec(
                        &mut *tx,
                        existing.id,
                        &ConnectionUpdate {
                            ended_at: None,
                            bytes_sent: event.bytes_sent.map(Some),
                            bytes_received: event.bytes_received.map(Some),
                            collector_id: None,
                        },
                    )
                    .await?;
                    Some(existing.id)
                } else {
                    let id = insert_connection(
                        &mut *tx,
                        &NewConnection {
                            process_id,
                            protocol: event.protocol.clone(),
                            direction: event.direction.clone(),
                            local_ip: event.local_ip.clone(),
                            local_port: event.local_port,
                            remote_ip: event.remote_ip.clone(),
                            remote_port: event.remote_port,
                            started_at: event.timestamp,
                            ended_at: None,
                            bytes_sent: event.bytes_sent,
                            bytes_received: event.bytes_received,
                            collector_id: self.collector_id.clone(),
                        },
                    )
                    .await?;
                    opened = true;
                    Some(id)
                }
            }
        };

        if let Some(connection_id) = connection_id {
            insert_connection_event(
                &mut *tx,
                connection_id,
                event_type_to_db(event.event_type),
                event.timestamp,
                None,
            )
            .await?;
        }

        tx.commit().await?;

        Ok(ProcessOutcome {
            process_id,
            connection_id,
            opened,
            closed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use common::{IpAddress, Protocol};
    use sqlx::Row;

    fn base_event(
        event_type: ConnectionEventType,
        pid: u32,
        process_start_time: DateTime<Utc>,
        timestamp: DateTime<Utc>,
        remote_ip: &str,
        remote_port: u16,
        process_path: Option<&str>,
    ) -> ConnectionEvent {
        ConnectionEvent {
            timestamp,
            event_type,
            process_id: pid,
            process_name: Some("curl".into()),
            process_path: process_path.map(str::to_owned),
            process_start_time: Some(process_start_time),
            protocol: Protocol::Tcp,
            local_ip: IpAddress::parse("192.168.1.10").unwrap(),
            local_port: 54321,
            remote_ip: IpAddress::parse(remote_ip).unwrap(),
            remote_port,
            direction: Direction::Outbound,
            bytes_sent: None,
            bytes_received: None,
        }
    }

    async fn count_connections(db: &Database) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM connections")
            .fetch_one(db.pool())
            .await
            .expect("count connections")
    }

    async fn count_processes(db: &Database) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM processes")
            .fetch_one(db.pool())
            .await
            .expect("count processes")
    }

    async fn count_connection_events(db: &Database) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM connection_events")
            .fetch_one(db.pool())
            .await
            .expect("count connection_events")
    }

    #[tokio::test]
    async fn repeated_observations_do_not_duplicate_open_connection() {
        let db = Database::open_in_memory().await.expect("open");
        let pipeline = EventPipeline::new(db.clone()).with_collector_id("mock");

        let start = Utc.with_ymd_and_hms(2026, 11, 9, 1, 0, 0).unwrap();
        let t0 = Utc.with_ymd_and_hms(2026, 11, 9, 1, 0, 1).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 11, 9, 1, 0, 2).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 11, 9, 1, 0, 3).unwrap();

        let first = pipeline
            .process(base_event(
                ConnectionEventType::NewConnection,
                4242,
                start,
                t0,
                "93.184.216.34",
                443,
                Some("/usr/bin/curl"),
            ))
            .await
            .expect("first");
        assert!(first.opened);
        let connection_id = first.connection_id.expect("connection");

        let second = pipeline
            .process(base_event(
                ConnectionEventType::NewConnection,
                4242,
                start,
                t1,
                "93.184.216.34",
                443,
                Some("/usr/bin/curl"),
            ))
            .await
            .expect("second new");
        assert!(!second.opened);
        assert_eq!(second.connection_id, Some(connection_id));

        let third = pipeline
            .process(base_event(
                ConnectionEventType::TrafficUpdate,
                4242,
                start,
                t2,
                "93.184.216.34",
                443,
                Some("/usr/bin/curl"),
            ))
            .await
            .expect("traffic");
        assert!(!third.opened);
        assert_eq!(third.connection_id, Some(connection_id));

        assert_eq!(count_connections(&db).await, 1);
        assert_eq!(count_processes(&db).await, 1);
        assert_eq!(count_connection_events(&db).await, 3);

        // Distinct remote endpoints stay as separate open rows.
        let other = pipeline
            .process(base_event(
                ConnectionEventType::NewConnection,
                4242,
                start,
                t2,
                "1.1.1.1",
                443,
                Some("/usr/bin/curl"),
            ))
            .await
            .expect("other remote");
        assert!(other.opened);
        assert_ne!(other.connection_id, Some(connection_id));
        assert_eq!(count_connections(&db).await, 2);
    }

    #[tokio::test]
    async fn close_then_reopen_creates_new_connection_row() {
        let db = Database::open_in_memory().await.expect("open");
        let pipeline = EventPipeline::new(db.clone());

        let start = Utc.with_ymd_and_hms(2026, 11, 9, 2, 0, 0).unwrap();
        let t_open = Utc.with_ymd_and_hms(2026, 11, 9, 2, 0, 1).unwrap();
        let t_close = Utc.with_ymd_and_hms(2026, 11, 9, 2, 0, 5).unwrap();
        let t_reopen = Utc.with_ymd_and_hms(2026, 11, 9, 2, 0, 10).unwrap();

        let opened = pipeline
            .process(base_event(
                ConnectionEventType::NewConnection,
                100,
                start,
                t_open,
                "8.8.8.8",
                53,
                Some("/usr/bin/dig"),
            ))
            .await
            .expect("open");
        let first_id = opened.connection_id.expect("id");

        let closed = pipeline
            .process(base_event(
                ConnectionEventType::ConnectionClosed,
                100,
                start,
                t_close,
                "8.8.8.8",
                53,
                Some("/usr/bin/dig"),
            ))
            .await
            .expect("close");
        assert!(closed.closed);
        assert_eq!(closed.connection_id, Some(first_id));

        let row = db.get_connection(first_id).await.expect("get").expect("row");
        assert_eq!(row.ended_at, Some(t_close));

        let reopened = pipeline
            .process(base_event(
                ConnectionEventType::NewConnection,
                100,
                start,
                t_reopen,
                "8.8.8.8",
                53,
                Some("/usr/bin/dig"),
            ))
            .await
            .expect("reopen");
        assert!(reopened.opened);
        let second_id = reopened.connection_id.expect("id");
        assert_ne!(second_id, first_id);

        assert_eq!(count_connections(&db).await, 2);
        assert_eq!(count_processes(&db).await, 1);

        let second = db
            .get_connection(second_id)
            .await
            .expect("get")
            .expect("row");
        assert!(second.ended_at.is_none());
        assert_eq!(second.started_at, t_reopen);
    }

    #[tokio::test]
    async fn pid_reuse_with_different_start_time_creates_new_process_row() {
        let db = Database::open_in_memory().await.expect("open");
        let pipeline = EventPipeline::new(db.clone());

        let start_a = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let start_b = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        let t_a = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 1).unwrap();
        let t_b = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 1).unwrap();

        let first = pipeline
            .process(base_event(
                ConnectionEventType::NewConnection,
                4242,
                start_a,
                t_a,
                "203.0.113.10",
                443,
                Some("/usr/bin/curl"),
            ))
            .await
            .expect("first process");

        let second = pipeline
            .process(base_event(
                ConnectionEventType::NewConnection,
                4242,
                start_b,
                t_b,
                "203.0.113.10",
                443,
                Some("/usr/bin/curl"),
            ))
            .await
            .expect("reused pid");

        assert_ne!(first.process_id, second.process_id);
        assert_eq!(count_processes(&db).await, 2);
        assert_eq!(count_connections(&db).await, 2);

        let process_a = db
            .get_process(first.process_id)
            .await
            .expect("get")
            .expect("row");
        let process_b = db
            .get_process(second.process_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(process_a.pid, 4242);
        assert_eq!(process_b.pid, 4242);
        assert_eq!(process_a.process_start_time, start_a);
        assert_eq!(process_b.process_start_time, start_b);
    }

    #[tokio::test]
    async fn null_process_path_is_stored_as_null() {
        let db = Database::open_in_memory().await.expect("open");
        let pipeline = EventPipeline::new(db.clone());

        let start = Utc.with_ymd_and_hms(2026, 11, 9, 3, 0, 0).unwrap();
        let t0 = Utc.with_ymd_and_hms(2026, 11, 9, 3, 0, 1).unwrap();

        let outcome = pipeline
            .process(base_event(
                ConnectionEventType::NewConnection,
                7,
                start,
                t0,
                "198.51.100.1",
                80,
                None,
            ))
            .await
            .expect("process");

        let process = db
            .get_process(outcome.process_id)
            .await
            .expect("get")
            .expect("row");
        assert!(process.executable_path.is_none());

        let path: Option<String> = sqlx::query_scalar(
            "SELECT executable_path FROM processes WHERE id = ?1",
        )
        .bind(outcome.process_id)
        .fetch_one(db.pool())
        .await
        .expect("scalar");
        assert!(path.is_none());

        // Empty / whitespace path must not be invented into a real path either.
        let t1 = Utc.with_ymd_and_hms(2026, 11, 9, 3, 0, 2).unwrap();
        let mut empty_path = base_event(
            ConnectionEventType::NewConnection,
            8,
            start,
            t1,
            "198.51.100.2",
            80,
            Some("   "),
        );
        empty_path.process_name = Some("idle".into());
        let outcome2 = pipeline.process(empty_path).await.expect("empty path");
        let process2 = db
            .get_process(outcome2.process_id)
            .await
            .expect("get")
            .expect("row");
        assert!(process2.executable_path.is_none());
    }

    #[tokio::test]
    async fn run_consumes_collector_channel() {
        let db = Database::open_in_memory().await.expect("open");
        let pipeline = EventPipeline::new(db.clone());

        let start = Utc.with_ymd_and_hms(2026, 11, 9, 4, 0, 0).unwrap();
        let (tx, rx) = mpsc::channel(4);
        tx.send(base_event(
            ConnectionEventType::NewConnection,
            1,
            start,
            Utc.with_ymd_and_hms(2026, 11, 9, 4, 0, 1).unwrap(),
            "9.9.9.9",
            853,
            Some("/bin/a"),
        ))
        .await
        .expect("send");
        drop(tx);

        pipeline.run(rx).await.expect("run");
        assert_eq!(count_connections(&db).await, 1);

        let open_ended: i64 = sqlx::query(
            "SELECT COUNT(*) as c FROM connections WHERE ended_at IS NULL",
        )
        .fetch_one(db.pool())
        .await
        .expect("query")
        .get::<i64, _>("c");
        assert_eq!(open_ended, 1);
    }
}
