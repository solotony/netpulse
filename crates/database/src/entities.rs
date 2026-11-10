//! Paginated entity list queries for Processes, IPs, Domains, and Networks UI pages.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, QueryBuilder, SqlitePool};

use crate::error::DatabaseError;
use crate::queries::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};

/// Optional networks-page filter by assigned group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum NetworkGroupFilter {
    /// No group constraint.
    #[default]
    Any,
    /// Only networks with `group_id IS NULL`.
    Ungrouped,
    /// Only networks belonging to this group id.
    Group(i64),
}

/// Shared pagination + optional text search for entity tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityListQuery {
    pub text: Option<String>,
    pub limit: u32,
    pub offset: u64,
    /// Applied by [`list_networks_page`]; ignored by other entity lists.
    pub group_filter: NetworkGroupFilter,
}

impl Default for EntityListQuery {
    fn default() -> Self {
        Self {
            text: None,
            limit: DEFAULT_PAGE_SIZE,
            offset: 0,
            group_filter: NetworkGroupFilter::Any,
        }
    }
}

fn clamp_limit(limit: u32) -> u32 {
    limit.clamp(1, MAX_PAGE_SIZE)
}

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

/// Process row with connection aggregations for the Processes page.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ProcessListRow {
    pub id: i64,
    pub process_name: Option<String>,
    pub pid: i64,
    pub executable_path: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub total_connections: i64,
    pub unique_remote_ips: i64,
    pub unique_domains: i64,
    pub bytes_sent: i64,
    pub bytes_received: i64,
}

/// One page of process list rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessListPage {
    pub rows: Vec<ProcessListRow>,
    pub total: u64,
    pub limit: u32,
    pub offset: u64,
}

/// Remote IP row with enrichment and connection aggregations.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct IpListRow {
    pub ip: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub connection_count: i64,
    pub process_count: i64,
    pub domain_count: i64,
    pub cidr: Option<String>,
    pub net_name: Option<String>,
    pub asn: Option<i64>,
    pub organization: Option<String>,
    pub country: Option<String>,
}

/// One page of IP list rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpListPage {
    pub rows: Vec<IpListRow>,
    pub total: u64,
    pub limit: u32,
    pub offset: u64,
}

/// Domain row with association aggregations.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct DomainListRow {
    pub id: i64,
    pub domain: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ip_count: i64,
    pub process_count: i64,
    pub connection_count: i64,
    pub sources: Option<String>,
}

/// One page of domain list rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainListPage {
    pub rows: Vec<DomainListRow>,
    pub total: u64,
    pub limit: u32,
    pub offset: u64,
}

/// Network registration row with IP and connection aggregations.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct NetworkListRow {
    pub id: i64,
    pub cidr: String,
    pub net_range: Option<String>,
    pub net_name: Option<String>,
    pub asn: Option<i64>,
    pub organization: Option<String>,
    pub country: Option<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub ip_count: i64,
    pub connection_count: i64,
    pub first_seen_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

/// One page of network list rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkListPage {
    pub rows: Vec<NetworkListRow>,
    pub total: u64,
    pub limit: u32,
    pub offset: u64,
}

