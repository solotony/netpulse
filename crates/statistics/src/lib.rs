//! Historical aggregation by process, IP, domain, network, organization, and time.
//!
//! Statistics are tables of SQL aggregations over stored connection rows (including
//! closed history), filtered by an inclusive date range on `started_at`.

use chrono::{DateTime, Utc};
use database::{Database, DatabaseError};
use sqlx::{FromRow, SqlitePool};

/// Crate identity used by unit tests and diagnostics.
pub const CRATE_NAME: &str = "statistics";

/// Inclusive filter on connection `started_at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

impl DateRange {
    /// Returns true when the range cannot contain any timestamps.
    pub fn is_empty(&self) -> bool {
        self.from > self.to
    }
}

/// Process | Connections | Unique IPs | Domains
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ProcessStatsRow {
    pub process: String,
    pub connections: i64,
    pub unique_ips: i64,
    pub domains: i64,
}

/// IP | Connections | Unique processes | Domains
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct IpStatsRow {
    pub ip: String,
    pub connections: i64,
    pub unique_processes: i64,
    pub domains: i64,
}

/// Domain | Connections | Unique IPs | Unique processes
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct DomainStatsRow {
    pub domain: String,
    pub connections: i64,
    pub unique_ips: i64,
    pub unique_processes: i64,
}

/// Network | Connections | Unique IPs | Unique processes
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct NetworkStatsRow {
    pub network: String,
    pub connections: i64,
    pub unique_ips: i64,
    pub unique_processes: i64,
}

/// Organization | Connections | Unique IPs | Unique processes
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct OrganizationStatsRow {
    pub organization: String,
    pub connections: i64,
    pub unique_ips: i64,
    pub unique_processes: i64,
}

/// Calendar bucket granularity for time-based statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeBucket {
    Day,
    Week,
    Month,
}

/// Period | Connections | Unique IPs | Domains
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TimeStatsRow {
    pub period: String,
    pub connections: i64,
    pub unique_ips: i64,
    pub domains: i64,
}

fn period_sql(bucket: TimeBucket) -> &'static str {
    match bucket {
        // UTC date from the ISO-8601 `started_at` text stored by SQLx/chrono.
        TimeBucket::Day => "substr(c.started_at, 1, 10)",
        // Monday-based week number (`%W`) with year prefix, e.g. `2024-W22`.
        TimeBucket::Week => "strftime('%Y-W%W', c.started_at)",
        TimeBucket::Month => "substr(c.started_at, 1, 7)",
    }
}

