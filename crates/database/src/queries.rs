//! Paginated connection-history queries with filters, sort, and enrichment joins.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, QueryBuilder, SqlitePool};

use crate::error::DatabaseError;

/// Default page size for UI-facing history lists.
pub const DEFAULT_PAGE_SIZE: u32 = 50;

/// Upper bound for a single page to keep memory use bounded.
pub const MAX_PAGE_SIZE: u32 = 500;

/// One connection history row with joined process and optional enrichment fields.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ConnectionHistoryRow {
    pub id: i64,
    pub process_id: i64,
    pub process_name: Option<String>,
    pub pid: i64,
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
    pub domain: Option<String>,
    pub network_cidr: Option<String>,
    pub network_name: Option<String>,
    pub organization: Option<String>,
}

/// Filter criteria for connection history. All fields are optional and AND-combined.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectionHistoryFilter {
    /// Free-text search across process, IPs, ports, protocol, domain, network, and org.
    pub text: Option<String>,
    /// Exact process primary key.
    pub process_id: Option<i64>,
    /// Case-insensitive substring match on process name.
    pub process_name: Option<String>,
    /// Case-insensitive substring match on remote IP.
    pub remote_ip: Option<String>,
    /// Case-insensitive substring match on any domain linked to the remote IP.
    pub domain: Option<String>,
    /// Case-insensitive substring match on winning network CIDR or net name.
    pub network: Option<String>,
    /// Exact protocol match (case-insensitive), e.g. `tcp`.
    pub protocol: Option<String>,
    /// Inclusive lower bound on `started_at`.
    pub started_from: Option<DateTime<Utc>>,
    /// Inclusive upper bound on `started_at`.
    pub started_to: Option<DateTime<Utc>>,
    /// When true, only rows with `ended_at IS NULL`.
    pub active_only: bool,
}

/// Sortable connection-history columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionSortColumn {
    #[default]
    StartedAt,
    EndedAt,
    ProcessName,
    Pid,
    Protocol,
    Direction,
    LocalIp,
    LocalPort,
    RemoteIp,
    RemotePort,
    Domain,
    Network,
    Organization,
    Id,
}

/// Ascending or descending sort order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

/// Paginated history query. Defaults to newest-first pages of [`DEFAULT_PAGE_SIZE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionHistoryQuery {
    pub filter: ConnectionHistoryFilter,
    pub sort_by: ConnectionSortColumn,
    pub sort_dir: SortDirection,
    pub limit: u32,
    pub offset: u64,
}

impl Default for ConnectionHistoryQuery {
    fn default() -> Self {
        Self {
            filter: ConnectionHistoryFilter::default(),
            sort_by: ConnectionSortColumn::StartedAt,
            sort_dir: SortDirection::Desc,
            limit: DEFAULT_PAGE_SIZE,
            offset: 0,
        }
    }
}

/// One page of connection history plus total matching row count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionHistoryPage {
    pub rows: Vec<ConnectionHistoryRow>,
    pub total: u64,
    pub limit: u32,
    pub offset: u64,
}

/// Escapes `\`, `%`, and `_` for use with `LIKE … ESCAPE '\'`.
fn escape_like(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for ch in term.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out
}

fn contains_pattern(term: &str) -> String {
    format!("%{}%", escape_like(term.trim()))
}

fn sort_sql(column: ConnectionSortColumn) -> &'static str {
    match column {
        ConnectionSortColumn::StartedAt => "c.started_at",
        ConnectionSortColumn::EndedAt => "c.ended_at",
        ConnectionSortColumn::ProcessName => "p.process_name",
        ConnectionSortColumn::Pid => "p.pid",
        ConnectionSortColumn::Protocol => "c.protocol",
        ConnectionSortColumn::Direction => "c.direction",
        ConnectionSortColumn::LocalIp => "c.local_ip",
        ConnectionSortColumn::LocalPort => "c.local_port",
        ConnectionSortColumn::RemoteIp => "c.remote_ip",
        ConnectionSortColumn::RemotePort => "c.remote_port",
        ConnectionSortColumn::Domain => "domain",
        ConnectionSortColumn::Network => "network_cidr",
        ConnectionSortColumn::Organization => "organization",
        ConnectionSortColumn::Id => "c.id",
    }
}