/// Lists processes with connection aggregations.
pub async fn list_processes(
    pool: &SqlitePool,
    query: &EntityListQuery,
) -> Result<ProcessListPage, DatabaseError> {
    let limit = clamp_limit(query.limit);
    let offset = query.offset;
    let pattern = query
        .text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(contains_pattern);

    let mut count_qb = QueryBuilder::new(
        r#"
        SELECT COUNT(*)
        FROM processes p
        WHERE 1 = 1
        "#,
    );
    if let Some(ref pattern) = pattern {
        count_qb.push(" AND (LOWER(IFNULL(p.process_name, '')) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\' OR CAST(p.pid AS TEXT) LIKE ");
        count_qb.push_bind(pattern.clone());
        count_qb.push(" ESCAPE '\\' OR LOWER(IFNULL(p.executable_path, '')) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\')");
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut list_qb = QueryBuilder::new(
        r#"
        SELECT
            p.id AS id,
            p.process_name AS process_name,
            p.pid AS pid,
            p.executable_path AS executable_path,
            p.first_seen_at AS first_seen_at,
            p.last_seen_at AS last_seen_at,
            COUNT(DISTINCT c.id) AS total_connections,
            COUNT(DISTINCT c.remote_ip) AS unique_remote_ips,
            COUNT(DISTINCT d.id) AS unique_domains,
            COALESCE(SUM(c.bytes_sent), 0) AS bytes_sent,
            COALESCE(SUM(c.bytes_received), 0) AS bytes_received
        FROM processes p
        LEFT JOIN connections c ON c.process_id = p.id
        LEFT JOIN ip_addresses ia ON ia.ip = c.remote_ip
        LEFT JOIN ip_domains idm ON idm.ip_id = ia.id
        LEFT JOIN domains d ON d.id = idm.domain_id
        WHERE 1 = 1
        "#,
    );
    if let Some(ref pattern) = pattern {
        list_qb.push(" AND (LOWER(IFNULL(p.process_name, '')) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\' OR CAST(p.pid AS TEXT) LIKE ");
        list_qb.push_bind(pattern.clone());
        list_qb.push(" ESCAPE '\\' OR LOWER(IFNULL(p.executable_path, '')) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\')");
    }
    list_qb.push(
        r#"
        GROUP BY p.id, p.process_name, p.pid, p.executable_path, p.first_seen_at, p.last_seen_at
        ORDER BY p.last_seen_at DESC, p.id DESC
        LIMIT 
        "#,
    );
    list_qb.push_bind(i64::from(limit));
    list_qb.push(" OFFSET ");
    list_qb.push_bind(offset as i64);

    let rows = list_qb
        .build_query_as::<ProcessListRow>()
        .fetch_all(pool)
        .await?;

    Ok(ProcessListPage {
        rows,
        total: total as u64,
        limit,
        offset,
    })
}

/// Lists IP addresses from enrichment storage and connection history.
///
/// Includes manually added addresses that have never appeared in connections.
pub async fn list_ip_addresses(
    pool: &SqlitePool,
    query: &EntityListQuery,
) -> Result<IpListPage, DatabaseError> {
    let limit = clamp_limit(query.limit);
    let offset = query.offset;
    let pattern = query
        .text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(contains_pattern);

    let mut count_qb = QueryBuilder::new(
        r#"
        SELECT COUNT(*)
        FROM (
            SELECT ia.ip AS ip
            FROM ip_addresses ia
            UNION
            SELECT c.remote_ip AS ip
            FROM connections c
        ) ips
        LEFT JOIN ip_addresses ia ON ia.ip = ips.ip
        LEFT JOIN ip_networks inw ON inw.ip_id = ia.id
        LEFT JOIN networks n ON n.id = inw.network_id
        WHERE 1 = 1
        "#,
    );
    if let Some(ref pattern) = pattern {
        count_qb.push(" AND (LOWER(ips.ip) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.cidr, '')) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.net_name, '')) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.organization, '')) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\')");
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut list_qb = QueryBuilder::new(
        r#"
        SELECT
            ips.ip AS ip,
            COALESCE(
                ia.first_seen_at,
                (SELECT MIN(c.started_at) FROM connections c WHERE c.remote_ip = ips.ip)
            ) AS first_seen_at,
            COALESCE(
                ia.last_seen_at,
                (SELECT MAX(COALESCE(c.ended_at, c.started_at)) FROM connections c WHERE c.remote_ip = ips.ip)
            ) AS last_seen_at,
            (SELECT COUNT(DISTINCT c.id) FROM connections c WHERE c.remote_ip = ips.ip) AS connection_count,
            (SELECT COUNT(DISTINCT c.process_id) FROM connections c WHERE c.remote_ip = ips.ip) AS process_count,
            (
                SELECT COUNT(DISTINCT idm.domain_id)
                FROM ip_addresses ia2
                INNER JOIN ip_domains idm ON idm.ip_id = ia2.id
                WHERE ia2.ip = ips.ip
            ) AS domain_count,
            (
                SELECT n.cidr
                FROM ip_addresses ia3
                INNER JOIN ip_networks inw ON inw.ip_id = ia3.id
                INNER JOIN networks n ON n.id = inw.network_id
                WHERE ia3.ip = ips.ip
                ORDER BY n.updated_at DESC, n.id ASC
                LIMIT 1
            ) AS cidr,
            (
                SELECT n.net_name
                FROM ip_addresses ia3
                INNER JOIN ip_networks inw ON inw.ip_id = ia3.id
                INNER JOIN networks n ON n.id = inw.network_id
                WHERE ia3.ip = ips.ip
                ORDER BY n.updated_at DESC, n.id ASC
                LIMIT 1
            ) AS net_name,
            (
                SELECT n.asn
                FROM ip_addresses ia3
                INNER JOIN ip_networks inw ON inw.ip_id = ia3.id
                INNER JOIN networks n ON n.id = inw.network_id
                WHERE ia3.ip = ips.ip
                ORDER BY n.updated_at DESC, n.id ASC
                LIMIT 1
            ) AS asn,
            (
                SELECT n.organization
                FROM ip_addresses ia3
                INNER JOIN ip_networks inw ON inw.ip_id = ia3.id
                INNER JOIN networks n ON n.id = inw.network_id
                WHERE ia3.ip = ips.ip
                ORDER BY n.updated_at DESC, n.id ASC
                LIMIT 1
            ) AS organization,
            (
                SELECT n.country
                FROM ip_addresses ia3
                INNER JOIN ip_networks inw ON inw.ip_id = ia3.id
                INNER JOIN networks n ON n.id = inw.network_id
                WHERE ia3.ip = ips.ip
                ORDER BY n.updated_at DESC, n.id ASC
                LIMIT 1
            ) AS country
        FROM (
            SELECT ia.ip AS ip
            FROM ip_addresses ia
            UNION
            SELECT c.remote_ip AS ip
            FROM connections c
        ) ips
        LEFT JOIN ip_addresses ia ON ia.ip = ips.ip
        WHERE 1 = 1
        "#,
    );
    if let Some(ref pattern) = pattern {
        list_qb.push(" AND (LOWER(ips.ip) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(
            r#") ESCAPE '\' OR EXISTS (
                SELECT 1 FROM ip_addresses iaf
                INNER JOIN ip_networks inw ON inw.ip_id = iaf.id
                INNER JOIN networks n ON n.id = inw.network_id
                WHERE iaf.ip = ips.ip AND (
                    LOWER(IFNULL(n.cidr, '')) LIKE LOWER(
            "#,
        );
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.net_name, '')) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.organization, '')) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\'))");
        list_qb.push(")");
    }
    list_qb.push(
        r#"
        ORDER BY last_seen_at DESC, ip ASC
        LIMIT 
        "#,
    );
    list_qb.push_bind(i64::from(limit));
    list_qb.push(" OFFSET ");
    list_qb.push_bind(offset as i64);

    let rows = list_qb
        .build_query_as::<IpListRow>()
        .fetch_all(pool)
        .await?;

    Ok(IpListPage {
        rows,
        total: total as u64,
        limit,
        offset,
    })
}

