//! Manual domain / IP / CIDR entry and group CRUD commands.

use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use common::normalize_domain;
use database::{
    create_group, delete_group, get_group, get_ip_winning_network, list_groups as db_list_groups,
    rename_group, set_network_group, set_networks_group_by_cidr, upsert_ip_address,
    upsert_ip_domain, upsert_network, GroupListRow, GroupNetworkRow, NewIpDomain, NewNetwork,
};
use dns_resolver::{DomainSource, FORWARD_DNS_CONFIDENCE};
use ip_enrichment::parse_cidr;
use serde::{Deserialize, Serialize};

use crate::monitoring::MonitoringSession;

/// Kind of manual entry from the UI form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ManualEntryKind {
    Domain,
    Ip,
    Cidr,
}

/// Request body for `add_manual_entry`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddManualEntryRequest {
    pub kind: ManualEntryKind,
    pub value: String,
    /// Existing group id, if choosing from the dropdown.
    pub group_id: Option<i64>,
    /// New group name (created when non-empty and `group_id` is absent).
    pub group_name: Option<String>,
}

/// Summary returned after a successful manual add.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualEntryResult {
    pub kind: String,
    pub value: String,
    pub addresses: Vec<String>,
    pub cidrs: Vec<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRowDto {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub network_count: i64,
}

