//! Tauri commands for streaming TXT/TSV connection-history export.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use database::{ConnectionHistoryFilter, ExportFormat, ExportOptions, EXPORT_CHUNK_SIZE};
use serde::{Deserialize, Serialize};

use crate::history::{parse_rfc3339, HistoryFilterDto};
use crate::monitoring::MonitoringSession;

/// Export format accepted from the frontend.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormatDto {
    Txt,
    Tsv,
}

impl From<ExportFormatDto> for ExportFormat {
    fn from(value: ExportFormatDto) -> Self {
        match value {
            ExportFormatDto::Txt => ExportFormat::Txt,
            ExportFormatDto::Tsv => ExportFormat::Tsv,
        }
    }
}

/// Request body for `export_connection_history`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequestDto {
    pub format: ExportFormatDto,
    #[serde(default)]
    pub filter: HistoryFilterDto,
    /// When set and non-empty, only these connection ids are exported.
    pub selected_ids: Option<Vec<i64>>,
    /// Optional destination path. When omitted, a native save dialog is shown.
    pub path: Option<String>,
}

/// Result returned to the UI after a successful export.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultDto {
    pub path: String,
    pub rows_written: u64,
    pub cancelled: bool,
}

fn to_filter(dto: HistoryFilterDto) -> Result<ConnectionHistoryFilter, String> {
    Ok(ConnectionHistoryFilter {
        text: dto.text,
        process_id: dto.process_id,
        process_name: dto.process_name,
        remote_ip: dto.remote_ip,
        domain: dto.domain,
        network: dto.network,
        protocol: dto.protocol,
        started_from: parse_rfc3339(dto.started_from.as_deref())?,
        started_to: parse_rfc3339(dto.started_to.as_deref())?,
        active_only: dto.active_only.unwrap_or(false),
    })
}

fn default_file_name(format: &ExportFormatDto) -> String {
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    match format {
        ExportFormatDto::Txt => format!("network-monitor-export-{stamp}.txt"),
        ExportFormatDto::Tsv => format!("network-monitor-export-{stamp}.tsv"),
    }
}

fn pick_save_path(format: &ExportFormatDto, suggested: &str) -> Option<PathBuf> {
    let (name, ext) = match format {
        ExportFormatDto::Txt => ("Plain text", "txt"),
        ExportFormatDto::Tsv => ("Tab-separated", "tsv"),
    };
    rfd::FileDialog::new()
        .set_file_name(suggested)
        .add_filter(name, &[ext])
        .add_filter("All files", &["*"])
        .save_file()
}

/// Streams matching connection history to a TXT or TSV file on disk.
#[tauri::command]
pub async fn export_connection_history(
    app: tauri::AppHandle,
    request: ExportRequestDto,
) -> Result<ExportResultDto, String> {
    use tauri::Manager;

    let filter = to_filter(request.filter)?;
    let selected_ids = request.selected_ids.filter(|ids| !ids.is_empty());
    let format = ExportFormat::from(request.format.clone());
    let suggested = default_file_name(&request.format);

    let path = if let Some(raw) = request.path.filter(|p| !p.trim().is_empty()) {
        PathBuf::from(raw)
    } else {
        let format_for_dialog = request.format.clone();
        let suggested_name = suggested.clone();
        let picked = tauri::async_runtime::spawn_blocking(move || {
            pick_save_path(&format_for_dialog, &suggested_name)
        })
        .await
        .map_err(|err| format!("save dialog join: {err}"))?;

        match picked {
            Some(path) => path,
            None => {
                return Ok(ExportResultDto {
                    path: String::new(),
                    rows_written: 0,
                    cancelled: true,
                });
            }
        }
    };

    // Ensure the extension matches the chosen format when the user omits it.
    let path = match (format, path.extension().and_then(|e| e.to_str())) {
        (ExportFormat::Txt, None) => path.with_extension("txt"),
        (ExportFormat::Tsv, None) => path.with_extension("tsv"),
        _ => path,
    };

    let options = ExportOptions {
        filter,
        selected_ids,
        format,
        export_date: Utc::now(),
        chunk_size: EXPORT_CHUNK_SIZE,
    };

    let session = app.state::<Arc<MonitoringSession>>();
    let summary = session
        .database()
        .export_connection_history_to_path(&options, &path)
        .await
        .map_err(|err| format!("export failed: {err}"))?;

    Ok(ExportResultDto {
        path: path.display().to_string(),
        rows_written: summary.rows_written,
        cancelled: false,
    })
}
