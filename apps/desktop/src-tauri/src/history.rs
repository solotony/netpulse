//! Tauri commands for filtered connection history and entity list pages.

use chrono::{DateTime, Utc};
use database::{
    get_bool_setting, ConnectionHistoryFilter, ConnectionHistoryQuery, ConnectionHistoryRow,
    ConnectionSortColumn, DomainListRow, EntityListQuery, IpListRow, NetworkListRow, ProcessListRow,
    SortDirection, DEFAULT_PAGE_SIZE, SETTING_ASN_INFO, SETTING_COUNTRY_INFO,
};
use serde::{Deserialize, Serialize};

use crate::monitoring::MonitoringSession;
use std::sync::Arc;

async fn enrichment_display_flags(db: &database::Database) -> (bool, bool) {
    let asn = get_bool_setting(db.pool(), SETTING_ASN_INFO, true)
        .await
        .unwrap_or(true);
    let country = get_bool_setting(db.pool(), SETTING_COUNTRY_INFO, true)
        .await
        .unwrap_or(true);
    (asn, country)
}

/// Frontend-facing connection history row (camelCase).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTableRow {
    pub id: i64,
    pub process_id: i64,
    pub started_at: String,
    pub process_name: Option<String>,
    pub pid: i64,
    pub protocol: String,
    pub direction: String,
    pub local_ip: String,
    pub local_port: i64,
    pub remote_ip: String,
    pub remote_port: i64,
    pub domain: Option<String>,
    pub network: Option<String>,
    pub organization: Option<String>,
    pub ended_at: Option<String>,
    pub status: String,
}

impl From<ConnectionHistoryRow> for ConnectionTableRow {
    fn from(row: ConnectionHistoryRow) -> Self {
        let status = if row.ended_at.is_some() {
            "closed"
        } else {
            "active"
        };
        let network = row
            .network_name
            .filter(|s| !s.trim().is_empty())
            .or(row.network_cidr);
        Self {
            id: row.id,
            process_id: row.process_id,
            started_at: row.started_at.to_rfc3339(),
            process_name: row.process_name,
            pid: row.pid,
            protocol: row.protocol,
            direction: row.direction,
            local_ip: row.local_ip,
            local_port: row.local_port,
            remote_ip: row.remote_ip,
            remote_port: row.remote_port,
            domain: row.domain,
            network,
            organization: row.organization,
            ended_at: row.ended_at.map(|t| t.to_rfc3339()),
            status: status.to_owned(),
        }
    }
}

/// Paginated connection history response.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsPage {
    pub rows: Vec<ConnectionTableRow>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

/// Optional filters accepted from the Vue Connections page.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFilterDto {
    pub text: Option<String>,
    pub process_id: Option<i64>,
    pub process_name: Option<String>,
    pub remote_ip: Option<String>,
    pub domain: Option<String>,
    pub network: Option<String>,
    pub protocol: Option<String>,
    pub started_from: Option<String>,
    pub started_to: Option<String>,
    pub active_only: Option<bool>,
}

/// Sort + page request for connection history.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQueryDto {
    #[serde(default)]
    pub filter: HistoryFilterDto,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Parses an optional RFC3339 / datetime-local string into UTC.
pub(crate) fn parse_rfc3339(value: Option<&str>) -> Result<Option<DateTime<Utc>>, String> {
    match value.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(raw) => DateTime::parse_from_rfc3339(raw)
            .map(|dt| Some(dt.with_timezone(&Utc)))
            .or_else(|_| {
                // Accept datetime-local style `YYYY-MM-DDTHH:MM` / `YYYY-MM-DDTHH:MM:SS`.
                let padded = if raw.len() == 16 {
                    format!("{raw}:00Z")
                } else if raw.len() == 19 && !raw.ends_with('Z') {
                    format!("{raw}Z")
                } else if !raw.ends_with('Z') && !raw.contains('+') {
                    format!("{raw}Z")
                } else {
                    raw.to_owned()
                };
                DateTime::parse_from_rfc3339(&padded)
                    .map(|dt| Some(dt.with_timezone(&Utc)))
                    .map_err(|err| format!("invalid datetime '{raw}': {err}"))
            }),
    }
}

fn parse_sort_column(raw: Option<&str>) -> ConnectionSortColumn {
    match raw.map(str::trim).map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("endedat") | Some("ended_at") => ConnectionSortColumn::EndedAt,
        Some("process") | Some("processname") | Some("process_name") => {
            ConnectionSortColumn::ProcessName
        }
        Some("pid") => ConnectionSortColumn::Pid,
        Some("protocol") => ConnectionSortColumn::Protocol,
        Some("direction") => ConnectionSortColumn::Direction,
        Some("local") | Some("localip") | Some("local_ip") => ConnectionSortColumn::LocalIp,
        Some("localport") | Some("local_port") => ConnectionSortColumn::LocalPort,
        Some("remote") | Some("remoteip") | Some("remote_ip") => ConnectionSortColumn::RemoteIp,
        Some("remoteport") | Some("remote_port") => ConnectionSortColumn::RemotePort,
        Some("domain") => ConnectionSortColumn::Domain,
        Some("network") => ConnectionSortColumn::Network,
        Some("organization") => ConnectionSortColumn::Organization,
        Some("id") => ConnectionSortColumn::Id,
        _ => ConnectionSortColumn::StartedAt,
    }
}