impl From<GroupListRow> for GroupRowDto {
    fn from(row: GroupListRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            created_at: row.created_at.to_rfc3339(),
            network_count: row.network_count,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupNetworkDto {
    pub id: i64,
    pub cidr: String,
    pub net_name: Option<String>,
    pub organization: Option<String>,
}

impl From<GroupNetworkRow> for GroupNetworkDto {
    fn from(row: GroupNetworkRow) -> Self {
        Self {
            id: row.id,
            cidr: row.cidr,
            net_name: row.net_name,
            organization: row.organization,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupRequest {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameGroupRequest {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetNetworkGroupRequest {
    pub network_id: i64,
    pub group_id: Option<i64>,
}

async fn resolve_group(
    session: &MonitoringSession,
    group_id: Option<i64>,
    group_name: Option<&str>,
) -> Result<(Option<i64>, Option<String>), String> {
    if let Some(id) = group_id {
        let row = get_group(session.database().pool(), id)
            .await
            .map_err(|err| format!("load group: {err}"))?
            .ok_or_else(|| format!("group {id} not found"))?;
        return Ok((Some(row.id), Some(row.name)));
    }

    let name = group_name.map(str::trim).filter(|s| !s.is_empty());
    if let Some(name) = name {
        let id = create_group(session.database().pool(), name, Utc::now())
            .await
            .map_err(|err| format!("create group: {err}"))?;
        let row = get_group(session.database().pool(), id)
            .await
            .map_err(|err| format!("load group: {err}"))?
            .ok_or_else(|| "created group missing".to_owned())?;
        return Ok((Some(row.id), Some(row.name)));
    }

    Ok((None, None))
}

async fn enrich_and_collect_cidrs(
    session: &MonitoringSession,
    ips: &[IpAddr],
    notes: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    let mut cidrs = Vec::new();
    for ip in ips {
        let ok = session.enrichment().enqueue_and_wait_forced(*ip).await;
        if !ok {
            notes.push(format!("enrichment queue unavailable for {ip}"));
            continue;
        }

        let ip_str = ip.to_string();
        let ip_id = upsert_ip_address(session.database().pool(), &ip_str, Utc::now())
            .await
            .map_err(|err| format!("upsert ip: {err}"))?;

        match get_ip_winning_network(session.database().pool(), ip_id).await {
            Ok(Some(network)) => {
                if !cidrs.iter().any(|c| c == &network.cidr) {
                    cidrs.push(network.cidr);
                }
            }
            Ok(None) => notes.push(format!("no subnet found for {ip}")),
            Err(err) => notes.push(format!("load subnet for {ip}: {err}")),
        }
    }
    Ok(cidrs)
}

#[tauri::command]
pub async fn add_manual_entry(
    app: tauri::AppHandle,
    request: AddManualEntryRequest,
) -> Result<ManualEntryResult, String> {
    use tauri::Manager;

    let session = app.state::<Arc<MonitoringSession>>();
    let value = request.value.trim();
    if value.is_empty() {
        return Err("value is empty".into());
    }

    let (group_id, group_name) =
        resolve_group(session.as_ref(), request.group_id, request.group_name.as_deref()).await?;

    let mut notes = Vec::new();
    let mut addresses = Vec::new();
    let mut cidrs = Vec::new();
    let kind_label;
    let normalized_value;

    match request.kind {
        ManualEntryKind::Domain => {
            kind_label = "domain";
            let domain = normalize_domain(value).map_err(|err| err.to_string())?;
            normalized_value = domain.clone();

            let looked_up = session
                .forward_resolver()
                .lookup_addresses(&domain)
                .await
                .map_err(|err| format!("forward DNS: {err}"))?;

            if looked_up.is_empty() {
                notes.push("no A/AAAA records found".into());
                // Still persist the domain with zero IPs.
                database::upsert_domain(session.database().pool(), &domain, Utc::now())
                    .await
                    .map_err(|err| format!("upsert domain: {err}"))?;
            } else {
                for ip in &looked_up {
                    let ip_str = ip.to_string();
                    upsert_ip_domain(
                        session.database().pool(),
                        &NewIpDomain {
                            ip: ip_str.clone(),
                            domain: domain.clone(),
                            source: DomainSource::ForwardDns.as_str().into(),
                            confidence: Some(FORWARD_DNS_CONFIDENCE),
                            seen_at: Utc::now(),
                        },
                    )
                    .await
                    .map_err(|err| format!("link domain to ip: {err}"))?;
                    addresses.push(ip_str);
                }
                cidrs = enrich_and_collect_cidrs(session.as_ref(), &looked_up, &mut notes).await?;
            }
        }
        ManualEntryKind::Ip => {
            kind_label = "ip";
            let ip = IpAddr::from_str(value).map_err(|err| format!("invalid IP: {err}"))?;
            normalized_value = ip.to_string();
            upsert_ip_address(session.database().pool(), &normalized_value, Utc::now())
                .await
                .map_err(|err| format!("upsert ip: {err}"))?;
            addresses.push(normalized_value.clone());
            cidrs = enrich_and_collect_cidrs(session.as_ref(), &[ip], &mut notes).await?;
        }
        ManualEntryKind::Cidr => {
            kind_label = "cidr";
            let net = parse_cidr(value).map_err(|err| err.to_string())?;
            normalized_value = net.to_string();
            let network_id = upsert_network(
                session.database().pool(),
                &NewNetwork {
                    cidr: normalized_value.clone(),
                    net_range: None,
                    net_name: None,
                    organization: None,
                    asn: None,
                    country: None,
                    source: Some("manual".into()),
                    updated_at: Utc::now(),
                },
            )
            .await
            .map_err(|err| format!("upsert network: {err}"))?;

            if let Some(gid) = group_id {
                set_network_group(session.database().pool(), network_id, Some(gid))
                    .await
                    .map_err(|err| format!("assign group: {err}"))?;
            }
            cidrs.push(normalized_value.clone());
        }
    }

    if request.kind != ManualEntryKind::Cidr {
        if let Some(gid) = group_id {
            if !cidrs.is_empty() {
                set_networks_group_by_cidr(session.database().pool(), &cidrs, Some(gid))
                    .await
                    .map_err(|err| format!("assign group to subnets: {err}"))?;
            }
        }
    }

    Ok(ManualEntryResult {
        kind: kind_label.into(),
        value: normalized_value,
        addresses,
        cidrs,
        group_id,
        group_name,
        notes,
    })
}

#[tauri::command]
pub async fn list_groups(app: tauri::AppHandle) -> Result<Vec<GroupRowDto>, String> {
    use tauri::Manager;

    let session = app.state::<Arc<MonitoringSession>>();
    let rows = db_list_groups(session.database().pool())
        .await
        .map_err(|err| format!("list groups: {err}"))?;
    Ok(rows.into_iter().map(GroupRowDto::from).collect())
}

#[tauri::command]
pub async fn list_group_networks(
    app: tauri::AppHandle,
    group_id: i64,
) -> Result<Vec<GroupNetworkDto>, String> {
    use tauri::Manager;

    let session = app.state::<Arc<MonitoringSession>>();
    let rows = session
        .database()
        .list_networks_for_group(group_id)
        .await
        .map_err(|err| format!("list group networks: {err}"))?;
    Ok(rows.into_iter().map(GroupNetworkDto::from).collect())
}

#[tauri::command]
pub async fn create_group_cmd(
    app: tauri::AppHandle,
    request: CreateGroupRequest,
) -> Result<GroupRowDto, String> {
    use tauri::Manager;

    let session = app.state::<Arc<MonitoringSession>>();
    let id = create_group(session.database().pool(), &request.name, Utc::now())
        .await
        .map_err(|err| format!("create group: {err}"))?;
    let row = get_group(session.database().pool(), id)
        .await
        .map_err(|err| format!("load group: {err}"))?
        .ok_or_else(|| "created group missing".to_owned())?;
    Ok(GroupRowDto {
        id: row.id,
        name: row.name,
        created_at: row.created_at.to_rfc3339(),
        network_count: 0,
    })
}

#[tauri::command]
pub async fn rename_group_cmd(
    app: tauri::AppHandle,
    request: RenameGroupRequest,
) -> Result<(), String> {
    use tauri::Manager;

    let session = app.state::<Arc<MonitoringSession>>();
    rename_group(session.database().pool(), request.id, &request.name)
        .await
        .map_err(|err| format!("rename group: {err}"))
}

#[tauri::command]
pub async fn delete_group_cmd(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    use tauri::Manager;

    let session = app.state::<Arc<MonitoringSession>>();
    delete_group(session.database().pool(), id)
        .await
        .map_err(|err| format!("delete group: {err}"))
}

#[tauri::command]
pub async fn set_network_group_cmd(
    app: tauri::AppHandle,
    request: SetNetworkGroupRequest,
) -> Result<(), String> {
    use tauri::Manager;

    let session = app.state::<Arc<MonitoringSession>>();
    if let Some(gid) = request.group_id {
        get_group(session.database().pool(), gid)
            .await
            .map_err(|err| format!("load group: {err}"))?
            .ok_or_else(|| format!("group {gid} not found"))?;
    }
    set_network_group(
        session.database().pool(),
        request.network_id,
        request.group_id,
    )
    .await
    .map_err(|err| format!("set network group: {err}"))
}