fn sort_dir_sql(dir: SortDirection) -> &'static str {
    match dir {
        SortDirection::Asc => "ASC",
        SortDirection::Desc => "DESC",
    }
}

/// Correlated enrichment subqueries shared by SELECT and ORDER BY aliases.
const DOMAIN_SUBQUERY: &str = r#"
    (
        SELECT d.domain
        FROM ip_addresses ia
        INNER JOIN ip_domains idm ON idm.ip_id = ia.id
        INNER JOIN domains d ON d.id = idm.domain_id
        WHERE ia.ip = c.remote_ip
        ORDER BY idm.last_seen_at DESC, d.domain ASC
        LIMIT 1
    )
"#;

const NETWORK_CIDR_SUBQUERY: &str = r#"
    (
        SELECT n.cidr
        FROM ip_addresses ia
        INNER JOIN ip_networks inw ON inw.ip_id = ia.id
        INNER JOIN networks n ON n.id = inw.network_id
        WHERE ia.ip = c.remote_ip
        ORDER BY n.updated_at DESC, n.id ASC
        LIMIT 1
    )
"#;

const NETWORK_NAME_SUBQUERY: &str = r#"
    (
        SELECT n.net_name
        FROM ip_addresses ia
        INNER JOIN ip_networks inw ON inw.ip_id = ia.id
        INNER JOIN networks n ON n.id = inw.network_id
        WHERE ia.ip = c.remote_ip
        ORDER BY n.updated_at DESC, n.id ASC
        LIMIT 1
    )
"#;

const ORGANIZATION_SUBQUERY: &str = r#"
    (
        SELECT n.organization
        FROM ip_addresses ia
        INNER JOIN ip_networks inw ON inw.ip_id = ia.id
        INNER JOIN networks n ON n.id = inw.network_id
        WHERE ia.ip = c.remote_ip
        ORDER BY n.updated_at DESC, n.id ASC
        LIMIT 1
    )
"#;

