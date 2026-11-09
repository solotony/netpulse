//! Tauri commands for statistics aggregations (tables only).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use statistics::{DateRange, TimeBucket};
use std::sync::Arc;

use crate::monitoring::MonitoringSession;

/// Supported statistics groupings for the Statistics page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StatsGrouping {
    Process,
    Ip,
    Domain,
    Network,
    Organization,
    Day,
    Week,
    Month,
}

/// Request payload for `query_statistics`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsQueryDto {
    pub group_by: StatsGrouping,
    pub from: String,
    pub to: String,
}

/// One statistics table row (label + three metric columns).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatsRowDto {
    pub label: String,
    pub connections: i64,
    pub metric_a: i64,
    pub metric_b: i64,
}

/// Statistics table with column headers matching the selected grouping.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatsTableDto {
    pub group_by: StatsGrouping,
    pub columns: [String; 4],
    pub rows: Vec<StatsRowDto>,
}

fn parse_bound(raw: &str) -> Result<DateTime<Utc>, String> {
    let trimmed = raw.trim();
    DateTime::parse_from_rfc3339(trimmed)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            let padded = if trimmed.len() == 10 {
                // date-only → start/end of day handled by caller padding; accept midnight UTC
                format!("{trimmed}T00:00:00Z")
            } else if trimmed.len() == 16 {
                format!("{trimmed}:00Z")
            } else if trimmed.len() == 19 && !trimmed.ends_with('Z') {
                format!("{trimmed}Z")
            } else if !trimmed.ends_with('Z') && !trimmed.contains('+') {
                format!("{trimmed}Z")
            } else {
                trimmed.to_owned()
            };
            DateTime::parse_from_rfc3339(&padded)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|err| format!("invalid datetime '{raw}': {err}"))
        })
}

fn columns_for(group: StatsGrouping) -> [String; 4] {
    match group {
        StatsGrouping::Process => [
            "Process".into(),
            "Connections".into(),
            "Unique IPs".into(),
            "Domains".into(),
        ],
        StatsGrouping::Ip => [
            "IP".into(),
            "Connections".into(),
            "Unique processes".into(),
            "Domains".into(),
        ],
        StatsGrouping::Domain => [
            "Domain".into(),
            "Connections".into(),
            "Unique IPs".into(),
            "Unique processes".into(),
        ],
        StatsGrouping::Network => [
            "Network".into(),
            "Connections".into(),
            "Unique IPs".into(),
            "Unique processes".into(),
        ],
        StatsGrouping::Organization => [
            "Organization".into(),
            "Connections".into(),
            "Unique IPs".into(),
            "Unique processes".into(),
        ],
        StatsGrouping::Day | StatsGrouping::Week | StatsGrouping::Month => [
            "Period".into(),
            "Connections".into(),
            "Unique IPs".into(),
            "Domains".into(),
        ],
    }
}

/// Runs a statistics aggregation for the given grouping and inclusive date range.
#[tauri::command]
pub async fn query_statistics(
    app: tauri::AppHandle,
    query: StatsQueryDto,
) -> Result<StatsTableDto, String> {
    use tauri::Manager;

    let from = parse_bound(&query.from)?;
    let mut to = parse_bound(&query.to)?;
    // If the user picked a bare date for `to`, treat it as end-of-day inclusive.
    if query.to.trim().len() == 10 {
        to = to + chrono::Duration::days(1) - chrono::Duration::nanoseconds(1);
    }

    let range = DateRange { from, to };
    let session = app.state::<Arc<MonitoringSession>>();
    let db = session.database();
    let columns = columns_for(query.group_by);

    let rows = match query.group_by {
        StatsGrouping::Process => statistics::process_stats(db, range)
            .await
            .map_err(|err| format!("process stats: {err}"))?
            .into_iter()
            .map(|r| StatsRowDto {
                label: r.process,
                connections: r.connections,
                metric_a: r.unique_ips,
                metric_b: r.domains,
            })
            .collect(),
        StatsGrouping::Ip => statistics::ip_stats(db, range)
            .await
            .map_err(|err| format!("ip stats: {err}"))?
            .into_iter()
            .map(|r| StatsRowDto {
                label: r.ip,
                connections: r.connections,
                metric_a: r.unique_processes,
                metric_b: r.domains,
            })
            .collect(),
        StatsGrouping::Domain => statistics::domain_stats(db, range)
            .await
            .map_err(|err| format!("domain stats: {err}"))?
            .into_iter()
            .map(|r| StatsRowDto {
                label: r.domain,
                connections: r.connections,
                metric_a: r.unique_ips,
                metric_b: r.unique_processes,
            })
            .collect(),
        StatsGrouping::Network => statistics::network_stats(db, range)
            .await
            .map_err(|err| format!("network stats: {err}"))?
            .into_iter()
            .map(|r| StatsRowDto {
                label: r.network,
                connections: r.connections,
                metric_a: r.unique_ips,
                metric_b: r.unique_processes,
            })
            .collect(),
        StatsGrouping::Organization => statistics::organization_stats(db, range)
            .await
            .map_err(|err| format!("organization stats: {err}"))?
            .into_iter()
            .map(|r| StatsRowDto {
                label: r.organization,
                connections: r.connections,
                metric_a: r.unique_ips,
                metric_b: r.unique_processes,
            })
            .collect(),
        StatsGrouping::Day => statistics::time_stats(db, range, TimeBucket::Day)
            .await
            .map_err(|err| format!("day stats: {err}"))?
            .into_iter()
            .map(|r| StatsRowDto {
                label: r.period,
                connections: r.connections,
                metric_a: r.unique_ips,
                metric_b: r.domains,
            })
            .collect(),
        StatsGrouping::Week => statistics::time_stats(db, range, TimeBucket::Week)
            .await
            .map_err(|err| format!("week stats: {err}"))?
            .into_iter()
            .map(|r| StatsRowDto {
                label: r.period,
                connections: r.connections,
                metric_a: r.unique_ips,
                metric_b: r.domains,
            })
            .collect(),
        StatsGrouping::Month => statistics::time_stats(db, range, TimeBucket::Month)
            .await
            .map_err(|err| format!("month stats: {err}"))?
            .into_iter()
            .map(|r| StatsRowDto {
                label: r.period,
                connections: r.connections,
                metric_a: r.unique_ips,
                metric_b: r.domains,
            })
            .collect(),
    };

    Ok(StatsTableDto {
        group_by: query.group_by,
        columns,
        rows,
    })
}