/// Lists domains with association aggregations.
pub async fn list_domains(
    pool: &SqlitePool,
    query: &EntityListQuery,
) -> Result<DomainListPage, DatabaseError> {
    let limit = clamp_limit(query.limit);
    let offset = query.offset;
    let pattern = query
        .text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(contains_pattern);

    let mut count_qb = QueryBuilder::new(
        r#"
        SELECT COUNT(*)
        FROM domains d
        WHERE 1 = 1
        "#,
    );
    if let Some(ref pattern) = pattern {
        count_qb.push(" AND LOWER(d.domain) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\'");
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut list_qb = QueryBuilder::new(
        r#"
        SELECT
            d.id AS id,
            d.domain AS domain,
            d.first_seen_at AS first_seen_at,
            d.last_seen_at AS last_seen_at,
            COUNT(DISTINCT ia.id) AS ip_count,
            COUNT(DISTINCT c.process_id) AS process_count,
            COUNT(DISTINCT c.id) AS connection_count,
            (
                SELECT GROUP_CONCAT(DISTINCT idm2.source)
                FROM ip_domains idm2
                WHERE idm2.domain_id = d.id
            ) AS sources
        FROM domains d
        LEFT JOIN ip_domains idm ON idm.domain_id = d.id
        LEFT JOIN ip_addresses ia ON ia.id = idm.ip_id
        LEFT JOIN connections c ON c.remote_ip = ia.ip
        WHERE 1 = 1
        "#,
    );
    if let Some(ref pattern) = pattern {
        list_qb.push(" AND LOWER(d.domain) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\'");
    }
    list_qb.push(
        r#"
        GROUP BY d.id, d.domain, d.first_seen_at, d.last_seen_at
        ORDER BY d.last_seen_at DESC, d.domain ASC
        LIMIT 
        "#,
    );
    list_qb.push_bind(i64::from(limit));
    list_qb.push(" OFFSET ");
    list_qb.push_bind(offset as i64);

    let rows = list_qb
        .build_query_as::<DomainListRow>()
        .fetch_all(pool)
        .await?;

    Ok(DomainListPage {
        rows,
        total: total as u64,
        limit,
        offset,
    })
}

/// Lists networks with IP and connection aggregations.
pub async fn list_networks_page(
    pool: &SqlitePool,
    query: &EntityListQuery,
) -> Result<NetworkListPage, DatabaseError> {
    let limit = clamp_limit(query.limit);
    let offset = query.offset;
    let pattern = query
        .text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(contains_pattern);

    let mut count_qb = QueryBuilder::new(
        r#"
        SELECT COUNT(*)
        FROM networks n
        LEFT JOIN "groups" g ON g.id = n.group_id
        WHERE 1 = 1
        "#,
    );
    push_network_group_filter(&mut count_qb, &query.group_filter);
    if let Some(ref pattern) = pattern {
        count_qb.push(" AND (LOWER(n.cidr) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.net_name, '')) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.organization, '')) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(g.name, '')) LIKE LOWER(");
        count_qb.push_bind(pattern.clone());
        count_qb.push(") ESCAPE '\\')");
    }
    let total: i64 = count_qb.build_query_scalar().fetch_one(pool).await?;

    let mut list_qb = QueryBuilder::new(
        r#"
        SELECT
            n.id AS id,
            n.cidr AS cidr,
            n.net_range AS net_range,
            n.net_name AS net_name,
            n.asn AS asn,
            n.organization AS organization,
            n.country AS country,
            n.group_id AS group_id,
            g.name AS group_name,
            COUNT(DISTINCT ia.id) AS ip_count,
            COUNT(DISTINCT c.id) AS connection_count,
            MIN(ia.first_seen_at) AS first_seen_at,
            MAX(ia.last_seen_at) AS last_seen_at
        FROM networks n
        LEFT JOIN "groups" g ON g.id = n.group_id
        LEFT JOIN ip_networks inw ON inw.network_id = n.id
        LEFT JOIN ip_addresses ia ON ia.id = inw.ip_id
        LEFT JOIN connections c ON c.remote_ip = ia.ip
        WHERE 1 = 1
        "#,
    );
    push_network_group_filter(&mut list_qb, &query.group_filter);
    if let Some(ref pattern) = pattern {
        list_qb.push(" AND (LOWER(n.cidr) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.net_name, '')) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(n.organization, '')) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\' OR LOWER(IFNULL(g.name, '')) LIKE LOWER(");
        list_qb.push_bind(pattern.clone());
        list_qb.push(") ESCAPE '\\')");
    }
    list_qb.push(
        r#"
        GROUP BY n.id, n.cidr, n.net_range, n.net_name, n.asn, n.organization, n.country,
                 n.group_id, g.name
        ORDER BY connection_count DESC, n.cidr ASC
        LIMIT 
        "#,
    );
    list_qb.push_bind(i64::from(limit));
    list_qb.push(" OFFSET ");
    list_qb.push_bind(offset as i64);

    let rows = list_qb
        .build_query_as::<NetworkListRow>()
        .fetch_all(pool)
        .await?;

    Ok(NetworkListPage {
        rows,
        total: total as u64,
        limit,
        offset,
    })
}

fn push_network_group_filter(qb: &mut QueryBuilder<'_, sqlx::Sqlite>, filter: &NetworkGroupFilter) {
    match filter {
        NetworkGroupFilter::Any => {}
        NetworkGroupFilter::Ungrouped => {
            qb.push(" AND n.group_id IS NULL");
        }
        NetworkGroupFilter::Group(id) => {
            qb.push(" AND n.group_id = ");
            qb.push_bind(*id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    use crate::models::{NewConnection, NewIpDomain, NewNetwork, NewProcess};
    use crate::writes::{insert_connection, insert_process, store_winning_ip_network, upsert_ip_domain};
    use crate::Database;

    fn t(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, s).unwrap()
    }

    async fn seed(db: &Database) {
        let safari = insert_process(
            db.pool(),
            &NewProcess {
                pid: 100,
                process_name: Some("Safari".into()),
                executable_path: Some("/Applications/Safari.app".into()),
                process_start_time: t(2024, 1, 1, 0, 0, 0),
                first_seen_at: t(2024, 6, 1, 10, 0, 0),
                last_seen_at: t(2024, 6, 1, 12, 0, 0),
            },
        )
        .await
        .unwrap();

        insert_connection(
            db.pool(),
            &NewConnection {
                process_id: safari,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 54321,
                remote_ip: "203.0.113.10".into(),
                remote_port: 443,
                started_at: t(2024, 6, 1, 10, 0, 0),
                ended_at: None,
                bytes_sent: Some(100),
                bytes_received: Some(200),
                collector_id: Some("test".into()),
            },
        )
        .await
        .unwrap();

        upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: "203.0.113.10".into(),
                domain: "example.com".into(),
                source: "REVERSE_DNS".into(),
                confidence: Some(0.9),
                seen_at: t(2024, 6, 1, 10, 0, 1),
            },
        )
        .await
        .unwrap();

        store_winning_ip_network(
            db.pool(),
            "203.0.113.10",
            &NewNetwork {
                cidr: "203.0.113.0/24".into(),
                net_range: Some("203.0.113.0 - 203.0.113.255".into()),
                net_name: Some("TEST-NET-3".into()),
                organization: Some("Example Org".into()),
                asn: Some(64500),
                country: Some("US".into()),
                source: Some("local".into()),
                updated_at: t(2024, 6, 1, 10, 0, 2),
            },
            t(2024, 6, 1, 10, 0, 2),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_processes_returns_aggregations() {
        let db = Database::open_in_memory().await.unwrap();
        seed(&db).await;

        let page = list_processes(db.pool(), &EntityListQuery::default())
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.rows.len(), 1);
        let row = &page.rows[0];
        assert_eq!(row.process_name.as_deref(), Some("Safari"));
        assert_eq!(row.total_connections, 1);
        assert_eq!(row.unique_remote_ips, 1);
        assert_eq!(row.unique_domains, 1);
        assert_eq!(row.bytes_sent, 100);
        assert_eq!(row.bytes_received, 200);
    }

    #[tokio::test]
    async fn list_ip_addresses_includes_enrichment() {
        let db = Database::open_in_memory().await.unwrap();
        seed(&db).await;

        let page = list_ip_addresses(db.pool(), &EntityListQuery::default())
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.rows[0];
        assert_eq!(row.ip, "203.0.113.10");
        assert_eq!(row.connection_count, 1);
        assert_eq!(row.domain_count, 1);
        assert_eq!(row.cidr.as_deref(), Some("203.0.113.0/24"));
        assert_eq!(row.organization.as_deref(), Some("Example Org"));
    }

    #[tokio::test]
    async fn list_domains_and_networks_pages() {
        let db = Database::open_in_memory().await.unwrap();
        seed(&db).await;

        let domains = list_domains(db.pool(), &EntityListQuery::default())
            .await
            .unwrap();
        assert_eq!(domains.total, 1);
        assert_eq!(domains.rows[0].domain, "example.com");
        assert_eq!(domains.rows[0].connection_count, 1);
        assert!(domains.rows[0]
            .sources
            .as_deref()
            .unwrap_or("")
            .contains("REVERSE_DNS"));

        let networks = list_networks_page(db.pool(), &EntityListQuery::default())
            .await
            .unwrap();
        assert_eq!(networks.total, 1);
        assert_eq!(networks.rows[0].cidr, "203.0.113.0/24");
        assert_eq!(networks.rows[0].ip_count, 1);
        assert_eq!(networks.rows[0].connection_count, 1);
    }

    #[tokio::test]
    async fn entity_text_filter_works() {
        let db = Database::open_in_memory().await.unwrap();
        seed(&db).await;

        let none = list_processes(
            db.pool(),
            &EntityListQuery {
                text: Some("zzz".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(none.total, 0);

        let hit = list_ip_addresses(
            db.pool(),
            &EntityListQuery {
                text: Some("203.0.113".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(hit.total, 1);
    }

    #[tokio::test]
    async fn list_ip_addresses_includes_manual_without_connections() {
        let db = Database::open_in_memory().await.unwrap();
        crate::writes::upsert_ip_address(db.pool(), "198.51.100.50", t(2024, 6, 1, 11, 0, 0))
            .await
            .unwrap();

        let page = list_ip_addresses(db.pool(), &EntityListQuery::default())
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].ip, "198.51.100.50");
        assert_eq!(page.rows[0].connection_count, 0);
    }

    #[tokio::test]
    async fn list_networks_filters_by_group_and_ungrouped() {
        let db = Database::open_in_memory().await.unwrap();
        let seen = t(2024, 6, 1, 11, 0, 0);
        let group_id = crate::groups::create_group(db.pool(), "OpenAI", seen)
            .await
            .unwrap();

        let grouped = crate::writes::upsert_network(
            db.pool(),
            &NewNetwork {
                cidr: "203.0.113.0/24".into(),
                net_range: None,
                net_name: Some("GROUPED".into()),
                organization: None,
                asn: None,
                country: None,
                source: Some("manual".into()),
                updated_at: seen,
            },
        )
        .await
        .unwrap();
        crate::writes::upsert_network(
            db.pool(),
            &NewNetwork {
                cidr: "198.51.100.0/24".into(),
                net_range: None,
                net_name: Some("FREE".into()),
                organization: None,
                asn: None,
                country: None,
                source: Some("manual".into()),
                updated_at: seen,
            },
        )
        .await
        .unwrap();
        crate::groups::set_network_group(db.pool(), grouped, Some(group_id))
            .await
            .unwrap();

        let all = list_networks_page(db.pool(), &EntityListQuery::default())
            .await
            .unwrap();
        assert_eq!(all.total, 2);

        let by_group = list_networks_page(
            db.pool(),
            &EntityListQuery {
                group_filter: NetworkGroupFilter::Group(group_id),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_group.total, 1);
        assert_eq!(by_group.rows[0].cidr, "203.0.113.0/24");

        let ungrouped = list_networks_page(
            db.pool(),
            &EntityListQuery {
                group_filter: NetworkGroupFilter::Ungrouped,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(ungrouped.total, 1);
        assert_eq!(ungrouped.rows[0].cidr, "198.51.100.0/24");
    }

    #[tokio::test]
    async fn forward_dns_domain_links_and_group_on_network() {
        let db = Database::open_in_memory().await.unwrap();
        let seen = t(2024, 6, 1, 11, 0, 0);

        upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: "203.0.113.10".into(),
                domain: "chatgpt.com".into(),
                source: "FORWARD_DNS".into(),
                confidence: Some(0.95),
                seen_at: seen,
            },
        )
        .await
        .unwrap();

        store_winning_ip_network(
            db.pool(),
            "203.0.113.10",
            &NewNetwork {
                cidr: "203.0.113.0/24".into(),
                net_range: None,
                net_name: Some("TEST".into()),
                organization: None,
                asn: None,
                country: None,
                source: Some("rdap".into()),
                updated_at: seen,
            },
            seen,
        )
        .await
        .unwrap();

        let group_id = crate::groups::create_group(db.pool(), "OpenAI", seen)
            .await
            .unwrap();
        crate::groups::set_networks_group_by_cidr(
            db.pool(),
            &["203.0.113.0/24".into()],
            Some(group_id),
        )
        .await
        .unwrap();

        let domains = list_domains(db.pool(), &EntityListQuery::default())
            .await
            .unwrap();
        assert_eq!(domains.total, 1);
        assert_eq!(domains.rows[0].domain, "chatgpt.com");
        assert_eq!(domains.rows[0].ip_count, 1);
        assert!(domains.rows[0]
            .sources
            .as_deref()
            .unwrap_or("")
            .contains("FORWARD_DNS"));

        let networks = list_networks_page(db.pool(), &EntityListQuery::default())
            .await
            .unwrap();
        assert_eq!(networks.total, 1);
        assert_eq!(networks.rows[0].group_id, Some(group_id));
        assert_eq!(networks.rows[0].group_name.as_deref(), Some("OpenAI"));
    }
}