fn push_filters<'a>(qb: &mut QueryBuilder<'a, sqlx::Sqlite>, filter: &'a ConnectionHistoryFilter) {
    if let Some(ref text) = filter.text {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let pattern = contains_pattern(trimmed);
            qb.push(" AND (");
            qb.push("LOWER(IFNULL(p.process_name, '')) LIKE LOWER(");
            qb.push_bind(pattern.clone());
            qb.push(") ESCAPE '\\'");
            qb.push(" OR LOWER(c.local_ip) LIKE LOWER(");
            qb.push_bind(pattern.clone());
            qb.push(") ESCAPE '\\'");
            qb.push(" OR LOWER(c.remote_ip) LIKE LOWER(");
            qb.push_bind(pattern.clone());
            qb.push(") ESCAPE '\\'");
            qb.push(" OR CAST(c.local_port AS TEXT) LIKE ");
            qb.push_bind(pattern.clone());
            qb.push(" ESCAPE '\\'");
            qb.push(" OR CAST(c.remote_port AS TEXT) LIKE ");
            qb.push_bind(pattern.clone());
            qb.push(" ESCAPE '\\'");
            qb.push(" OR LOWER(c.protocol) LIKE LOWER(");
            qb.push_bind(pattern.clone());
            qb.push(") ESCAPE '\\'");
            qb.push(" OR LOWER(c.direction) LIKE LOWER(");
            qb.push_bind(pattern.clone());
            qb.push(") ESCAPE '\\'");
            qb.push(" OR EXISTS (");
            qb.push(
                r#"
                SELECT 1 FROM ip_addresses ia
                INNER JOIN ip_domains idm ON idm.ip_id = ia.id
                INNER JOIN domains d ON d.id = idm.domain_id
                WHERE ia.ip = c.remote_ip AND LOWER(d.domain) LIKE LOWER(
                "#,
            );
            qb.push_bind(pattern.clone());
            qb.push(") ESCAPE '\\')");
            qb.push(" OR EXISTS (");
            qb.push(
                r#"
                SELECT 1 FROM ip_addresses ia
                INNER JOIN ip_networks inw ON inw.ip_id = ia.id
                INNER JOIN networks n ON n.id = inw.network_id
                WHERE ia.ip = c.remote_ip AND (
                    LOWER(n.cidr) LIKE LOWER(
                "#,
            );
            qb.push_bind(pattern.clone());
            qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.net_name, '')) LIKE LOWER(");
            qb.push_bind(pattern.clone());
            qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.organization, '')) LIKE LOWER(");
            qb.push_bind(pattern);
            qb.push(") ESCAPE '\\'))");
            qb.push(")");
        }
    }

    if let Some(process_id) = filter.process_id {
        qb.push(" AND c.process_id = ");
        qb.push_bind(process_id);
    }

    if let Some(ref name) = filter.process_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            let pattern = contains_pattern(trimmed);
            qb.push(" AND LOWER(IFNULL(p.process_name, '')) LIKE LOWER(");
            qb.push_bind(pattern);
            qb.push(") ESCAPE '\\'");
        }
    }

    if let Some(ref ip) = filter.remote_ip {
        let trimmed = ip.trim();
        if !trimmed.is_empty() {
            let pattern = contains_pattern(trimmed);
            qb.push(" AND LOWER(c.remote_ip) LIKE LOWER(");
            qb.push_bind(pattern);
            qb.push(") ESCAPE '\\'");
        }
    }

    if let Some(ref domain) = filter.domain {
        let trimmed = domain.trim();
        if !trimmed.is_empty() {
            let pattern = contains_pattern(trimmed);
            qb.push(
                r#"
                AND EXISTS (
                    SELECT 1 FROM ip_addresses ia
                    INNER JOIN ip_domains idm ON idm.ip_id = ia.id
                    INNER JOIN domains d ON d.id = idm.domain_id
                    WHERE ia.ip = c.remote_ip AND LOWER(d.domain) LIKE LOWER(
                "#,
            );
            qb.push_bind(pattern);
            qb.push(") ESCAPE '\\')");
        }
    }

    if let Some(ref network) = filter.network {
        let trimmed = network.trim();
        if !trimmed.is_empty() {
            let pattern = contains_pattern(trimmed);
            qb.push(
                r#"
                AND EXISTS (
                    SELECT 1 FROM ip_addresses ia
                    INNER JOIN ip_networks inw ON inw.ip_id = ia.id
                    INNER JOIN networks n ON n.id = inw.network_id
                    WHERE ia.ip = c.remote_ip AND (
                        LOWER(n.cidr) LIKE LOWER(
                "#,
            );
            qb.push_bind(pattern.clone());
            qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.net_name, '')) LIKE LOWER(");
            qb.push_bind(pattern);
            qb.push(") ESCAPE '\\'))");
        }
    }

    if let Some(ref protocol) = filter.protocol {
        let trimmed = protocol.trim();
        if !trimmed.is_empty() {
            qb.push(" AND LOWER(c.protocol) = LOWER(");
            qb.push_bind(trimmed.to_owned());
            qb.push(")");
        }
    }

    if let Some(from) = filter.started_from {
        qb.push(" AND c.started_at >= ");
        qb.push_bind(from);
    }

    if let Some(to) = filter.started_to {
        qb.push(" AND c.started_at <= ");
        qb.push_bind(to);
    }

    if filter.active_only {
        qb.push(" AND c.ended_at IS NULL");
    }
}

fn clamp_limit(limit: u32) -> u32 {
    limit.clamp(1, MAX_PAGE_SIZE)
}

/// Runs a filtered, sorted, paginated connection-history query.
///
/// Always applies `LIMIT`/`OFFSET` so callers never load the full table by default.
pub async fn query_connection_history(
    pool: &SqlitePool,
    query: &ConnectionHistoryQuery,
) -> Result<ConnectionHistoryPage, DatabaseError> {
    let limit = clamp_limit(query.limit);
    let offset = query.offset;

    let mut count_qb = QueryBuilder::new(
        r#"
        SELECT COUNT(*)
        FROM connections c
        INNER JOIN processes p ON p.id = c.process_id
        WHERE 1 = 1
        "#,
    );
    push_filters(&mut count_qb, &query.filter);
    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut list_qb = QueryBuilder::new(
        r#"
        SELECT
            c.id AS id,
            c.process_id AS process_id,
            p.process_name AS process_name,
            p.pid AS pid,
            c.protocol AS protocol,
            c.direction AS direction,
            c.local_ip AS local_ip,
            c.local_port AS local_port,
            c.remote_ip AS remote_ip,
            c.remote_port AS remote_port,
            c.started_at AS started_at,
            c.ended_at AS ended_at,
            c.bytes_sent AS bytes_sent,
            c.bytes_received AS bytes_received,
        "#,
    );
    list_qb.push(DOMAIN_SUBQUERY);
    list_qb.push(" AS domain, ");
    list_qb.push(NETWORK_CIDR_SUBQUERY);
    list_qb.push(" AS network_cidr, ");
    list_qb.push(NETWORK_NAME_SUBQUERY);
    list_qb.push(" AS network_name, ");
    list_qb.push(ORGANIZATION_SUBQUERY);
    list_qb.push(
        r#"
         AS organization
        FROM connections c
        INNER JOIN processes p ON p.id = c.process_id
        WHERE 1 = 1
        "#,
    );
    push_filters(&mut list_qb, &query.filter);

    list_qb.push(" ORDER BY ");
    list_qb.push(sort_sql(query.sort_by));
    list_qb.push(" ");
    list_qb.push(sort_dir_sql(query.sort_dir));
    // Stable tie-breaker for page boundaries.
    list_qb.push(", c.id ");
    list_qb.push(sort_dir_sql(query.sort_dir));
    list_qb.push(" LIMIT ");
    list_qb.push_bind(i64::from(limit));
    list_qb.push(" OFFSET ");
    list_qb.push_bind(offset as i64);

    let rows = list_qb
        .build_query_as::<ConnectionHistoryRow>()
        .fetch_all(pool)
        .await?;

    Ok(ConnectionHistoryPage {
        rows,
        total: total as u64,
        limit,
        offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use crate::models::{NewConnection, NewIpDomain, NewNetwork, NewProcess};
    use crate::writes::{
        insert_connection, insert_process, store_winning_ip_network, update_connection,
        upsert_ip_domain,
    };
    use crate::{ConnectionUpdate, Database};

    fn t(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
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

    async fn seed_connection(
        db: &Database,
        process_id: i64,
        protocol: &str,
        remote_ip: &str,
        remote_port: u16,
        started_at: DateTime<Utc>,
        ended_at: Option<DateTime<Utc>>,
    ) -> i64 {
        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: protocol.into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 40000 + remote_port,
                remote_ip: remote_ip.into(),
                remote_port,
                started_at,
                ended_at,
                bytes_sent: Some(10),
                bytes_received: Some(20),
                collector_id: Some("test".into()),
            },
        )
        .await
        .expect("insert connection")
    }

    async fn seed_fixture(db: &Database) -> (i64, i64, Vec<i64>) {
        let safari = seed_process(db, 100, "Safari", t(2024, 1, 1, 0, 0, 0)).await;
        let curl = seed_process(db, 200, "curl", t(2024, 1, 1, 0, 0, 1)).await;

        let mut ids = Vec::new();
        // Active Safari → example.com / TEST-NET
        ids.push(
            seed_connection(
                db,
                safari,
                "tcp",
                "203.0.113.10",
                443,
                t(2024, 6, 1, 10, 0, 0),
                None,
            )
            .await,
        );
        // Closed Safari → DNS
        ids.push(
            seed_connection(
                db,
                safari,
                "udp",
                "8.8.8.8",
                53,
                t(2024, 6, 1, 11, 0, 0),
                Some(t(2024, 6, 1, 11, 0, 5)),
            )
            .await,
        );
        // Active curl → same TEST-NET IP
        ids.push(
            seed_connection(
                db,
                curl,
                "tcp",
                "203.0.113.10",
                80,
                t(2024, 6, 2, 9, 0, 0),
                None,
            )
            .await,
        );
        // Closed curl → unrelated IP
        ids.push(
            seed_connection(
                db,
                curl,
                "tcp",
                "198.51.100.1",
                443,
                t(2024, 6, 3, 8, 0, 0),
                Some(t(2024, 6, 3, 8, 1, 0)),
            )
            .await,
        );

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
        .expect("domain");

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

        (safari, curl, ids)
    }

    #[tokio::test]
    async fn filter_combinations_return_enriched_rows() {
        let db = Database::open_in_memory().await.expect("open");
        let (safari, _curl, ids) = seed_fixture(&db).await;

        let page = query_connection_history(
            db.pool(),
            &ConnectionHistoryQuery {
                filter: ConnectionHistoryFilter {
                    process_id: Some(safari),
                    protocol: Some("tcp".into()),
                    domain: Some("example.com".into()),
                    network: Some("TEST-NET".into()),
                    remote_ip: Some("203.0.113".into()),
                    active_only: true,
                    started_from: Some(t(2024, 6, 1, 0, 0, 0)),
                    started_to: Some(t(2024, 6, 1, 23, 59, 59)),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .expect("query");

        assert_eq!(page.total, 1);
        assert_eq!(page.rows.len(), 1);
        let row = &page.rows[0];
        assert_eq!(row.id, ids[0]);
        assert_eq!(row.process_name.as_deref(), Some("Safari"));
        assert_eq!(row.domain.as_deref(), Some("www.example.com"));
        assert_eq!(row.network_cidr.as_deref(), Some("203.0.113.0/24"));
        assert_eq!(row.network_name.as_deref(), Some("TEST-NET-3"));
        assert_eq!(row.organization.as_deref(), Some("Example Org"));
        assert!(row.ended_at.is_none());

        let text_page = query_connection_history(
            db.pool(),
            &ConnectionHistoryQuery {
                filter: ConnectionHistoryFilter {
                    text: Some("Example Org".into()),
                    process_name: Some("curl".into()),
                    ..Default::default()
                },
                sort_by: ConnectionSortColumn::StartedAt,
                sort_dir: SortDirection::Asc,
                ..Default::default()
            },
        )
        .await
        .expect("text query");
        assert_eq!(text_page.total, 1);
        assert_eq!(text_page.rows[0].id, ids[2]);
        assert_eq!(text_page.rows[0].process_name.as_deref(), Some("curl"));
    }

    #[tokio::test]
    async fn empty_result_for_nonmatching_filters() {
        let db = Database::open_in_memory().await.expect("open");
        seed_fixture(&db).await;

        let page = query_connection_history(
            db.pool(),
            &ConnectionHistoryQuery {
                filter: ConnectionHistoryFilter {
                    protocol: Some("sctp".into()),
                    active_only: true,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .await
        .expect("query");

        assert_eq!(page.total, 0);
        assert!(page.rows.is_empty());
        assert_eq!(page.limit, DEFAULT_PAGE_SIZE);
        assert_eq!(page.offset, 0);
    }

    #[tokio::test]
    async fn page_boundaries_use_limit_and_offset() {
        let db = Database::open_in_memory().await.expect("open");
        let process_id = seed_process(&db, 1, "pager", t(2024, 1, 1, 0, 0, 0)).await;

        let mut expected_ids = Vec::new();
        for i in 0..55u32 {
            let id = seed_connection(
                &db,
                process_id,
                "tcp",
                "203.0.113.50",
                1000 + i as u16,
                t(2024, 7, 1, 0, 0, 0) + chrono::Duration::seconds(i as i64),
                None,
            )
            .await;
            expected_ids.push(id);
        }
        // Newest-first default sort.
        expected_ids.reverse();

        let page1 = query_connection_history(
            db.pool(),
            &ConnectionHistoryQuery {
                limit: DEFAULT_PAGE_SIZE,
                offset: 0,
                ..Default::default()
            },
        )
        .await
        .expect("page1");
        assert_eq!(page1.total, 55);
        assert_eq!(page1.limit, DEFAULT_PAGE_SIZE);
        assert_eq!(page1.rows.len(), 50);
        assert_eq!(
            page1.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            expected_ids[..50]
        );

        let page2 = query_connection_history(
            db.pool(),
            &ConnectionHistoryQuery {
                limit: DEFAULT_PAGE_SIZE,
                offset: 50,
                ..Default::default()
            },
        )
        .await
        .expect("page2");
        assert_eq!(page2.total, 55);
        assert_eq!(page2.rows.len(), 5);
        assert_eq!(
            page2.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            expected_ids[50..]
        );

        let page3 = query_connection_history(
            db.pool(),
            &ConnectionHistoryQuery {
                limit: DEFAULT_PAGE_SIZE,
                offset: 100,
                ..Default::default()
            },
        )
        .await
        .expect("page3");
        assert_eq!(page3.total, 55);
        assert!(page3.rows.is_empty());

        // Limit is clamped so callers cannot request an unbounded page.
        let capped = query_connection_history(
            db.pool(),
            &ConnectionHistoryQuery {
                limit: 10_000,
                offset: 0,
                ..Default::default()
            },
        )
        .await
        .expect("capped");
        assert_eq!(capped.limit, MAX_PAGE_SIZE);
        assert_eq!(capped.rows.len(), 55);
    }

    #[tokio::test]
    async fn history_rows_survive_database_reopen() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("history.db");

        let (safari, connection_id, domain, org) = {
            let db = Database::open(&path).await.expect("open");
            let (safari, _curl, ids) = seed_fixture(&db).await;
            let connection_id = ids[0];

            // Close one connection after the initial seed so reopen sees updates too.
            update_connection(
                db.pool(),
                ids[2],
                &ConnectionUpdate {
                    ended_at: Some(Some(t(2024, 6, 2, 9, 5, 0))),
                    ..Default::default()
                },
            )
            .await
            .expect("close curl conn");

            let page = query_connection_history(
                db.pool(),
                &ConnectionHistoryQuery {
                    filter: ConnectionHistoryFilter {
                        process_id: Some(safari),
                        active_only: true,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .await
            .expect("query before reopen");
            assert_eq!(page.total, 1);
            assert_eq!(page.rows[0].id, connection_id);
            (
                safari,
                connection_id,
                page.rows[0].domain.clone(),
                page.rows[0].organization.clone(),
            )
        };

        let db = Database::open(&path).await.expect("reopen");
        let page = query_connection_history(
            db.pool(),
            &ConnectionHistoryQuery {
                filter: ConnectionHistoryFilter {
                    process_id: Some(safari),
                    active_only: true,
                    domain: Some("example.com".into()),
                    ..Default::default()
                },
                sort_by: ConnectionSortColumn::RemotePort,
                sort_dir: SortDirection::Asc,
                ..Default::default()
            },
        )
        .await
        .expect("query after reopen");

        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].id, connection_id);
        assert_eq!(page.rows[0].domain, domain);
        assert_eq!(page.rows[0].organization, org);
        assert_eq!(page.rows[0].domain.as_deref(), Some("www.example.com"));
        assert_eq!(page.rows[0].organization.as_deref(), Some("Example Org"));
    }
}