fn parse_sort_dir(raw: Option<&str>) -> SortDirection {
    match raw.map(str::trim).map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("asc") => SortDirection::Asc,
        _ => SortDirection::Desc,
    }
}

fn to_history_query(dto: HistoryQueryDto) -> Result<ConnectionHistoryQuery, String> {
    let page = dto.page.unwrap_or(1).max(1);
    let page_size = dto
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, database::MAX_PAGE_SIZE);
    let offset = u64::from((page - 1) * page_size);

    let filter = ConnectionHistoryFilter {
        text: dto.filter.text,
        process_id: dto.filter.process_id,
        process_name: dto.filter.process_name,
        remote_ip: dto.filter.remote_ip,
        domain: dto.filter.domain,
        network: dto.filter.network,
        protocol: dto.filter.protocol,
        started_from: parse_rfc3339(dto.filter.started_from.as_deref())?,
        started_to: parse_rfc3339(dto.filter.started_to.as_deref())?,
        active_only: dto.filter.active_only.unwrap_or(false),
    };

    Ok(ConnectionHistoryQuery {
        filter,
        sort_by: parse_sort_column(dto.sort_by.as_deref()),
        sort_dir: parse_sort_dir(dto.sort_dir.as_deref()),
        limit: page_size,
        offset,
    })
}

/// Queries connection history with filters, sort, and pagination.
#[tauri::command]
pub async fn query_connection_history(
    app: tauri::AppHandle,
    query: HistoryQueryDto,
) -> Result<ConnectionsPage, String> {
    use tauri::Manager;

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, database::MAX_PAGE_SIZE);
    let history_query = to_history_query(query)?;

    let session = app.state::<Arc<MonitoringSession>>();
    let result = session
        .database()
        .query_connection_history(&history_query)
        .await
        .map_err(|err| format!("query connection history: {err}"))?;

    Ok(ConnectionsPage {
        rows: result.rows.into_iter().map(ConnectionTableRow::from).collect(),
        page,
        page_size,
        total: result.total,
    })
}

/// Optional text + page request for entity tables.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityQueryDto {
    pub text: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

fn to_entity_query(dto: EntityQueryDto) -> (EntityListQuery, u32, u32) {
    let page = dto.page.unwrap_or(1).max(1);
    let page_size = dto
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, database::MAX_PAGE_SIZE);
    let query = EntityListQuery {
        text: dto.text,
        limit: page_size,
        offset: u64::from((page - 1) * page_size),
    };
    (query, page, page_size)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRowDto {
    pub id: i64,
    pub process_name: Option<String>,
    pub pid: i64,
    pub executable_path: Option<String>,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub total_connections: i64,
    pub unique_remote_ips: i64,
    pub unique_domains: i64,
    pub bytes_sent: i64,
    pub bytes_received: i64,
}

