//! Mock collector → SQLite → paginated connection rows for the desktop UI.

use chrono::{DateTime, TimeZone, Utc};
use collector_api::NetworkCollector;
use common::{ConnectionEvent, ConnectionEventType, Direction, IpAddress, Protocol};
use monitor_core::{EventPipeline, MockCollector};
use database::Database;
use serde::Serialize;
use sqlx::FromRow;
use tokio::sync::mpsc;

/// Default page size for the Connections table.
pub const DEFAULT_PAGE_SIZE: u32 = 50;

/// One row shown in the Connections table (camelCase for the Vue frontend).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTableRow {
    pub id: i64,
    pub started_at: String,
    pub process_name: Option<String>,
    pub pid: i64,
    pub protocol: String,
    pub direction: String,
    pub local_ip: String,
    pub local_port: i64,
    pub remote_ip: String,
    pub remote_port: i64,
    pub domain: Option<String>,
    pub network: Option<String>,
    pub organization: Option<String>,
    pub ended_at: Option<String>,
    pub status: String,
}

/// First page of connections after seeding mock data.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsPage {
    pub rows: Vec<ConnectionTableRow>,
    pub page: u32,
    pub page_size: u32,
    pub total: u32,
}

#[derive(Debug, FromRow)]
struct ConnectionQueryRow {
    id: i64,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    protocol: String,
    direction: String,
    local_ip: String,
    local_port: i64,
    remote_ip: String,
    remote_port: i64,
    pid: i64,
    process_name: Option<String>,
}

impl From<ConnectionQueryRow> for ConnectionTableRow {
    fn from(row: ConnectionQueryRow) -> Self {
        let status = if row.ended_at.is_some() {
            "closed"
        } else {
            "active"
        };
        Self {
            id: row.id,
            started_at: row.started_at.to_rfc3339(),
            process_name: row.process_name,
            pid: row.pid,
            protocol: row.protocol,
            direction: row.direction,
            local_ip: row.local_ip,
            local_port: row.local_port,
            remote_ip: row.remote_ip,
            remote_port: row.remote_port,
            domain: None,
            network: None,
            organization: None,
            ended_at: row.ended_at.map(|t| t.to_rfc3339()),
            status: status.to_owned(),
        }
    }
}

fn scripted_events() -> Vec<ConnectionEvent> {
    let process_start = Utc.with_ymd_and_hms(2026, 11, 9, 0, 0, 0).unwrap();
    let t0 = Utc.with_ymd_and_hms(2026, 11, 9, 0, 1, 0).unwrap();
    let t1 = Utc.with_ymd_and_hms(2026, 11, 9, 0, 1, 5).unwrap();
    let t2 = Utc.with_ymd_and_hms(2026, 11, 9, 0, 1, 10).unwrap();
    let t3 = Utc.with_ymd_and_hms(2026, 11, 9, 0, 1, 20).unwrap();
    let t4 = Utc.with_ymd_and_hms(2026, 11, 9, 0, 1, 30).unwrap();

    vec![
        ConnectionEvent {
            timestamp: t0,
            event_type: ConnectionEventType::NewConnection,
            process_id: 4242,
            process_name: Some("curl".into()),
            process_path: Some("/usr/bin/curl".into()),
            process_start_time: Some(process_start),
            protocol: Protocol::Tcp,
            local_ip: IpAddress::parse("192.168.1.10").unwrap(),
            local_port: 54321,
            remote_ip: IpAddress::parse("93.184.216.34").unwrap(),
            remote_port: 443,
            direction: Direction::Outbound,
            bytes_sent: Some(128),
            bytes_received: Some(4096),
        },
        ConnectionEvent {
            timestamp: t1,
            event_type: ConnectionEventType::NewConnection,
            process_id: 1001,
            process_name: Some("Safari".into()),
            process_path: Some("/Applications/Safari.app/Contents/MacOS/Safari".into()),
            process_start_time: Some(process_start),
            protocol: Protocol::Tcp,
            local_ip: IpAddress::parse("192.168.1.10").unwrap(),
            local_port: 54322,
            remote_ip: IpAddress::parse("1.1.1.1").unwrap(),
            remote_port: 443,
            direction: Direction::Outbound,
            bytes_sent: Some(256),
            bytes_received: Some(8192),
        },
        ConnectionEvent {
            timestamp: t2,
            event_type: ConnectionEventType::NewConnection,
            process_id: 88,
            process_name: Some("mDNSResponder".into()),
            process_path: Some("/usr/sbin/mDNSResponder".into()),
            process_start_time: Some(process_start),
            protocol: Protocol::Udp,
            local_ip: IpAddress::parse("192.168.1.10").unwrap(),
            local_port: 5353,
            remote_ip: IpAddress::parse("8.8.8.8").unwrap(),
            remote_port: 53,
            direction: Direction::Outbound,
            bytes_sent: Some(64),
            bytes_received: Some(128),
        },
        ConnectionEvent {
            timestamp: t3,
            event_type: ConnectionEventType::NewConnection,
            process_id: 501,
            process_name: Some("ssh".into()),
            process_path: Some("/usr/bin/ssh".into()),
            process_start_time: Some(process_start),
            protocol: Protocol::Tcp,
            local_ip: IpAddress::parse("192.168.1.10").unwrap(),
            local_port: 54400,
            remote_ip: IpAddress::parse("203.0.113.50").unwrap(),
            remote_port: 22,
            direction: Direction::Outbound,
            bytes_sent: Some(512),
            bytes_received: Some(1024),
        },
        ConnectionEvent {
            timestamp: t4,
            event_type: ConnectionEventType::ConnectionClosed,
            process_id: 501,
            process_name: Some("ssh".into()),
            process_path: Some("/usr/bin/ssh".into()),
            process_start_time: Some(process_start),
            protocol: Protocol::Tcp,
            local_ip: IpAddress::parse("192.168.1.10").unwrap(),
            local_port: 54400,
            remote_ip: IpAddress::parse("203.0.113.50").unwrap(),
            remote_port: 22,
            direction: Direction::Outbound,
            bytes_sent: Some(1024),
            bytes_received: Some(2048),
        },
    ]
}

