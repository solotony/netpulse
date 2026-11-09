//! SQLite persistence, migrations, and historical queries.

mod entities;
mod error;
mod export;
mod maintenance;
mod models;
mod queries;
mod settings;
mod writes;

use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{ConnectOptions, SqlitePool};

pub use error::DatabaseError;
pub use maintenance::{
    apply_history_retention, clear_connection_history, connection_history_bounds, database_info,
    delete_connections_older_than, DatabaseInfo,
};
pub use models::{
    ConnectionRow, ConnectionUpdate, DomainRow, IpAddressRow, IpDomainRow, NetworkRow,
    NewConnection, NewIpDomain, NewNetwork, NewProcess, ProcessRow, ProcessUpdate,
};
pub use entities::{
    list_domains, list_ip_addresses, list_networks_page, list_processes, DomainListPage,
    DomainListRow, EntityListQuery, IpListPage, IpListRow, NetworkListPage, NetworkListRow,
    ProcessListPage, ProcessListRow,
};
pub use export::{
    export_connection_history, export_connection_history_to_path, describe_export_filter,
    escape_tsv_field, ExportFormat, ExportOptions, ExportSummary, EXPORT_CHUNK_SIZE,
    TSV_COLUMNS,
};
pub use queries::{
    query_connection_history, ConnectionHistoryFilter, ConnectionHistoryPage,
    ConnectionHistoryQuery, ConnectionHistoryRow, ConnectionSortColumn, SortDirection,
    DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
};
pub use settings::{
    ensure_default_settings, get_bool_setting, get_i64_setting, get_setting, set_bool_setting,
    set_i64_setting, set_setting, SETTING_ASN_INFO, SETTING_AUTOSTART, SETTING_BACKGROUND_MODE,
    SETTING_CACHE_TTL_SECONDS, SETTING_COUNTRY_INFO, SETTING_DATE_TIME_FORMAT,
    SETTING_DNS_RESOLUTION, SETTING_HISTORY_RETENTION_DAYS, SETTING_HISTORY_RETENTION_ENABLED,
    SETTING_IP_ENRICHMENT, SETTING_LANGUAGE, SETTING_REVERSE_DNS,
    SETTING_START_MONITORING_AUTOMATICALLY, SETTING_THEME,
};
pub use writes::{
    find_open_connection, get_domain, get_ip_address, get_ip_winning_network, get_network,
    insert_connection, insert_connection_event, insert_connection_for_process, insert_process,
    list_domains_for_ip, list_ip_domains, list_networks, set_ip_winning_network,
    store_winning_ip_network, update_connection, update_connection_exec, update_process,
    upsert_domain, upsert_ip_address, upsert_ip_domain, upsert_network, upsert_process,
};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "database";

/// Embedded schema migrations from `resources/migrations`.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../resources/migrations");

/// Open SQLite pool with WAL, foreign keys, and applied migrations.
#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
    path: Option<PathBuf>,
}