/// Aggregates connections by process name within `range`.
pub async fn stats_by_process(
    pool: &SqlitePool,
    range: DateRange,
) -> Result<Vec<ProcessStatsRow>, DatabaseError> {
    if range.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query_as::<_, ProcessStatsRow>(
        r#"
        SELECT
            COALESCE(p.process_name, '') AS process,
            COUNT(DISTINCT c.id) AS connections,
            COUNT(DISTINCT c.remote_ip) AS unique_ips,
            COUNT(DISTINCT d.id) AS domains
        FROM connections c
        INNER JOIN processes p ON p.id = c.process_id
        LEFT JOIN ip_addresses ia ON ia.ip = c.remote_ip
        LEFT JOIN ip_domains idm ON idm.ip_id = ia.id
        LEFT JOIN domains d ON d.id = idm.domain_id
        WHERE c.started_at >= ?1 AND c.started_at <= ?2
        GROUP BY COALESCE(p.process_name, '')
        ORDER BY connections DESC, process ASC
        "#,
    )
    .bind(range.from)
    .bind(range.to)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Aggregates connections by remote IP within `range`.
pub async fn stats_by_ip(
    pool: &SqlitePool,
    range: DateRange,
) -> Result<Vec<IpStatsRow>, DatabaseError> {
    if range.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query_as::<_, IpStatsRow>(
        r#"
        SELECT
            c.remote_ip AS ip,
            COUNT(DISTINCT c.id) AS connections,
            COUNT(DISTINCT c.process_id) AS unique_processes,
            COUNT(DISTINCT d.id) AS domains
        FROM connections c
        LEFT JOIN ip_addresses ia ON ia.ip = c.remote_ip
        LEFT JOIN ip_domains idm ON idm.ip_id = ia.id
        LEFT JOIN domains d ON d.id = idm.domain_id
        WHERE c.started_at >= ?1 AND c.started_at <= ?2
        GROUP BY c.remote_ip
        ORDER BY connections DESC, ip ASC
        "#,
    )
    .bind(range.from)
    .bind(range.to)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Aggregates connections by linked domain within `range`.
pub async fn stats_by_domain(
    pool: &SqlitePool,
    range: DateRange,
) -> Result<Vec<DomainStatsRow>, DatabaseError> {
    if range.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query_as::<_, DomainStatsRow>(
        r#"
        SELECT
            d.domain AS domain,
            COUNT(DISTINCT c.id) AS connections,
            COUNT(DISTINCT c.remote_ip) AS unique_ips,
            COUNT(DISTINCT c.process_id) AS unique_processes
        FROM connections c
        INNER JOIN ip_addresses ia ON ia.ip = c.remote_ip
        INNER JOIN ip_domains idm ON idm.ip_id = ia.id
        INNER JOIN domains d ON d.id = idm.domain_id
        WHERE c.started_at >= ?1 AND c.started_at <= ?2
        GROUP BY d.domain
        ORDER BY connections DESC, domain ASC
        "#,
    )
    .bind(range.from)
    .bind(range.to)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Aggregates connections by winning network registration within `range`.
pub async fn stats_by_network(
    pool: &SqlitePool,
    range: DateRange,
) -> Result<Vec<NetworkStatsRow>, DatabaseError> {
    if range.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query_as::<_, NetworkStatsRow>(
        r#"
        SELECT
            COALESCE(n.net_name, n.cidr) AS network,
            COUNT(DISTINCT c.id) AS connections,
            COUNT(DISTINCT c.remote_ip) AS unique_ips,
            COUNT(DISTINCT c.process_id) AS unique_processes
        FROM connections c
        INNER JOIN ip_addresses ia ON ia.ip = c.remote_ip
        INNER JOIN ip_networks inw ON inw.ip_id = ia.id
        INNER JOIN networks n ON n.id = inw.network_id
        WHERE c.started_at >= ?1 AND c.started_at <= ?2
        GROUP BY COALESCE(n.net_name, n.cidr)
        ORDER BY connections DESC, network ASC
        "#,
    )
    .bind(range.from)
    .bind(range.to)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Aggregates connections by winning network organization within `range`.
pub async fn stats_by_organization(
    pool: &SqlitePool,
    range: DateRange,
) -> Result<Vec<OrganizationStatsRow>, DatabaseError> {
    if range.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query_as::<_, OrganizationStatsRow>(
        r#"
        SELECT
            COALESCE(n.organization, '') AS organization,
            COUNT(DISTINCT c.id) AS connections,
            COUNT(DISTINCT c.remote_ip) AS unique_ips,
            COUNT(DISTINCT c.process_id) AS unique_processes
        FROM connections c
        INNER JOIN ip_addresses ia ON ia.ip = c.remote_ip
        INNER JOIN ip_networks inw ON inw.ip_id = ia.id
        INNER JOIN networks n ON n.id = inw.network_id
        WHERE c.started_at >= ?1
          AND c.started_at <= ?2
          AND n.organization IS NOT NULL
          AND n.organization != ''
        GROUP BY n.organization
        ORDER BY connections DESC, organization ASC
        "#,
    )
    .bind(range.from)
    .bind(range.to)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Aggregates connections into day, week, or month buckets within `range`.
pub async fn stats_by_time(
    pool: &SqlitePool,
    range: DateRange,
    bucket: TimeBucket,
) -> Result<Vec<TimeStatsRow>, DatabaseError> {
    if range.is_empty() {
        return Ok(Vec::new());
    }

    let period = period_sql(bucket);
    let sql = format!(
        r#"
        SELECT
            {period} AS period,
            COUNT(DISTINCT c.id) AS connections,
            COUNT(DISTINCT c.remote_ip) AS unique_ips,
            COUNT(DISTINCT d.id) AS domains
        FROM connections c
        LEFT JOIN ip_addresses ia ON ia.ip = c.remote_ip
        LEFT JOIN ip_domains idm ON idm.ip_id = ia.id
        LEFT JOIN domains d ON d.id = idm.domain_id
        WHERE c.started_at >= ?1 AND c.started_at <= ?2
        GROUP BY {period}
        ORDER BY period ASC
        "#
    );

    let rows = sqlx::query_as::<_, TimeStatsRow>(&sql)
        .bind(range.from)
        .bind(range.to)
        .fetch_all(pool)
        .await?;

    Ok(rows)
}

/// Convenience wrappers that take [`Database`].
pub async fn process_stats(
    db: &Database,
    range: DateRange,
) -> Result<Vec<ProcessStatsRow>, DatabaseError> {
    stats_by_process(db.pool(), range).await
}

pub async fn ip_stats(
    db: &Database,
    range: DateRange,
) -> Result<Vec<IpStatsRow>, DatabaseError> {
    stats_by_ip(db.pool(), range).await
}

pub async fn domain_stats(
    db: &Database,
    range: DateRange,
) -> Result<Vec<DomainStatsRow>, DatabaseError> {
    stats_by_domain(db.pool(), range).await
}

pub async fn network_stats(
    db: &Database,
    range: DateRange,
) -> Result<Vec<NetworkStatsRow>, DatabaseError> {
    stats_by_network(db.pool(), range).await
}

pub async fn organization_stats(
    db: &Database,
    range: DateRange,
) -> Result<Vec<OrganizationStatsRow>, DatabaseError> {
    stats_by_organization(db.pool(), range).await
}

pub async fn time_stats(
    db: &Database,
    range: DateRange,
    bucket: TimeBucket,
) -> Result<Vec<TimeStatsRow>, DatabaseError> {
    stats_by_time(db.pool(), range, bucket).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use database::{
        insert_connection, insert_process, store_winning_ip_network, upsert_ip_domain, NewConnection,
        NewIpDomain, NewNetwork, NewProcess,
    };

    fn t(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, hh, mm, ss).unwrap()
    }

    fn full_range() -> DateRange {
        DateRange {
            from: t(2020, 1, 1, 0, 0, 0),
            to: t(2030, 12, 31, 23, 59, 59),
        }
    }

    async fn seed_process(db: &Database, pid: u32, name: &str, start: DateTime<Utc>) -> i64 {
        insert_process(
            db.pool(),
            &NewProcess {
                pid,
                process_name: Some(name.into()),
                executable_path: None,
                process_start_time: start,
                first_seen_at: start,
                last_seen_at: start,
            },
        )
        .await
        .expect("insert process")
    }

    /// Seeds Safari / Chrome / Telegram counts matching the task.md example table.
    async fn seed_browser_messenger_fixture(db: &Database) {
        let seen = t(2024, 6, 1, 12, 0, 0);
        let safari = seed_process(db, 100, "Safari", seen).await;
        let chrome = seed_process(db, 200, "Chrome", seen).await;
        let telegram = seed_process(db, 300, "Telegram", seen).await;

        // IPs + domains first so connection joins resolve.
        seed_process_ips_and_domains(db, "10.10.0", 87, 65, seen).await;
        seed_process_ips_and_domains(db, "10.20.0", 152, 130, seen).await;
        seed_process_ips_and_domains(db, "10.30.0", 24, 12, seen).await;

        seed_connections_for_process(db, safari, "10.10.0", 87, 1240, seen).await;
        seed_connections_for_process(db, chrome, "10.20.0", 152, 3410, seen).await;
        seed_connections_for_process(db, telegram, "10.30.0", 24, 520, seen).await;
    }

    async fn seed_process_ips_and_domains(
        db: &Database,
        prefix: &str,
        ip_count: i64,
        domain_count: i64,
        seen: DateTime<Utc>,
    ) {
        assert!(domain_count <= ip_count);
        sqlx::query(
            r#"
            WITH RECURSIVE seq(n) AS (
                SELECT 1
                UNION ALL
                SELECT n + 1 FROM seq WHERE n < ?1
            )
            INSERT INTO ip_addresses (ip, first_seen_at, last_seen_at)
            SELECT printf('%s.%d', ?2, n), ?3, ?3 FROM seq
            "#,
        )
        .bind(ip_count)
        .bind(prefix)
        .bind(seen)
        .execute(db.pool())
        .await
        .expect("insert ips");

        sqlx::query(
            r#"
            WITH RECURSIVE seq(n) AS (
                SELECT 1
                UNION ALL
                SELECT n + 1 FROM seq WHERE n < ?1
            )
            INSERT INTO domains (domain, first_seen_at, last_seen_at)
            SELECT printf('host-%s-%d.example', ?2, n), ?3, ?3 FROM seq
            "#,
        )
        .bind(domain_count)
        .bind(prefix.replace('.', "-"))
        .bind(seen)
        .execute(db.pool())
        .await
        .expect("insert domains");

        sqlx::query(
            r#"
            WITH RECURSIVE seq(n) AS (
                SELECT 1
                UNION ALL
                SELECT n + 1 FROM seq WHERE n < ?1
            )
            INSERT INTO ip_domains (ip_id, domain_id, source, first_seen_at, last_seen_at, confidence)
            SELECT ia.id, d.id, 'REVERSE_DNS', ?3, ?3, 0.9
            FROM seq
            INNER JOIN ip_addresses ia ON ia.ip = printf('%s.%d', ?2, n)
            INNER JOIN domains d ON d.domain = printf('host-%s-%d.example', ?4, n)
            "#,
        )
        .bind(domain_count)
        .bind(prefix)
        .bind(seen)
        .bind(prefix.replace('.', "-"))
        .execute(db.pool())
        .await
        .expect("link ip domains");
    }

    async fn seed_connections_for_process(
        db: &Database,
        process_id: i64,
        prefix: &str,
        ip_count: i64,
        connection_count: i64,
        started_at: DateTime<Utc>,
    ) {
        sqlx::query(
            r#"
            WITH RECURSIVE seq(n) AS (
                SELECT 1
                UNION ALL
                SELECT n + 1 FROM seq WHERE n < ?1
            )
            INSERT INTO connections (
                process_id, protocol, direction, local_ip, local_port,
                remote_ip, remote_port, started_at, ended_at,
                bytes_sent, bytes_received, collector_id
            )
            SELECT
                ?2,
                'tcp',
                'outbound',
                '192.168.1.10',
                40000 + (n % 1000),
                printf('%s.%d', ?3, ((n - 1) % ?4) + 1),
                443,
                ?5,
                CASE WHEN n % 2 = 0 THEN ?5 ELSE NULL END,
                10,
                20,
                'test'
            FROM seq
            "#,
        )
        .bind(connection_count)
        .bind(process_id)
        .bind(prefix)
        .bind(ip_count)
        .bind(started_at)
        .execute(db.pool())
        .await
        .expect("insert connections");
    }

    #[test]
    fn crate_name_is_statistics() {
        assert_eq!(CRATE_NAME, "statistics");
    }

    #[tokio::test]
    async fn process_stats_match_safari_chrome_telegram_example() {
        let db = Database::open_in_memory().await.expect("open");
        seed_browser_messenger_fixture(&db).await;

        let rows = process_stats(&db, full_range()).await.expect("stats");
        assert_eq!(rows.len(), 3);

        assert_eq!(rows[0].process, "Chrome");
        assert_eq!(rows[0].connections, 3410);
        assert_eq!(rows[0].unique_ips, 152);
        assert_eq!(rows[0].domains, 130);

        assert_eq!(rows[1].process, "Safari");
        assert_eq!(rows[1].connections, 1240);
        assert_eq!(rows[1].unique_ips, 87);
        assert_eq!(rows[1].domains, 65);

        assert_eq!(rows[2].process, "Telegram");
        assert_eq!(rows[2].connections, 520);
        assert_eq!(rows[2].unique_ips, 24);
        assert_eq!(rows[2].domains, 12);
    }

    #[tokio::test]
    async fn empty_date_range_returns_no_rows() {
        let db = Database::open_in_memory().await.expect("open");
        seed_browser_messenger_fixture(&db).await;

        let inverted = DateRange {
            from: t(2025, 1, 2, 0, 0, 0),
            to: t(2025, 1, 1, 0, 0, 0),
        };
        assert!(inverted.is_empty());
        assert!(process_stats(&db, inverted).await.expect("process").is_empty());
        assert!(ip_stats(&db, inverted).await.expect("ip").is_empty());
        assert!(domain_stats(&db, inverted).await.expect("domain").is_empty());
        assert!(network_stats(&db, inverted).await.expect("network").is_empty());
        assert!(
            organization_stats(&db, inverted)
                .await
                .expect("org")
                .is_empty()
        );
        assert!(
            time_stats(&db, inverted, TimeBucket::Day)
                .await
                .expect("day")
                .is_empty()
        );

        // Valid bounds that simply contain no stored connections.
        let gap = DateRange {
            from: t(2010, 1, 1, 0, 0, 0),
            to: t(2010, 12, 31, 23, 59, 59),
        };
        assert!(!gap.is_empty());
        assert!(process_stats(&db, gap).await.expect("gap").is_empty());
        assert!(
            time_stats(&db, gap, TimeBucket::Month)
                .await
                .expect("month gap")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn week_and_month_buckets() {
        let db = Database::open_in_memory().await.expect("open");
        let process_id = seed_process(&db, 42, "curl", t(2024, 1, 1, 0, 0, 0)).await;

        // Same ISO week (2024-W22): Mon 2024-05-27 .. Sun 2024-06-02
        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 50001,
                remote_ip: "203.0.113.1".into(),
                remote_port: 443,
                started_at: t(2024, 5, 28, 10, 0, 0),
                ended_at: Some(t(2024, 5, 28, 10, 0, 5)),
                bytes_sent: Some(1),
                bytes_received: Some(2),
                collector_id: None,
            },
        )
        .await
        .expect("week a");

        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 50002,
                remote_ip: "203.0.113.2".into(),
                remote_port: 443,
                started_at: t(2024, 5, 30, 10, 0, 0),
                ended_at: None,
                bytes_sent: None,
                bytes_received: None,
                collector_id: None,
            },
        )
        .await
        .expect("week b");

        // Still week 22 (Mon 2024-05-27 .. Sun 2024-06-02), same month.
        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 50003,
                remote_ip: "203.0.113.3".into(),
                remote_port: 443,
                started_at: t(2024, 5, 31, 10, 0, 0),
                ended_at: None,
                bytes_sent: None,
                bytes_received: None,
                collector_id: None,
            },
        )
        .await
        .expect("still week 22");

        // Week 23 / June.
        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 50004,
                remote_ip: "203.0.113.4".into(),
                remote_port: 443,
                started_at: t(2024, 6, 3, 10, 0, 0),
                ended_at: None,
                bytes_sent: None,
                bytes_received: None,
                collector_id: None,
            },
        )
        .await
        .expect("week 23");

        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 50005,
                remote_ip: "198.51.100.1".into(),
                remote_port: 443,
                started_at: t(2024, 7, 1, 10, 0, 0),
                ended_at: Some(t(2024, 7, 1, 10, 1, 0)),
                bytes_sent: Some(8),
                bytes_received: Some(16),
                collector_id: None,
            },
        )
        .await
        .expect("july");

        upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: "203.0.113.1".into(),
                domain: "one.example.com".into(),
                source: "REVERSE_DNS".into(),
                confidence: Some(0.8),
                seen_at: t(2024, 5, 28, 10, 0, 0),
            },
        )
        .await
        .expect("domain");

        let weeks = time_stats(&db, full_range(), TimeBucket::Week)
            .await
            .expect("weeks");
        let week_map: Vec<_> = weeks
            .iter()
            .map(|r| (r.period.as_str(), r.connections, r.unique_ips, r.domains))
            .collect();

        assert!(
            week_map.iter().any(|(p, c, ips, d)| {
                *p == "2024-W22" && *c == 3 && *ips == 3 && *d == 1
            }),
            "expected 2024-W22 with 3 connections, got {week_map:?}"
        );
        assert!(
            week_map
                .iter()
                .any(|(p, c, ips, _)| *p == "2024-W23" && *c == 1 && *ips == 1),
            "expected 2024-W23, got {week_map:?}"
        );
        assert!(
            week_map
                .iter()
                .any(|(p, c, ips, _)| *p == "2024-W27" && *c == 1 && *ips == 1),
            "expected 2024-W27 (July 1), got {week_map:?}"
        );

        let months = time_stats(&db, full_range(), TimeBucket::Month)
            .await
            .expect("months");
        assert_eq!(
            months
                .iter()
                .map(|r| (r.period.as_str(), r.connections))
                .collect::<Vec<_>>(),
            vec![("2024-05", 3), ("2024-06", 1), ("2024-07", 1)]
        );

        let days = time_stats(&db, full_range(), TimeBucket::Day)
            .await
            .expect("days");
        assert_eq!(days.len(), 5);
        assert_eq!(days[0].period, "2024-05-28");
        assert_eq!(days[0].connections, 1);
        assert_eq!(days[0].domains, 1);
    }

    #[tokio::test]
    async fn ip_domain_network_organization_groupings() {
        let db = Database::open_in_memory().await.expect("open");
        let safari = seed_process(&db, 1, "Safari", t(2024, 1, 1, 0, 0, 0)).await;
        let chrome = seed_process(&db, 2, "Chrome", t(2024, 1, 1, 0, 0, 1)).await;

        for (process_id, ip, port, started) in [
            (safari, "203.0.113.10", 443u16, t(2024, 6, 1, 10, 0, 0)),
            (safari, "203.0.113.10", 80, t(2024, 6, 1, 11, 0, 0)),
            (chrome, "203.0.113.10", 443, t(2024, 6, 2, 9, 0, 0)),
            (chrome, "198.51.100.1", 443, t(2024, 6, 3, 8, 0, 0)),
        ] {
            insert_connection(
                db.pool(),
                &NewConnection {
                    process_id,
                    protocol: "tcp".into(),
                    direction: "outbound".into(),
                    local_ip: "192.168.1.10".into(),
                    local_port: 40_000 + port,
                    remote_ip: ip.into(),
                    remote_port: port,
                    started_at: started,
                    ended_at: Some(started + chrono::Duration::seconds(5)),
                    bytes_sent: Some(10),
                    bytes_received: Some(20),
                    collector_id: Some("test".into()),
                },
            )
            .await
            .expect("connection");
        }

        upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: "203.0.113.10".into(),
                domain: "www.example.com".into(),
                source: "REVERSE_DNS".into(),
                confidence: Some(0.9),
                seen_at: t(2024, 6, 1, 10, 0, 0),
            },
        )
        .await
        .expect("domain a");
        upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: "203.0.113.10".into(),
                domain: "cdn.example.com".into(),
                source: "REVERSE_DNS".into(),
                confidence: Some(0.7),
                seen_at: t(2024, 6, 1, 10, 0, 1),
            },
        )
        .await
        .expect("domain b");

        store_winning_ip_network(
            db.pool(),
            "203.0.113.10",
            &NewNetwork {
                cidr: "203.0.113.0/24".into(),
                net_range: None,
                net_name: Some("TEST-NET-3".into()),
                organization: Some("Example Org".into()),
                asn: Some(64500),
                country: Some("US".into()),
                source: Some("local".into()),
                updated_at: t(2024, 6, 1, 10, 0, 0),
            },
            t(2024, 6, 1, 10, 0, 0),
        )
        .await
        .expect("network");

        let ips = ip_stats(&db, full_range()).await.expect("ips");
        assert_eq!(ips.len(), 2);
        assert_eq!(ips[0].ip, "203.0.113.10");
        assert_eq!(ips[0].connections, 3);
        assert_eq!(ips[0].unique_processes, 2);
        assert_eq!(ips[0].domains, 2);
        assert_eq!(ips[1].ip, "198.51.100.1");
        assert_eq!(ips[1].connections, 1);
        assert_eq!(ips[1].domains, 0);

        let domains = domain_stats(&db, full_range()).await.expect("domains");
        assert_eq!(domains.len(), 2);
        assert!(domains.iter().all(|d| d.connections == 3));
        assert!(domains.iter().all(|d| d.unique_ips == 1));
        assert!(domains.iter().all(|d| d.unique_processes == 2));

        let networks = network_stats(&db, full_range()).await.expect("networks");
        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].network, "TEST-NET-3");
        assert_eq!(networks[0].connections, 3);
        assert_eq!(networks[0].unique_ips, 1);
        assert_eq!(networks[0].unique_processes, 2);

        let orgs = organization_stats(&db, full_range()).await.expect("orgs");
        assert_eq!(orgs.len(), 1);
        assert_eq!(orgs[0].organization, "Example Org");
        assert_eq!(orgs[0].connections, 3);
        assert_eq!(orgs[0].unique_ips, 1);
        assert_eq!(orgs[0].unique_processes, 2);

        // Date range clips Chrome's later connections.
        let early = DateRange {
            from: t(2024, 6, 1, 0, 0, 0),
            to: t(2024, 6, 1, 23, 59, 59),
        };
        let clipped = process_stats(&db, early).await.expect("clipped");
        assert_eq!(clipped.len(), 1);
        assert_eq!(clipped[0].process, "Safari");
        assert_eq!(clipped[0].connections, 2);
        assert_eq!(clipped[0].unique_ips, 1);
        assert_eq!(clipped[0].domains, 2);
    }
}
