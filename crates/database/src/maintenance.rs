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

/// Deletes connection history and the process rows that existed only to explain it.
///
/// `connection_events` cascade from `connections`. Enrichment tables
/// (`ip_addresses`, `domains`, `networks`, and their links) are left intact, so
/// Domains and Networks stay. The IP Addresses list is built from connections
/// and is therefore empty after this call.
pub async fn clear_connection_history(pool: &SqlitePool) -> Result<u64, DatabaseError> {
    let mut tx = pool.begin().await?;
    let deleted = sqlx::query("DELETE FROM connections")
        .execute(&mut *tx)
        .await?
        .rows_affected();
    sqlx::query("DELETE FROM processes")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(deleted)
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
    use crate::entities::{
        list_domains, list_ip_addresses, list_networks_page, list_processes, EntityListQuery,
    };
    use crate::models::{NewConnection, NewNetwork, NewProcess};
    use crate::settings::{set_bool_setting, set_i64_setting};
    use crate::writes::{
        insert_connection, insert_process, store_winning_ip_network, upsert_domain,
    };
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
        let processes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM processes")
            .fetch_one(db.pool())
            .await
            .expect("count processes");
        assert_eq!(processes, 0, "process rows have no meaning without history");
    }

    #[tokio::test]
    async fn clear_history_keeps_domains_and_networks_and_enrichment_ips() {
        let db = Database::open_in_memory().await.expect("db");
        let now = Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap();
        seed_connection(&db, now).await;
        upsert_domain(db.pool(), "example.com", now)
            .await
            .expect("domain");
        store_winning_ip_network(
            db.pool(),
            "1.1.1.1",
            &NewNetwork {
                cidr: "1.1.1.0/24".into(),
                net_range: None,
                net_name: Some("APNIC-LABS".into()),
                organization: Some("APNIC".into()),
                asn: Some(13335),
                country: Some("AU".into()),
                source: Some("rdap".into()),
                updated_at: now,
            },
            now,
        )
        .await
        .expect("network");

        clear_connection_history(db.pool()).await.expect("clear");

        let query = EntityListQuery::default();
        let processes = list_processes(db.pool(), &query).await.expect("processes");
        let ips = list_ip_addresses(db.pool(), &query).await.expect("ips");
        let domains = list_domains(db.pool(), &query).await.expect("domains");
        let networks = list_networks_page(db.pool(), &query)
            .await
            .expect("networks");

        assert_eq!(processes.total, 0);
        assert_eq!(
            ips.total, 1,
            "enrichment IPs remain visible after clearing connection history"
        );
        assert_eq!(ips.rows[0].ip, "1.1.1.1");
        assert_eq!(ips.rows[0].connection_count, 0);
        assert_eq!(domains.total, 1);
        assert_eq!(domains.rows[0].domain, "example.com");
        assert_eq!(networks.total, 1);
        assert_eq!(networks.rows[0].cidr, "1.1.1.0/24");

        let ip_cache: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ip_addresses")
            .fetch_one(db.pool())
            .await
            .expect("count ip cache");
        assert_eq!(
            ip_cache, 1,
            "enrichment cache stays so domain and network links remain"
        );
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
