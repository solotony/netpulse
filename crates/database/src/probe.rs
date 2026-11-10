//! Prepare/measure probe: baseline remote IPs, then IPs that appear only while measuring.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, QueryBuilder, SqlitePool};

use crate::error::DatabaseError;

/// How an application key matches `processes.executable_path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKeyKind {
    /// Exact executable path match.
    Exact,
    /// macOS `.app` bundle prefix: path equals key or is under `key/`.
    Bundle,
}

/// One selectable application for the Probe page.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ProbeAppRow {
    pub app_key: String,
    pub display_name: Option<String>,
    pub process_count: i64,
    pub last_seen_at: DateTime<Utc>,
}

/// Whether a listed IP was already in the baseline or first appeared while measuring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeIpOrigin {
    Baseline,
    New,
}

/// Remote IP from the prepare window, or a measure-only IP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeDiffRow {
    pub remote_ip: String,
    pub domains: Option<String>,
    pub ports: Option<String>,
    pub process_names: Option<String>,
    pub first_seen: DateTime<Utc>,
    pub origin: ProbeIpOrigin,
}

/// One distinct remote IP inside a single time window, before baseline/new tagging.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
struct WindowIpRow {
    remote_ip: String,
    domains: Option<String>,
    ports: Option<String>,
    process_names: Option<String>,
    first_seen: DateTime<Utc>,
}

/// Result of a prepare/measure IP set difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeDiffResult {
    pub prepare_ip_count: u64,
    pub measure_ip_count: u64,
    pub rows: Vec<ProbeDiffRow>,
}

/// Query parameters for prepare/measure diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeDiffQuery {
    pub app_keys: Vec<String>,
    pub prepare_start: DateTime<Utc>,
    pub prepare_end: DateTime<Utc>,
    pub measure_start: Option<DateTime<Utc>>,
    pub measure_end: Option<DateTime<Utc>>,
}

/// Derives the Probe app key from an executable path.
///
/// Paths inside `Something.app/...` collapse to the `.app` bundle prefix.
pub fn app_key_from_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(idx) = trimmed.find(".app/") {
        return Some(trimmed[..=idx + 3].to_string());
    }
    if trimmed.ends_with(".app") {
        return Some(trimmed.to_string());
    }
    Some(trimmed.to_string())
}

/// Classifies an app key as a bundle prefix or exact path.
pub fn app_key_kind(key: &str) -> AppKeyKind {
    if key.ends_with(".app") {
        AppKeyKind::Bundle
    } else {
        AppKeyKind::Exact
    }
}

fn push_app_match<'a>(qb: &mut QueryBuilder<'a, sqlx::Sqlite>, app_keys: &'a [String]) {
    // Empty selection means "all applications".
    if app_keys.is_empty() {
        return;
    }
    qb.push(" AND (");
    let mut first = true;
    for key in app_keys {
        if !first {
            qb.push(" OR ");
        }
        first = false;
        if let Some(name) = key.strip_prefix("name:") {
            qb.push("(IFNULL(p.executable_path, '') = '' AND IFNULL(p.process_name, '') = ");
            qb.push_bind(name);
            qb.push(")");
        } else if let Some(pid) = key.strip_prefix("pid:") {
            qb.push("CAST(p.pid AS TEXT) = ");
            qb.push_bind(pid);
        } else {
            match app_key_kind(key) {
                AppKeyKind::Exact => {
                    qb.push("p.executable_path = ");
                    qb.push_bind(key.as_str());
                }
                AppKeyKind::Bundle => {
                    qb.push("(p.executable_path = ");
                    qb.push_bind(key.as_str());
                    qb.push(" OR p.executable_path LIKE ");
                    let like = format!(
                        "{}/%",
                        key.replace('\\', "\\\\")
                            .replace('%', "\\%")
                            .replace('_', "\\_")
                    );
                    qb.push_bind(like);
                    qb.push(" ESCAPE '\\')");
                }
            }
        }
    }
    qb.push(")");
}