impl From<ProcessListRow> for ProcessRowDto {
    fn from(row: ProcessListRow) -> Self {
        Self {
            id: row.id,
            process_name: row.process_name,
            pid: row.pid,
            executable_path: row.executable_path,
            first_seen_at: row.first_seen_at.to_rfc3339(),
            last_seen_at: row.last_seen_at.to_rfc3339(),
            total_connections: row.total_connections,
            unique_remote_ips: row.unique_remote_ips,
            unique_domains: row.unique_domains,
            bytes_sent: row.bytes_sent,
            bytes_received: row.bytes_received,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessPageDto {
    pub rows: Vec<ProcessRowDto>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

#[tauri::command]
pub async fn list_processes(
    app: tauri::AppHandle,
    query: Option<EntityQueryDto>,
) -> Result<ProcessPageDto, String> {
    use tauri::Manager;

    let (entity_query, page, page_size) = to_entity_query(query.unwrap_or_default());
    let session = app.state::<Arc<MonitoringSession>>();
    let result = session
        .database()
        .list_processes(&entity_query)
        .await
        .map_err(|err| format!("list processes: {err}"))?;

    Ok(ProcessPageDto {
        rows: result.rows.into_iter().map(ProcessRowDto::from).collect(),
        page,
        page_size,
        total: result.total,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpRowDto {
    pub ip: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub connection_count: i64,
    pub process_count: i64,
    pub domain_count: i64,
    pub cidr: Option<String>,
    pub net_name: Option<String>,
    pub asn: Option<i64>,
    pub organization: Option<String>,
    pub country: Option<String>,
}

impl From<IpListRow> for IpRowDto {
    fn from(row: IpListRow) -> Self {
        Self {
            ip: row.ip,
            first_seen_at: row.first_seen_at.to_rfc3339(),
            last_seen_at: row.last_seen_at.to_rfc3339(),
            connection_count: row.connection_count,
            process_count: row.process_count,
            domain_count: row.domain_count,
            cidr: row.cidr,
            net_name: row.net_name,
            asn: row.asn,
            organization: row.organization,
            country: row.country,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpPageDto {
    pub rows: Vec<IpRowDto>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

#[tauri::command]
pub async fn list_ip_addresses(
    app: tauri::AppHandle,
    query: Option<EntityQueryDto>,
) -> Result<IpPageDto, String> {
    use tauri::Manager;

    let (entity_query, page, page_size) = to_entity_query(query.unwrap_or_default());
    let session = app.state::<Arc<MonitoringSession>>();
    let (show_asn, show_country) = enrichment_display_flags(session.database()).await;
    let result = session
        .database()
        .list_ip_addresses(&entity_query)
        .await
        .map_err(|err| format!("list ip addresses: {err}"))?;

    Ok(IpPageDto {
        rows: result
            .rows
            .into_iter()
            .map(|row| {
                let mut dto = IpRowDto::from(row);
                if !show_asn {
                    dto.asn = None;
                }
                if !show_country {
                    dto.country = None;
                }
                dto
            })
            .collect(),
        page,
        page_size,
        total: result.total,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainRowDto {
    pub id: i64,
    pub domain: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub ip_count: i64,
    pub process_count: i64,
    pub connection_count: i64,
    pub sources: Option<String>,
}

impl From<DomainListRow> for DomainRowDto {
    fn from(row: DomainListRow) -> Self {
        Self {
            id: row.id,
            domain: row.domain,
            first_seen_at: row.first_seen_at.to_rfc3339(),
            last_seen_at: row.last_seen_at.to_rfc3339(),
            ip_count: row.ip_count,
            process_count: row.process_count,
            connection_count: row.connection_count,
            sources: row.sources,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainPageDto {
    pub rows: Vec<DomainRowDto>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

#[tauri::command]
pub async fn list_domains(
    app: tauri::AppHandle,
    query: Option<EntityQueryDto>,
) -> Result<DomainPageDto, String> {
    use tauri::Manager;

    let (entity_query, page, page_size) = to_entity_query(query.unwrap_or_default());
    let session = app.state::<Arc<MonitoringSession>>();
    let result = session
        .database()
        .list_domains(&entity_query)
        .await
        .map_err(|err| format!("list domains: {err}"))?;

    Ok(DomainPageDto {
        rows: result.rows.into_iter().map(DomainRowDto::from).collect(),
        page,
        page_size,
        total: result.total,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkRowDto {
    pub id: i64,
    pub cidr: String,
    pub net_range: Option<String>,
    pub net_name: Option<String>,
    pub asn: Option<i64>,
    pub organization: Option<String>,
    pub country: Option<String>,
    pub ip_count: i64,
    pub connection_count: i64,
    pub first_seen_at: Option<String>,
    pub last_seen_at: Option<String>,
}

impl From<NetworkListRow> for NetworkRowDto {
    fn from(row: NetworkListRow) -> Self {
        Self {
            id: row.id,
            cidr: row.cidr,
            net_range: row.net_range,
            net_name: row.net_name,
            asn: row.asn,
            organization: row.organization,
            country: row.country,
            ip_count: row.ip_count,
            connection_count: row.connection_count,
            first_seen_at: row.first_seen_at.map(|t| t.to_rfc3339()),
            last_seen_at: row.last_seen_at.map(|t| t.to_rfc3339()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPageDto {
    pub rows: Vec<NetworkRowDto>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

#[tauri::command]
pub async fn list_networks(
    app: tauri::AppHandle,
    query: Option<EntityQueryDto>,
) -> Result<NetworkPageDto, String> {
    use tauri::Manager;

    let (entity_query, page, page_size) = to_entity_query(query.unwrap_or_default());
    let session = app.state::<Arc<MonitoringSession>>();
    let (show_asn, show_country) = enrichment_display_flags(session.database()).await;
    let result = session
        .database()
        .list_networks_page(&entity_query)
        .await
        .map_err(|err| format!("list networks: {err}"))?;

    Ok(NetworkPageDto {
        rows: result
            .rows
            .into_iter()
            .map(|row| {
                let mut dto = NetworkRowDto::from(row);
                if !show_asn {
                    dto.asn = None;
                }
                if !show_country {
                    dto.country = None;
                }
                dto
            })
            .collect(),
        page,
        page_size,
        total: result.total,
    })
}
