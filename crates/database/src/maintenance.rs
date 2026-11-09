//! History cleanup, retention, and database summary helpers.

use chrono::{DateTime, Duration, Utc};
use sqlx::SqlitePool;
use std::path::Path;

use crate::settings::{
    get_bool_setting, get_i64_setting, SETTING_HISTORY_RETENTION_DAYS,
    SETTING_HISTORY_RETENTION_ENABLED,
};
use crate::DatabaseError;

/// Summary of the on-disk database for the Settings → Data section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseInfo {
    /// Absolute path to the SQLite file, when known.
    pub path: Option<String>,
    /// File size in bytes (`None` for in-memory or missing files).
    pub size_bytes: Option<u64>,
    /// Earliest connection `started_at`, if any rows exist.
    pub first_recorded_at: Option<DateTime<Utc>>,
    /// Latest connection activity (`ended_at` or `started_at`).
    pub last_recorded_at: Option<DateTime<Utc>>,
    /// Number of connection history rows.
    pub connection_count: i64,
}

/// Deletes all connection history rows (events cascade via FK).
///
/// Enrichment tables (`ip_addresses`, `domains`, `networks`, …) are left intact.
pub async fn clear_connection_history(pool: &SqlitePool) -> Result<u64, DatabaseError> {
    let result = sqlx::query("DELETE FROM connections").execute(pool).await?;
    Ok(result.rows_affected())
}

/// Deletes connections whose `started_at` is strictly older than `cutoff`.
pub async fn delete_connections_older_than(
    pool: &SqlitePool,
    cutoff: DateTime<Utc>,
) -> Result<u64, DatabaseError> {
    let result = sqlx::query("DELETE FROM connections WHERE started_at < ?1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Applies configured retention: when enabled, deletes connections older than N days.
///
/// Returns the number of deleted rows (`0` when retention is disabled).
pub async fn apply_history_retention(pool: &SqlitePool) -> Result<u64, DatabaseError> {
    let enabled = get_bool_setting(pool, SETTING_HISTORY_RETENTION_ENABLED, false).await?;
    if !enabled {
        return Ok(0);
    }
    let days = get_i64_setting(pool, SETTING_HISTORY_RETENTION_DAYS, 30).await?;
    let days = days.max(1);
    let cutoff = Utc::now() - Duration::days(days);
    delete_connections_older_than(pool, cutoff).await
}

/// Loads first/last recorded connection timestamps and row count.
pub async fn connection_history_bounds(
    pool: &SqlitePool,
) -> Result<(Option<DateTime<Utc>>, Option<DateTime<Utc>>, i64), DatabaseError> {
    let row: (Option<DateTime<Utc>>, Option<DateTime<Utc>>, i64) = sqlx::query_as(
        r#"
        SELECT
            MIN(started_at),
            MAX(COALESCE(ended_at, started_at)),
            COUNT(*)
        FROM connections
        "#,
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Builds a [`DatabaseInfo`] snapshot for the given optional on-disk path.
pub async fn database_info(
    pool: &SqlitePool,
    path: Option<&Path>,
) -> Result<DatabaseInfo, DatabaseError> {
    let (first_recorded_at, last_recorded_at, connection_count) =
        connection_history_bounds(pool).await?;
    let path_string = path.map(|p| p.display().to_string());
    let size_bytes = path.and_then(|p| std::fs::metadata(p).ok().map(|m| m.len()));
    Ok(DatabaseInfo {
        path: path_string,
        size_bytes,
        first_recorded_at,
        last_recorded_at,
        connection_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NewConnection, NewProcess};
    use crate::settings::{set_bool_setting, set_i64_setting};
    use crate::writes::{insert_connection, insert_process};
    use crate::Database;
    use chrono::TimeZone;

    async fn seed_connection(db: &Database, started: DateTime<Utc>) -> i64 {
        let process_id = insert_process(
            db.pool(),
            &NewProcess {
                pid: 100,
                process_name: Some("curl".into()),
                executable_path: Some("/usr/bin/curl".into()),
                process_start_time: started,
                first_seen_at: started,
                last_seen_at: started,
            },
        )
        .await
        .expect("process");
        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "127.0.0.1".into(),
                local_port: 40000,
                remote_ip: "1.1.1.1".into(),
                remote_port: 443,
                started_at: started,
                ended_at: None,
                bytes_sent: Some(10),
                bytes_received: Some(20),
                collector_id: Some("test".into()),
            },
        )
        .await
        .expect("connection")
    }

    #[tokio::test]
    async fn clear_history_removes_all_connections() {
        let db = Database::open_in_memory().await.expect("db");
        let now = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
        seed_connection(&db, now).await;
        seed_connection(&db, now - Duration::days(1)).await;

        let deleted = clear_connection_history(db.pool()).await.expect("clear");
        assert_eq!(deleted, 2);

        let (_, _, count) = connection_history_bounds(db.pool()).await.expect("bounds");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn retention_deletes_old_connections_and_keeps_newer() {
        let db = Database::open_in_memory().await.expect("db");
        let now = Utc::now();
        let old_id = seed_connection(&db, now - Duration::days(40)).await;
        let recent_id = seed_connection(&db, now - Duration::days(5)).await;

        set_bool_setting(db.pool(), SETTING_HISTORY_RETENTION_ENABLED, true)
            .await
            .expect("enable retention");
        set_i64_setting(db.pool(), SETTING_HISTORY_RETENTION_DAYS, 30)
            .await
            .expect("set days");

        let deleted = apply_history_retention(db.pool()).await.expect("retain");
        assert_eq!(deleted, 1);

        assert!(
            db.get_connection(old_id).await.expect("get old").is_none(),
            "old connection must be deleted"
        );
        assert!(
            db.get_connection(recent_id)
                .await
                .expect("get recent")
                .is_some(),
            "recent connection must be kept"
        );
    }

    #[tokio::test]
    async fn retention_noop_when_disabled() {
        let db = Database::open_in_memory().await.expect("db");
        let now = Utc::now();
        seed_connection(&db, now - Duration::days(90)).await;

        set_bool_setting(db.pool(), SETTING_HISTORY_RETENTION_ENABLED, false)
            .await
            .expect("disable");
        let deleted = apply_history_retention(db.pool()).await.expect("retain");
        assert_eq!(deleted, 0);

        let (_, _, count) = connection_history_bounds(db.pool()).await.expect("bounds");
        assert_eq!(count, 1);
    }
}