/// SQL expression that derives an app key from a process row alias `p`.
const APP_KEY_SQL: &str = r#"
    CASE
        WHEN p.executable_path IS NOT NULL AND TRIM(p.executable_path) != '' THEN
            CASE
                WHEN INSTR(p.executable_path, '.app/') > 0 THEN
                    SUBSTR(p.executable_path, 1, INSTR(p.executable_path, '.app/') + 3)
                ELSE p.executable_path
            END
        WHEN p.process_name IS NOT NULL AND TRIM(p.process_name) != '' THEN
            'name:' || p.process_name
        ELSE
            'pid:' || CAST(p.pid AS TEXT)
    END
"#;

/// Lists applications that had non-loopback connections overlapping `[window_start, window_end]`.
pub async fn list_probe_apps(
    pool: &SqlitePool,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Result<Vec<ProbeAppRow>, DatabaseError> {
    let mut qb = QueryBuilder::new(
        r#"
        WITH keyed AS (
            SELECT
        "#,
    );
    qb.push(APP_KEY_SQL);
    qb.push(
        r#"
                AS app_key,
                p.id AS process_id,
                p.process_name AS process_name,
                c.started_at AS activity_at
            FROM connections c
            INNER JOIN processes p ON p.id = c.process_id
            WHERE NOT (
                c.remote_ip LIKE '127.%'
                OR c.remote_ip = '::1'
                OR LOWER(c.remote_ip) = '::1'
                OR c.remote_ip = '0:0:0:0:0:0:0:1'
            )
              AND c.started_at <= 
        "#,
    );
    qb.push_bind(window_end);
    qb.push(" AND (c.ended_at IS NULL OR c.ended_at >= ");
    qb.push_bind(window_start);
    qb.push(
        r#"
            )
        )
        SELECT
            app_key AS app_key,
            (
                SELECT k2.process_name
                FROM keyed k2
                WHERE k2.app_key = keyed.app_key
                  AND IFNULL(k2.process_name, '') != ''
                ORDER BY k2.activity_at DESC
                LIMIT 1
            ) AS display_name,
            COUNT(DISTINCT process_id) AS process_count,
            MAX(activity_at) AS last_seen_at
        FROM keyed
        GROUP BY app_key
        ORDER BY last_seen_at DESC, app_key ASC
        "#,
    );

    let rows: Vec<ProbeAppRow> = qb.build_query_as().fetch_all(pool).await?;
    Ok(rows)
}

fn push_not_loopback(qb: &mut QueryBuilder<'_, sqlx::Sqlite>) {
    qb.push(
        "NOT (c.remote_ip LIKE '127.%' OR c.remote_ip = '::1' OR LOWER(c.remote_ip) = '::1' OR c.remote_ip = '0:0:0:0:0:0:0:1')",
    );
}

fn push_overlap<'a>(
    qb: &mut QueryBuilder<'a, sqlx::Sqlite>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) {
    qb.push("c.started_at <= ");
    qb.push_bind(window_end);
    qb.push(" AND (c.ended_at IS NULL OR c.ended_at >= ");
    qb.push_bind(window_start);
    qb.push(")");
}

fn with_origin(row: WindowIpRow, origin: ProbeIpOrigin) -> ProbeDiffRow {
    ProbeDiffRow {
        remote_ip: row.remote_ip,
        domains: row.domains,
        ports: row.ports,
        process_names: row.process_names,
        first_seen: row.first_seen,
        origin,
    }
}

