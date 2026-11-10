//! Tauri commands for the Prepare / Measure probe page.

use chrono::{DateTime, Utc};
use database::{
    diff_probe as query_diff_probe, list_probe_apps, ProbeAppRow, ProbeDiffQuery, ProbeDiffRow,
    ProbeIpOrigin,
};
use serde::{Deserialize, Serialize};

use crate::history::parse_rfc3339;
use crate::monitoring::MonitoringSession;
use std::sync::Arc;

/// One application available for probe selection.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeAppDto {
    pub app_key: String,
    pub display_name: Option<String>,
    pub process_count: i64,
    pub last_seen_at: String,
}

impl From<ProbeAppRow> for ProbeAppDto {
    fn from(row: ProbeAppRow) -> Self {
        Self {
            app_key: row.app_key,
            display_name: row.display_name,
            process_count: row.process_count,
            last_seen_at: row.last_seen_at.to_rfc3339(),
        }
    }
}

/// `baseline` IPs were seen while preparing; `new` IPs appeared only while measuring.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProbeIpOriginDto {
    Baseline,
    New,
}

impl From<ProbeIpOrigin> for ProbeIpOriginDto {
    fn from(origin: ProbeIpOrigin) -> Self {
        match origin {
            ProbeIpOrigin::Baseline => Self::Baseline,
            ProbeIpOrigin::New => Self::New,
        }
    }
}

/// One remote IP from the prepare window, or a measure-only IP.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeDiffRowDto {
    pub remote_ip: String,
    pub domains: Option<String>,
    pub ports: Option<String>,
    pub process_names: Option<String>,
    pub first_seen: String,
    pub origin: ProbeIpOriginDto,
}

impl From<ProbeDiffRow> for ProbeDiffRowDto {
    fn from(row: ProbeDiffRow) -> Self {
        Self {
            remote_ip: row.remote_ip,
            domains: row.domains,
            ports: row.ports,
            process_names: row.process_names,
            first_seen: row.first_seen.to_rfc3339(),
            origin: ProbeIpOriginDto::from(row.origin),
        }
    }
}

/// Probe diff response for the Vue page.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeDiffResultDto {
    pub prepare_ip_count: u64,
    pub measure_ip_count: u64,
    pub rows: Vec<ProbeDiffRowDto>,
}

/// Request body for `list_probe_applications`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeAppsRequestDto {
    pub window_start: String,
    pub window_end: String,
}

/// Request body for `diff_probe`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeDiffRequestDto {
    pub app_keys: Vec<String>,
    pub prepare_start: String,
    pub prepare_end: String,
    pub measure_start: Option<String>,
    pub measure_end: Option<String>,
}

fn require_rfc3339(label: &str, value: &str) -> Result<DateTime<Utc>, String> {
    parse_rfc3339(Some(value))?.ok_or_else(|| format!("{label} is required"))
}

/// Lists applications with network activity inside the given probe window.
#[tauri::command]
pub async fn list_probe_applications(
    app: tauri::AppHandle,
    request: ProbeAppsRequestDto,
) -> Result<Vec<ProbeAppDto>, String> {
    use tauri::Manager;

    let window_start = require_rfc3339("windowStart", &request.window_start)?;
    let window_end = require_rfc3339("windowEnd", &request.window_end)?;

    let session = app.state::<Arc<MonitoringSession>>();
    let rows = list_probe_apps(session.database().pool(), window_start, window_end)
        .await
        .map_err(|err| format!("list probe apps: {err}"))?;
    Ok(rows.into_iter().map(ProbeAppDto::from).collect())
}

/// Returns baseline remote IPs, and measure-only IPs once a measure window is set.
///
/// Empty `appKeys` means every application.
#[tauri::command]
pub async fn diff_probe(
    app: tauri::AppHandle,
    request: ProbeDiffRequestDto,
) -> Result<ProbeDiffResultDto, String> {
    use tauri::Manager;

    let prepare_start = require_rfc3339("prepareStart", &request.prepare_start)?;
    let prepare_end = require_rfc3339("prepareEnd", &request.prepare_end)?;
    let measure_start = match request.measure_start.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(require_rfc3339("measureStart", raw)?),
    };
    let measure_end = match request.measure_end.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(require_rfc3339("measureEnd", raw)?),
    };

    let app_keys: Vec<String> = request
        .app_keys
        .into_iter()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .collect();

    let session = app.state::<Arc<MonitoringSession>>();
    let result = query_diff_probe(
        session.database().pool(),
        &ProbeDiffQuery {
            app_keys,
            prepare_start,
            prepare_end,
            measure_start,
            measure_end,
        },
    )
    .await
    .map_err(|err| format!("diff probe: {err}"))?;

    Ok(ProbeDiffResultDto {
        prepare_ip_count: result.prepare_ip_count,
        measure_ip_count: result.measure_ip_count,
        rows: result.rows.into_iter().map(ProbeDiffRowDto::from).collect(),
    })
}
