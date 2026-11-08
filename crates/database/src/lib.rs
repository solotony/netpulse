//! SQLite persistence, migrations, and historical queries.

mod error;
mod models;
mod writes;

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{ConnectOptions, SqlitePool};

pub use error::DatabaseError;
pub use models::{
    ConnectionRow, ConnectionUpdate, NewConnection, NewProcess, ProcessRow, ProcessUpdate,
};
pub use writes::{
    insert_connection, insert_connection_for_process, insert_process, update_connection,
    update_process, upsert_process,
};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "database";

/// Embedded schema migrations from `resources/migrations`.
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../resources/migrations");

/// Open SQLite pool with WAL, foreign keys, and applied migrations.
#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Opens (or creates) a SQLite database at `path`, enables WAL + foreign keys, and migrates.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, DatabaseError> {
        let options = SqliteConnectOptions::new()
            .filename(path.as_ref())
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .disable_statement_logging();

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        let db = Self { pool };
        db.migrate().await?;
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

        let db = Self { pool };
        db.migrate().await?;
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
}