/// Runs the scripted [`MockCollector`] through [`EventPipeline`] when the DB is empty,
/// then returns a page of rows.
pub async fn seed_mock_and_list_connections(
    db: &Database,
    page: u32,
    page_size: u32,
) -> Result<ConnectionsPage, String> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 500);

    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connections")
        .fetch_one(db.pool())
        .await
        .map_err(|err| format!("count connections failed: {err}"))?;

    if existing == 0 {
        let pipeline = EventPipeline::new(db.clone()).with_collector_id("mock");
        let (tx, rx) = mpsc::channel(64);
        let mut collector = MockCollector::new(scripted_events());

        collector
            .start(tx)
            .await
            .map_err(|err| format!("mock collector start failed: {err}"))?;

        pipeline
            .run(rx)
            .await
            .map_err(|err| format!("event pipeline failed: {err}"))?;

        // Emit task already finished; stop may race — treat NotStarted as success.
        let _ = collector.stop().await;
    }

    list_connections_page(db, page, page_size).await
}

/// Reads a page of connection rows joined with process identity.
pub async fn list_connections_page(
    db: &Database,
    page: u32,
    page_size: u32,
) -> Result<ConnectionsPage, String> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 500);
    let offset = u64::from((page - 1) * page_size);
    let limit = i64::from(page_size);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM connections")
        .fetch_one(db.pool())
        .await
        .map_err(|err| format!("count connections failed: {err}"))?;

    let rows = sqlx::query_as::<_, ConnectionQueryRow>(
        r#"
        SELECT
            c.id,
            c.started_at,
            c.ended_at,
            c.protocol,
            c.direction,
            c.local_ip,
            c.local_port,
            c.remote_ip,
            c.remote_port,
            p.pid,
            p.process_name
        FROM connections c
        INNER JOIN processes p ON p.id = c.process_id
        ORDER BY c.started_at DESC, c.id DESC
        LIMIT ?1 OFFSET ?2
        "#,
    )
    .bind(limit)
    .bind(offset as i64)
    .fetch_all(db.pool())
    .await
    .map_err(|err| format!("list connections failed: {err}"))?;

    Ok(ConnectionsPage {
        rows: rows.into_iter().map(ConnectionTableRow::from).collect(),
        page,
        page_size,
        total: total as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn seed_mock_and_list_returns_first_page() {
        let db = Database::open_in_memory().await.expect("open db");
        let page = seed_mock_and_list_connections(&db, 1, DEFAULT_PAGE_SIZE)
            .await
            .expect("seed and list");

        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, DEFAULT_PAGE_SIZE);
        assert_eq!(page.total, 4);
        assert_eq!(page.rows.len(), 4);

        let ssh = page
            .rows
            .iter()
            .find(|row| row.process_name.as_deref() == Some("ssh"))
            .expect("ssh row");
        assert_eq!(ssh.status, "closed");
        assert_eq!(ssh.remote_port, 22);
        assert!(ssh.ended_at.is_some());

        let active = page
            .rows
            .iter()
            .filter(|row| row.status == "active")
            .count();
        assert_eq!(active, 3);
    }

    #[tokio::test]
    async fn second_call_skips_seed_when_rows_exist() {
        let db = Database::open_in_memory().await.expect("open db");
        let first = seed_mock_and_list_connections(&db, 1, 10)
            .await
            .expect("first");
        let second = seed_mock_and_list_connections(&db, 1, 10)
            .await
            .expect("second");

        assert_eq!(first.total, 4);
        assert_eq!(second.total, first.total);
        assert_eq!(second.rows, first.rows);
    }
}