/// Distinct non-loopback remote IPs overlapping `[window_start, window_end]`.
async fn list_window_ips(
    pool: &SqlitePool,
    app_keys: &[String],
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Result<Vec<WindowIpRow>, DatabaseError> {
    let mut qb = QueryBuilder::new(
        r#"
        WITH window_ips AS (
            SELECT DISTINCT c.remote_ip AS remote_ip
            FROM connections c
            INNER JOIN processes p ON p.id = c.process_id
            WHERE 
        "#,
    );
    push_not_loopback(&mut qb);
    qb.push(" AND ");
    push_overlap(&mut qb, window_start, window_end);
    push_app_match(&mut qb, app_keys);
    qb.push(
        r#"
        )
        SELECT
            n.remote_ip AS remote_ip,
            (
                SELECT GROUP_CONCAT(d.domain, ', ')
                FROM (
                    SELECT DISTINCT d.domain AS domain
                    FROM ip_addresses ia
                    INNER JOIN ip_domains idm ON idm.ip_id = ia.id
                    INNER JOIN domains d ON d.id = idm.domain_id
                    WHERE ia.ip = n.remote_ip
                    ORDER BY d.domain
                ) d
            ) AS domains,
            (
                SELECT GROUP_CONCAT(port, ', ')
                FROM (
                    SELECT DISTINCT c.remote_port AS port
                    FROM connections c
                    INNER JOIN processes p ON p.id = c.process_id
                    WHERE c.remote_ip = n.remote_ip
                      AND 
        "#,
    );
    push_overlap(&mut qb, window_start, window_end);
    push_app_match(&mut qb, app_keys);
    qb.push(
        r#"
                    ORDER BY c.remote_port
                )
            ) AS ports,
            (
                SELECT GROUP_CONCAT(name, ', ')
                FROM (
                    SELECT DISTINCT IFNULL(p.process_name, '') AS name
                    FROM connections c
                    INNER JOIN processes p ON p.id = c.process_id
                    WHERE c.remote_ip = n.remote_ip
                      AND IFNULL(p.process_name, '') != ''
                      AND 
        "#,
    );
    push_overlap(&mut qb, window_start, window_end);
    push_app_match(&mut qb, app_keys);
    qb.push(
        r#"
                    ORDER BY name
                )
            ) AS process_names,
            (
                SELECT MIN(c.started_at)
                FROM connections c
                INNER JOIN processes p ON p.id = c.process_id
                WHERE c.remote_ip = n.remote_ip
                  AND 
        "#,
    );
    push_overlap(&mut qb, window_start, window_end);
    push_app_match(&mut qb, app_keys);
    qb.push(
        r#"
            ) AS first_seen
        FROM window_ips n
        ORDER BY first_seen ASC, n.remote_ip ASC
        "#,
    );

    let rows: Vec<WindowIpRow> = qb.build_query_as().fetch_all(pool).await?;
    Ok(rows)
}

/// Lists baseline IPs from the prepare window.
///
/// Once both measure bounds are set, measure-only IPs are appended after the baseline.
/// Empty `app_keys` means every application; a non-empty list filters to those apps.
pub async fn diff_probe(
    pool: &SqlitePool,
    query: &ProbeDiffQuery,
) -> Result<ProbeDiffResult, DatabaseError> {
    let baseline = list_window_ips(
        pool,
        &query.app_keys,
        query.prepare_start,
        query.prepare_end,
    )
    .await?;
    let prepare_ip_count = baseline.len() as u64;

    let (Some(measure_start), Some(measure_end)) = (query.measure_start, query.measure_end) else {
        return Ok(ProbeDiffResult {
            prepare_ip_count,
            measure_ip_count: 0,
            rows: baseline
                .into_iter()
                .map(|row| with_origin(row, ProbeIpOrigin::Baseline))
                .collect(),
        });
    };

    let measured = list_window_ips(pool, &query.app_keys, measure_start, measure_end).await?;
    let measure_ip_count = measured.len() as u64;
    let baseline_ips: HashSet<String> = baseline.iter().map(|row| row.remote_ip.clone()).collect();

    let mut rows = Vec::with_capacity(baseline.len() + measured.len());
    for row in baseline {
        rows.push(with_origin(row, ProbeIpOrigin::Baseline));
    }
    for row in measured {
        if baseline_ips.contains(&row.remote_ip) {
            continue;
        }
        rows.push(with_origin(row, ProbeIpOrigin::New));
    }

    Ok(ProbeDiffResult {
        prepare_ip_count,
        measure_ip_count,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    use crate::models::{NewConnection, NewIpDomain, NewProcess};
    use crate::writes::{insert_connection, insert_process, upsert_ip_domain};
    use crate::Database;

    fn t(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 6, 1, h, m, s).unwrap()
    }

    async fn seed_process(
        db: &Database,
        pid: u32,
        name: &str,
        path: &str,
        start: DateTime<Utc>,
    ) -> i64 {
        insert_process(
            db.pool(),
            &NewProcess {
                pid,
                process_name: Some(name.into()),
                executable_path: Some(path.into()),
                process_start_time: start,
                first_seen_at: start,
                last_seen_at: start,
            },
        )
        .await
        .expect("insert process")
    }

    async fn seed_conn(
        db: &Database,
        process_id: i64,
        remote_ip: &str,
        remote_port: u16,
        started_at: DateTime<Utc>,
        ended_at: Option<DateTime<Utc>>,
    ) {
        insert_connection(
            db.pool(),
            &NewConnection {
                process_id,
                protocol: "tcp".into(),
                direction: "outbound".into(),
                local_ip: "192.168.1.10".into(),
                local_port: 50_000 + remote_port,
                remote_ip: remote_ip.into(),
                remote_port,
                started_at,
                ended_at,
                bytes_sent: Some(1),
                bytes_received: Some(2),
                collector_id: Some("test".into()),
            },
        )
        .await
        .expect("insert connection");
    }

    #[test]
    fn app_key_collapses_bundle_helpers() {
        assert_eq!(
            app_key_from_path("/Applications/Safari.app/Contents/MacOS/Safari").as_deref(),
            Some("/Applications/Safari.app")
        );
        assert_eq!(
            app_key_from_path("/Applications/Safari.app/Contents/Helpers/web").as_deref(),
            Some("/Applications/Safari.app")
        );
        assert_eq!(
            app_key_from_path("/usr/bin/curl").as_deref(),
            Some("/usr/bin/curl")
        );
        assert_eq!(app_key_kind("/Applications/Safari.app"), AppKeyKind::Bundle);
        assert_eq!(app_key_kind("/usr/bin/curl"), AppKeyKind::Exact);
    }

    #[tokio::test]
    async fn diff_hides_shared_and_prepare_only_keeps_measure_only() {
        let db = Database::open_in_memory().await.expect("open");

        let safari = seed_process(
            &db,
            100,
            "Safari",
            "/Applications/Safari.app/Contents/MacOS/Safari",
            t(0, 0, 0),
        )
        .await;
        let helper = seed_process(
            &db,
            101,
            "Safari Helper",
            "/Applications/Safari.app/Contents/Helpers/XPC",
            t(0, 0, 1),
        )
        .await;
        let curl = seed_process(&db, 200, "curl", "/usr/bin/curl", t(0, 0, 2)).await;

        // Shared IP in prepare + measure (Safari)
        seed_conn(
            &db,
            safari,
            "203.0.113.10",
            443,
            t(10, 0, 0),
            Some(t(10, 0, 30)),
        )
        .await;
        seed_conn(
            &db,
            safari,
            "203.0.113.10",
            443,
            t(11, 0, 0),
            Some(t(11, 0, 30)),
        )
        .await;

        // Prepare-only
        seed_conn(
            &db,
            safari,
            "198.51.100.1",
            443,
            t(10, 1, 0),
            Some(t(10, 1, 10)),
        )
        .await;

        // Measure-only via Safari
        seed_conn(
            &db,
            safari,
            "192.0.2.50",
            443,
            t(11, 1, 0),
            Some(t(11, 1, 10)),
        )
        .await;

        // Measure-only via helper inside same .app — must count
        seed_conn(
            &db,
            helper,
            "192.0.2.60",
            443,
            t(11, 2, 0),
            Some(t(11, 2, 10)),
        )
        .await;

        // Other process measure-only — must NOT count
        seed_conn(
            &db,
            curl,
            "192.0.2.70",
            443,
            t(11, 3, 0),
            Some(t(11, 3, 10)),
        )
        .await;

        // Loopback in measure — excluded
        seed_conn(
            &db,
            safari,
            "127.0.0.1",
            8080,
            t(11, 4, 0),
            Some(t(11, 4, 10)),
        )
        .await;

        // Already-open socket overlapping prepare (started before prepare)
        seed_conn(&db, safari, "203.0.113.99", 443, t(9, 0, 0), None).await;

        upsert_ip_domain(
            db.pool(),
            &NewIpDomain {
                ip: "192.0.2.50".into(),
                domain: "new.example.com".into(),
                source: "REVERSE_DNS".into(),
                confidence: Some(0.9),
                seen_at: t(11, 1, 0),
            },
        )
        .await
        .expect("domain");

        let apps = list_probe_apps(db.pool(), t(10, 0, 0), t(10, 30, 0))
            .await
            .expect("apps");
        let safari_key = apps
            .iter()
            .find(|a| a.app_key == "/Applications/Safari.app")
            .expect("safari app in prepare window");
        assert!(safari_key.process_count >= 1);
        assert!(
            !apps.iter().any(|a| a.app_key == "/usr/bin/curl"),
            "curl only active in measure must not appear in prepare-window list"
        );

        let filtered = diff_probe(
            db.pool(),
            &ProbeDiffQuery {
                app_keys: vec!["/Applications/Safari.app".into()],
                prepare_start: t(10, 0, 0),
                prepare_end: t(10, 30, 0),
                measure_start: Some(t(11, 0, 0)),
                measure_end: Some(t(11, 30, 0)),
            },
        )
        .await
        .expect("diff");

        // Prepare: 203.0.113.10, 198.51.100.1, 203.0.113.99 (open overlap)
        assert_eq!(filtered.prepare_ip_count, 3);
        // Measure: 203.0.113.10, 192.0.2.50, 192.0.2.60, 203.0.113.99 (still open)
        assert_eq!(filtered.measure_ip_count, 4);

        let ips: Vec<&str> = filtered.rows.iter().map(|r| r.remote_ip.as_str()).collect();
        // Baseline stays in the list, in first-seen order; measure-only IPs follow it.
        assert_eq!(
            ips,
            vec![
                "203.0.113.99",
                "203.0.113.10",
                "198.51.100.1",
                "192.0.2.50",
                "192.0.2.60",
            ]
        );
        assert!(filtered
            .rows
            .iter()
            .take(3)
            .all(|row| row.origin == ProbeIpOrigin::Baseline));
        assert!(filtered
            .rows
            .iter()
            .skip(3)
            .all(|row| row.origin == ProbeIpOrigin::New));
        assert!(
            !ips.contains(&"192.0.2.70"),
            "other process hidden when filtered"
        );
        assert!(!ips.contains(&"127.0.0.1"), "loopback excluded");

        let new_row = filtered
            .rows
            .iter()
            .find(|r| r.remote_ip == "192.0.2.50")
            .expect("new row");
        assert_eq!(new_row.domains.as_deref(), Some("new.example.com"));
        assert!(new_row.ports.as_deref().unwrap_or("").contains("443"));
        assert!(new_row
            .process_names
            .as_deref()
            .unwrap_or("")
            .contains("Safari"));

        // Empty app_keys = all applications, so curl's measure-only IP appears.
        let all_apps = diff_probe(
            db.pool(),
            &ProbeDiffQuery {
                app_keys: vec![],
                prepare_start: t(10, 0, 0),
                prepare_end: t(10, 30, 0),
                measure_start: Some(t(11, 0, 0)),
                measure_end: Some(t(11, 30, 0)),
            },
        )
        .await
        .expect("diff all");
        let all_new: Vec<&str> = all_apps
            .rows
            .iter()
            .filter(|row| row.origin == ProbeIpOrigin::New)
            .map(|row| row.remote_ip.as_str())
            .collect();
        assert!(
            all_new.contains(&"192.0.2.70"),
            "unfiltered includes other process"
        );
    }

    #[tokio::test]
    async fn prepare_only_returns_baseline_count() {
        let db = Database::open_in_memory().await.expect("open");
        let curl = seed_process(&db, 1, "curl", "/usr/bin/curl", t(0, 0, 0)).await;
        seed_conn(&db, curl, "8.8.8.8", 53, t(10, 0, 0), Some(t(10, 0, 5))).await;

        let result = diff_probe(
            db.pool(),
            &ProbeDiffQuery {
                app_keys: vec![],
                prepare_start: t(10, 0, 0),
                prepare_end: t(10, 30, 0),
                measure_start: None,
                measure_end: None,
            },
        )
        .await
        .expect("diff");

        assert_eq!(result.prepare_ip_count, 1);
        assert_eq!(result.measure_ip_count, 0);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0].remote_ip, "8.8.8.8");
        assert_eq!(result.rows[0].origin, ProbeIpOrigin::Baseline);
    }
}