impl Database {
    /// Opens (or creates) a SQLite database at `path`, enables WAL + foreign keys, and migrates.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, DatabaseError> {
        let path_buf = path.as_ref().to_path_buf();
        let options = SqliteConnectOptions::new()
            .filename(&path_buf)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .disable_statement_logging();

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let db = Self {
            pool,
            path: Some(path_buf),
        };
        db.migrate().await?;
        ensure_default_settings(db.pool()).await?;
        Ok(db)
    }

    /// Opens an in-memory database (useful for unit tests). Foreign keys are enabled.
    ///
    /// Note: WAL is a file-journal mode; in-memory databases stay in memory.
    pub async fn open_in_memory() -> Result<Self, DatabaseError> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true)
            .shared_cache(true)
            .disable_statement_logging();

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;

        let db = Self {
            pool,
            path: None,
        };
        db.migrate().await?;
        ensure_default_settings(db.pool()).await?;
        Ok(db)
    }

    /// Runs embedded migrations against the pool.
    pub async fn migrate(&self) -> Result<(), DatabaseError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    /// Shared connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// On-disk path when opened via [`Database::open`]; `None` for in-memory databases.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Snapshot for Settings → Data (path, size, first/last event).
    pub async fn database_info(&self) -> Result<DatabaseInfo, DatabaseError> {
        database_info(self.pool(), self.path()).await
    }

    /// Deletes all connection history (events cascade).
    pub async fn clear_connection_history(&self) -> Result<u64, DatabaseError> {
        clear_connection_history(self.pool()).await
    }

    /// Applies optional automatic history retention from settings.
    pub async fn apply_history_retention(&self) -> Result<u64, DatabaseError> {
        apply_history_retention(self.pool()).await
    }

    /// Reports whether foreign keys are enabled on a pooled connection.
    pub async fn foreign_keys_enabled(&self) -> Result<bool, DatabaseError> {
        let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&self.pool)
            .await?;
        Ok(enabled != 0)
    }

    /// Reports the active journal mode (expected `wal` for on-disk databases).
    pub async fn journal_mode(&self) -> Result<String, DatabaseError> {
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&self.pool)
            .await?;
        Ok(mode.to_lowercase())
    }

    /// Loads a process row by primary key.
    pub async fn get_process(&self, id: i64) -> Result<Option<ProcessRow>, DatabaseError> {
        let row = sqlx::query_as::<_, ProcessRow>(
            r#"
            SELECT id, pid, process_name, executable_path, process_start_time,
                   first_seen_at, last_seen_at
            FROM processes
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Loads a connection row by primary key.
    pub async fn get_connection(&self, id: i64) -> Result<Option<ConnectionRow>, DatabaseError> {
        let row = sqlx::query_as::<_, ConnectionRow>(
            r#"
            SELECT id, process_id, protocol, direction, local_ip, local_port,
                   remote_ip, remote_port, started_at, ended_at,
                   bytes_sent, bytes_received, collector_id
            FROM connections
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Filtered, sorted, paginated connection history (never loads the full table).
    pub async fn query_connection_history(
        &self,
        query: &ConnectionHistoryQuery,
    ) -> Result<ConnectionHistoryPage, DatabaseError> {
        queries::query_connection_history(&self.pool, query).await
    }

    /// Streams matching connection history to `writer` as TXT or TSV (chunked pages).
    pub async fn export_connection_history<W: std::io::Write>(
        &self,
        options: &ExportOptions,
        writer: &mut W,
    ) -> Result<ExportSummary, DatabaseError> {
        export::export_connection_history(&self.pool, options, writer).await
    }

    /// Streams matching connection history to a filesystem path.
    pub async fn export_connection_history_to_path(
        &self,
        options: &ExportOptions,
        path: impl AsRef<std::path::Path>,
    ) -> Result<ExportSummary, DatabaseError> {
        export::export_connection_history_to_path(&self.pool, options, path).await
    }

    /// Paginated process list with connection aggregations.
    pub async fn list_processes(
        &self,
        query: &EntityListQuery,
    ) -> Result<ProcessListPage, DatabaseError> {
        entities::list_processes(&self.pool, query).await
    }

    /// Paginated remote-IP list with enrichment joins.
    pub async fn list_ip_addresses(
        &self,
        query: &EntityListQuery,
    ) -> Result<IpListPage, DatabaseError> {
        entities::list_ip_addresses(&self.pool, query).await
    }

    /// Paginated domain list with association aggregations.
    pub async fn list_domains(
        &self,
        query: &EntityListQuery,
    ) -> Result<DomainListPage, DatabaseError> {
        entities::list_domains(&self.pool, query).await
    }

    /// Paginated network list with IP/connection aggregations.
    pub async fn list_networks_page(
        &self,
        query: &EntityListQuery,
    ) -> Result<NetworkListPage, DatabaseError> {
        entities::list_networks_page(&self.pool, query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    #[test]
    fn crate_name_is_database() {
        assert_eq!(CRATE_NAME, "database");
    }

    #[tokio::test]
    async fn migration_up_succeeds() {
        let db = Database::open_in_memory().await.expect("open in-memory db");
        assert!(db.foreign_keys_enabled().await.expect("pragma foreign_keys"));

        // Core tables from the initial migration must exist.
        for table in [
            "processes",
            "connections",
            "connection_events",
            "ip_addresses",
            "domains",
            "ip_domains",
            "networks",
            "ip_networks",
            "settings",
        ] {
            let name: Option<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1",
            )
            .bind(table)
            .fetch_optional(db.pool())
            .await
            .expect("query sqlite_master");
            assert_eq!(name.as_deref(), Some(table), "missing table {table}");
        }

        assert!(
            !get_bool_setting(db.pool(), SETTING_AUTOSTART, true)
                .await
                .expect("autostart default")
        );
        assert!(
            get_bool_setting(db.pool(), SETTING_BACKGROUND_MODE, false)
                .await
                .expect("background default")
        );
    }

    #[tokio::test]
    async fn settings_round_trip() {
        let db = Database::open_in_memory().await.expect("open");
        set_bool_setting(db.pool(), SETTING_AUTOSTART, true)
            .await
            .expect("set");
        assert!(
            get_bool_setting(db.pool(), SETTING_AUTOSTART, false)
                .await
                .expect("get")
        );
        set_setting(db.pool(), "custom", "value")
            .await
            .expect("set custom");
        assert_eq!(
            get_setting(db.pool(), "custom").await.expect("get custom"),
            Some("value".into())
        );
    }

    #[tokio::test]
    async fn on_disk_opens_with_wal_and_foreign_keys() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("monitor.db");
        let db = Database::open(&path).await.expect("open on-disk db");

        assert!(db.foreign_keys_enabled().await.expect("foreign_keys"));
        assert_eq!(db.journal_mode().await.expect("journal_mode"), "wal");
    }

    #[tokio::test]
    async fn insert_and_update_operations_work() {
        let db = Database::open_in_memory().await.expect("open");

        let start = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 0).unwrap();
        let seen = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 1).unwrap();

        let process_id = insert_process(
            db.pool(),
            &NewProcess {
                pid: 4242,
                process_name: Some("curl".into()),
                executable_path: Some("/usr/bin/curl".into()),
                process_start_time: start,
                first_seen_at: seen,
                last_seen_at: seen,
            },
        )
        .await
        .expect("insert process");

        let later = Utc.with_ymd_and_hms(2024, 3, 15, 10, 5, 0).unwrap();
        update_process(
            db.pool(),
            process_id,
            &ProcessUpdate {
                process_name: Some(Some("curl".into())),
                executable_path: None,
                last_seen_at: Some(later),
            },
        )
        .await
        .expect("update process");

        let process = db.get_process(process_id).await.expect("get").expect("row");
        assert_eq!(process.pid, 4242);
        assert_eq!(process.process_name.as_deref(), Some("curl"));
        assert_eq!(process.last_seen_at, later);

        let conn_start = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 2).unwrap();
        let connection_id = insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 54321,
                remote_ip: "93.184.216.34".into(),
                remote_port: 443,
                started_at: conn_start,
                ended_at: None,
                bytes_sent: None,
                bytes_received: None,
                collector_id: Some("mock".into()),
            },
        )
        .await
        .expect("insert connection");

        let ended = Utc.with_ymd_and_hms(2024, 3, 15, 10, 0, 10).unwrap();
        update_connection(
            db.pool(),
            connection_id,
            &ConnectionUpdate {
                ended_at: Some(Some(ended)),
                bytes_sent: Some(Some(128)),
                bytes_received: Some(Some(4096)),
                collector_id: None,
            },
        )
        .await
        .expect("update connection");

        let connection = db
            .get_connection(connection_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(connection.process_id, process_id);
        assert_eq!(connection.protocol, "tcp");
        assert_eq!(connection.remote_ip, "93.184.216.34");
        assert_eq!(connection.ended_at, Some(ended));
        assert_eq!(connection.bytes_sent, Some(128));
        assert_eq!(connection.bytes_received, Some(4096));
    }

    #[tokio::test]
    async fn reopen_database_reads_same_rows() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("persist.db");

        let start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let seen = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 1).unwrap();

        let (process_id, connection_id) = {
            let db = Database::open(&path).await.expect("first open");
            let process_id = upsert_process(
                db.pool(),
                &NewProcess {
                    pid: 99,
                    process_name: Some("nginx".into()),
                    executable_path: None,
                    process_start_time: start,
                    first_seen_at: seen,
                    last_seen_at: seen,
                },
            )
            .await
            .expect("upsert process");

            let connection_id = insert_connection(
                db.pool(),
                &NewConnection {
                    process_id,
                    protocol: "udp".into(),
                    direction: "inbound".into(),
                    local_ip: "127.0.0.1".into(),
                    local_port: 53,
                    remote_ip: "8.8.8.8".into(),
                    remote_port: 53,
                    started_at: seen,
                    ended_at: None,
                    bytes_sent: Some(40),
                    bytes_received: Some(80),
                    collector_id: None,
                },
            )
            .await
            .expect("insert connection");

            // Same process instance identity must not create a second row.
            let again = upsert_process(
                db.pool(),
                &NewProcess {
                    pid: 99,
                    process_name: Some("nginx".into()),
                    executable_path: Some("/usr/sbin/nginx".into()),
                    process_start_time: start,
                    first_seen_at: seen,
                    last_seen_at: seen + chrono::Duration::seconds(30),
                },
            )
            .await
            .expect("upsert again");
            assert_eq!(again, process_id);

            (process_id, connection_id)
        };

        let db = Database::open(&path).await.expect("reopen");
        assert_eq!(db.journal_mode().await.expect("wal"), "wal");

        let process = db.get_process(process_id).await.expect("get").expect("row");
        assert_eq!(process.pid, 99);
        assert_eq!(process.process_start_time, start);
        assert_eq!(process.executable_path.as_deref(), Some("/usr/sbin/nginx"));
        assert_eq!(
            process.last_seen_at,
            seen + chrono::Duration::seconds(30)
        );

        let connection = db
            .get_connection(connection_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(connection.protocol, "udp");
        assert_eq!(connection.remote_ip, "8.8.8.8");
        assert_eq!(connection.bytes_sent, Some(40));
        assert_eq!(connection.bytes_received, Some(80));

        // PID reuse with a different start time is a distinct process instance.
        let reused_start = Utc.with_ymd_and_hms(2025, 6, 1, 12, 0, 0).unwrap();
        let reused_id = upsert_process(
            db.pool(),
            &NewProcess {
                pid: 99,
                process_name: Some("nginx".into()),
                executable_path: None,
                process_start_time: reused_start,
                first_seen_at: reused_start,
                last_seen_at: reused_start,
            },
        )
        .await
        .expect("pid reuse");
        assert_ne!(reused_id, process_id);
    }

    #[tokio::test]
    async fn upsert_network_updates_fields_and_timestamp() {
        let db = Database::open_in_memory().await.expect("open");
        let t1 = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2024, 6, 2, 12, 0, 0).unwrap();

        let id = upsert_network(
            db.pool(),
            &NewNetwork {
                cidr: "203.0.113.0/24".into(),
                net_range: Some("203.0.113.0 - 203.0.113.255".into()),
                net_name: Some("TEST-NET-3".into()),
                organization: Some("Example Org".into()),
                asn: Some(64500),
                country: Some("US".into()),
                source: Some("local".into()),
                updated_at: t1,
            },
        )
        .await
        .expect("insert network");

        let again = upsert_network(
            db.pool(),
            &NewNetwork {
                cidr: "203.0.113.0/24".into(),
                net_range: None,
                net_name: Some("TEST-NET-3-UPDATED".into()),
                organization: None,
                asn: Some(64501),
                country: None,
                source: Some("local".into()),
                updated_at: t2,
            },
        )
        .await
        .expect("update network");
        assert_eq!(again, id);

        let row = get_network(db.pool(), id).await.expect("get").expect("row");
        assert_eq!(row.cidr, "203.0.113.0/24");
        assert_eq!(row.net_name.as_deref(), Some("TEST-NET-3-UPDATED"));
        assert_eq!(row.organization.as_deref(), Some("Example Org"));
        assert_eq!(row.asn, Some(64501));
        assert_eq!(row.updated_at, t2);
    }

    #[tokio::test]
    async fn upsert_ip_domain_keeps_multiple_domains_per_ip() {
        let db = Database::open_in_memory().await.expect("open");
        let seen = Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap();

        let (ip_id, _) = upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: "203.0.113.10".into(),
                domain: "one.example.com".into(),
                source: "REVERSE_DNS".into(),
                confidence: Some(0.5),
                seen_at: seen,
            },
        )
        .await
        .expect("first domain");

        let (ip_id2, _) = upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: "203.0.113.10".into(),
                domain: "two.example.com".into(),
                source: "REVERSE_DNS".into(),
                confidence: Some(0.5),
                seen_at: seen + chrono::Duration::seconds(1),
            },
        )
        .await
        .expect("second domain");
        assert_eq!(ip_id, ip_id2);

        let domains = list_domains_for_ip(db.pool(), ip_id)
            .await
            .expect("list");
        let names: Vec<_> = domains.iter().map(|d| d.domain.as_str()).collect();
        assert_eq!(names, ["one.example.com", "two.example.com"]);

        let links = list_ip_domains(db.pool(), ip_id).await.expect("links");
        assert_eq!(links.len(), 2);
        assert!(links.iter().all(|l| l.source == "REVERSE_DNS"));
        assert!(links.iter().all(|l| (l.confidence.unwrap() - 0.5).abs() < f64::EPSILON));
    }

    #[tokio::test]
    async fn store_winning_ip_network_keeps_single_association() {
        let db = Database::open_in_memory().await.expect("open");
        let seen = Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap();

        let broad = upsert_network(
            db.pool(),
            &NewNetwork {
                cidr: "198.51.100.0/24".into(),
                net_range: None,
                net_name: Some("broad".into()),
                organization: None,
                asn: None,
                country: None,
                source: Some("local".into()),
                updated_at: seen,
            },
        )
        .await
        .expect("broad");

        let specific = NewNetwork {
            cidr: "198.51.100.0/28".into(),
            net_range: None,
            net_name: Some("specific".into()),
            organization: Some("Winner".into()),
            asn: None,
            country: None,
            source: Some("local".into()),
            updated_at: seen + chrono::Duration::hours(1),
        };

        let (ip_id, network_id) =
            store_winning_ip_network(db.pool(), "198.51.100.10", &specific, seen)
                .await
                .expect("store winner");
        assert_ne!(network_id, broad);

        // Associate the broader prefix first, then replace with the winner.
        set_ip_winning_network(db.pool(), ip_id, broad)
            .await
            .expect("set broad");
        set_ip_winning_network(db.pool(), ip_id, network_id)
            .await
            .expect("set specific");

        let winner = get_ip_winning_network(db.pool(), ip_id)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(winner.id, network_id);
        assert_eq!(winner.cidr, "198.51.100.0/28");
        assert_eq!(winner.net_name.as_deref(), Some("specific"));

        let link_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ip_networks WHERE ip_id = ?1")
                .bind(ip_id)
                .fetch_one(db.pool())
                .await
                .expect("count");
        assert_eq!(link_count, 1);

        let ip_row = get_ip_address(db.pool(), ip_id)
            .await
            .expect("get ip")
            .expect("row");
        assert_eq!(ip_row.ip, "198.51.100.10");
    }
}
