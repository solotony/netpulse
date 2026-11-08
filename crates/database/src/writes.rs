//! Transactional insert / update helpers for processes and connections.

use sqlx::{SqliteExecutor, SqlitePool};

use chrono::{DateTime, Utc};

use crate::error::DatabaseError;
use crate::models::{
    ConnectionUpdate, DomainRow, IpAddressRow, IpDomainRow, NetworkRow, NewConnection, NewIpDomain,
    NewNetwork, NewProcess, ProcessUpdate,
};

/// Inserts a new process row. Returns the new primary key.
pub async fn insert_process<'e, E>(executor: E, process: &NewProcess) -> Result<i64, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO processes (
            pid, process_name, executable_path, process_start_time,
            first_seen_at, last_seen_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        RETURNING id
        "#,
    )
    .bind(i64::from(process.pid))
    .bind(&process.process_name)
    .bind(&process.executable_path)
    .bind(process.process_start_time)
    .bind(process.first_seen_at)
    .bind(process.last_seen_at)
    .fetch_one(executor)
    .await?;

    Ok(id)
}

/// Inserts a process or updates fields when `(pid, process_start_time)` already exists.
///
/// Stable process-instance identity is `(pid, process_start_time)`, never PID alone.
pub async fn upsert_process<'e, E>(executor: E, process: &NewProcess) -> Result<i64, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO processes (
            pid, process_name, executable_path, process_start_time,
            first_seen_at, last_seen_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT (pid, process_start_time) DO UPDATE SET
            process_name = COALESCE(excluded.process_name, processes.process_name),
            executable_path = COALESCE(excluded.executable_path, processes.executable_path),
            last_seen_at = CASE
                WHEN excluded.last_seen_at > processes.last_seen_at THEN excluded.last_seen_at
                ELSE processes.last_seen_at
            END
        RETURNING id
        "#,
    )
    .bind(i64::from(process.pid))
    .bind(&process.process_name)
    .bind(&process.executable_path)
    .bind(process.process_start_time)
    .bind(process.first_seen_at)
    .bind(process.last_seen_at)
    .fetch_one(executor)
    .await?;

    Ok(id)
}

/// Applies a partial update to an existing process row inside a transaction.
pub async fn update_process(
    pool: &SqlitePool,
    id: i64,
    update: &ProcessUpdate,
) -> Result<(), DatabaseError> {
    let mut tx = pool.begin().await?;

    let set_name = i64::from(update.process_name.is_some());
    let name_value = update.process_name.clone().flatten();
    let set_path = i64::from(update.executable_path.is_some());
    let path_value = update.executable_path.clone().flatten();
    let set_seen = i64::from(update.last_seen_at.is_some());
    let seen_value = update.last_seen_at;

    let result = sqlx::query(
        r#"
        UPDATE processes
        SET
            process_name = CASE WHEN ?2 = 1 THEN ?3 ELSE process_name END,
            executable_path = CASE WHEN ?4 = 1 THEN ?5 ELSE executable_path END,
            last_seen_at = CASE WHEN ?6 = 1 THEN ?7 ELSE last_seen_at END
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(set_name)
    .bind(name_value)
    .bind(set_path)
    .bind(path_value)
    .bind(set_seen)
    .bind(seen_value)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DatabaseError::Sqlx(sqlx::Error::RowNotFound));
    }

    tx.commit().await?;
    Ok(())
}

/// Inserts a new connection session. Returns the new primary key.
pub async fn insert_connection<'e, E>(
    executor: E,
    connection: &NewConnection,
) -> Result<i64, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO connections (
            process_id, protocol, direction, local_ip, local_port,
            remote_ip, remote_port, started_at, ended_at,
            bytes_sent, bytes_received, collector_id
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
        RETURNING id
        "#,
    )
    .bind(connection.process_id)
    .bind(&connection.protocol)
    .bind(&connection.direction)
    .bind(&connection.local_ip)
    .bind(i64::from(connection.local_port))
    .bind(&connection.remote_ip)
    .bind(i64::from(connection.remote_port))
    .bind(connection.started_at)
    .bind(connection.ended_at)
    .bind(connection.bytes_sent)
    .bind(connection.bytes_received)
    .bind(&connection.collector_id)
    .fetch_one(executor)
    .await?;

    Ok(id)
}

/// Applies a partial update to an existing connection row using `executor`.
pub async fn update_connection_exec<'e, E>(
    executor: E,
    id: i64,
    update: &ConnectionUpdate,
) -> Result<(), DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let set_ended = i64::from(update.ended_at.is_some());
    let ended_value = update.ended_at.flatten();
    let set_sent = i64::from(update.bytes_sent.is_some());
    let sent_value = update.bytes_sent.flatten();
    let set_recv = i64::from(update.bytes_received.is_some());
    let recv_value = update.bytes_received.flatten();
    let set_collector = i64::from(update.collector_id.is_some());
    let collector_value = update.collector_id.clone().flatten();

    let result = sqlx::query(
        r#"
        UPDATE connections
        SET
            ended_at = CASE WHEN ?2 = 1 THEN ?3 ELSE ended_at END,
            bytes_sent = CASE WHEN ?4 = 1 THEN ?5 ELSE bytes_sent END,
            bytes_received = CASE WHEN ?6 = 1 THEN ?7 ELSE bytes_received END,
            collector_id = CASE WHEN ?8 = 1 THEN ?9 ELSE collector_id END
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .bind(set_ended)
    .bind(ended_value)
    .bind(set_sent)
    .bind(sent_value)
    .bind(set_recv)
    .bind(recv_value)
    .bind(set_collector)
    .bind(collector_value)
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DatabaseError::Sqlx(sqlx::Error::RowNotFound));
    }

    Ok(())
}

/// Applies a partial update to an existing connection row inside a transaction.
pub async fn update_connection(
    pool: &SqlitePool,
    id: i64,
    update: &ConnectionUpdate,
) -> Result<(), DatabaseError> {
    let mut tx = pool.begin().await?;
    update_connection_exec(&mut *tx, id, update).await?;
    tx.commit().await?;
    Ok(())
}

/// Finds the open connection matching the connection identity (ended_at IS NULL).
///
/// Identity is protocol + local/remote endpoints + process instance. The open
/// interval is represented by `ended_at IS NULL`.
pub async fn find_open_connection<'e, E>(
    executor: E,
    process_id: i64,
    protocol: &str,
    local_ip: &str,
    local_port: u16,
    remote_ip: &str,
    remote_port: u16,
) -> Result<Option<crate::models::ConnectionRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, crate::models::ConnectionRow>(
        r#"
        SELECT id, process_id, protocol, direction, local_ip, local_port,
               remote_ip, remote_port, started_at, ended_at,
               bytes_sent, bytes_received, collector_id
        FROM connections
        WHERE process_id = ?1
          AND protocol = ?2
          AND local_ip = ?3
          AND local_port = ?4
          AND remote_ip = ?5
          AND remote_port = ?6
          AND ended_at IS NULL
        ORDER BY started_at DESC
        LIMIT 1
        "#,
    )
    .bind(process_id)
    .bind(protocol)
    .bind(local_ip)
    .bind(i64::from(local_port))
    .bind(remote_ip)
    .bind(i64::from(remote_port))
    .fetch_optional(executor)
    .await?;

    Ok(row)
}

/// Inserts a row into `connection_events`. Returns the new primary key.
pub async fn insert_connection_event<'e, E>(
    executor: E,
    connection_id: i64,
    event_type: &str,
    timestamp: chrono::DateTime<chrono::Utc>,
    event_data: Option<&str>,
) -> Result<i64, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO connection_events (connection_id, event_type, timestamp, event_data)
        VALUES (?1, ?2, ?3, ?4)
        RETURNING id
        "#,
    )
    .bind(connection_id)
    .bind(event_type)
    .bind(timestamp)
    .bind(event_data)
    .fetch_one(executor)
    .await?;

    Ok(id)
}

/// Upserts a process and inserts a connection in one transaction.
pub async fn insert_connection_for_process(
    pool: &SqlitePool,
    process: &NewProcess,
    connection: NewConnection,
) -> Result<(i64, i64), DatabaseError> {
    let mut tx = pool.begin().await?;
    let process_id = upsert_process(&mut *tx, process).await?;
    let mut connection = connection;
    connection.process_id = process_id;
    let connection_id = insert_connection(&mut *tx, &connection).await?;
    tx.commit().await?;
    Ok((process_id, connection_id))
}

/// Upserts an IP address by exact `ip` string. Extends `last_seen_at` when newer.
pub async fn upsert_ip_address<'e, E>(
    executor: E,
    ip: &str,
    seen_at: DateTime<Utc>,
) -> Result<i64, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO ip_addresses (ip, first_seen_at, last_seen_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT (ip) DO UPDATE SET
            last_seen_at = CASE
                WHEN excluded.last_seen_at > ip_addresses.last_seen_at THEN excluded.last_seen_at
                ELSE ip_addresses.last_seen_at
            END
        RETURNING id
        "#,
    )
    .bind(ip)
    .bind(seen_at)
    .bind(seen_at)
    .fetch_one(executor)
    .await?;

    Ok(id)
}

/// Inserts or updates a network row keyed by unique `cidr`, refreshing `updated_at`.
pub async fn upsert_network<'e, E>(
    executor: E,
    network: &NewNetwork,
) -> Result<i64, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO networks (
            cidr, net_range, net_name, organization, asn, country, source, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT (cidr) DO UPDATE SET
            net_range = COALESCE(excluded.net_range, networks.net_range),
            net_name = COALESCE(excluded.net_name, networks.net_name),
            organization = COALESCE(excluded.organization, networks.organization),
            asn = COALESCE(excluded.asn, networks.asn),
            country = COALESCE(excluded.country, networks.country),
            source = COALESCE(excluded.source, networks.source),
            updated_at = excluded.updated_at
        RETURNING id
        "#,
    )
    .bind(&network.cidr)
    .bind(&network.net_range)
    .bind(&network.net_name)
    .bind(&network.organization)
    .bind(network.asn)
    .bind(&network.country)
    .bind(&network.source)
    .bind(network.updated_at)
    .fetch_one(executor)
    .await?;

    Ok(id)
}

/// Loads a network by primary key.
pub async fn get_network<'e, E>(
    executor: E,
    id: i64,
) -> Result<Option<NetworkRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, NetworkRow>(
        r#"
        SELECT id, cidr, net_range, net_name, organization, asn, country, source, updated_at
        FROM networks
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

/// Loads all stored networks (for longest-prefix matching in callers).
pub async fn list_networks<'e, E>(executor: E) -> Result<Vec<NetworkRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, NetworkRow>(
        r#"
        SELECT id, cidr, net_range, net_name, organization, asn, country, source, updated_at
        FROM networks
        ORDER BY id
        "#,
    )
    .fetch_all(executor)
    .await?;
    Ok(rows)
}

/// Replaces any existing network associations for `ip_id` with a single winning network.
///
/// One IP maps to the most specific applicable prefix only.
pub async fn set_ip_winning_network(
    pool: &SqlitePool,
    ip_id: i64,
    network_id: i64,
) -> Result<(), DatabaseError> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM ip_networks WHERE ip_id = ?1")
        .bind(ip_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO ip_networks (ip_id, network_id)
        VALUES (?1, ?2)
        "#,
    )
    .bind(ip_id)
    .bind(network_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Returns the single winning network associated with `ip_id`, if any.
pub async fn get_ip_winning_network<'e, E>(
    executor: E,
    ip_id: i64,
) -> Result<Option<NetworkRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, NetworkRow>(
        r#"
        SELECT n.id, n.cidr, n.net_range, n.net_name, n.organization, n.asn,
               n.country, n.source, n.updated_at
        FROM ip_networks AS link
        JOIN networks AS n ON n.id = link.network_id
        WHERE link.ip_id = ?1
        LIMIT 1
        "#,
    )
    .bind(ip_id)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

/// Upserts network + IP rows and stores the winning association in one transaction.
pub async fn store_winning_ip_network(
    pool: &SqlitePool,
    ip: &str,
    network: &NewNetwork,
    seen_at: DateTime<Utc>,
) -> Result<(i64, i64), DatabaseError> {
    let mut tx = pool.begin().await?;
    let network_id = upsert_network(&mut *tx, network).await?;
    let ip_id = upsert_ip_address(&mut *tx, ip, seen_at).await?;

    sqlx::query("DELETE FROM ip_networks WHERE ip_id = ?1")
        .bind(ip_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO ip_networks (ip_id, network_id)
        VALUES (?1, ?2)
        "#,
    )
    .bind(ip_id)
    .bind(network_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((ip_id, network_id))
}

/// Loads an IP address row by primary key.
pub async fn get_ip_address<'e, E>(
    executor: E,
    id: i64,
) -> Result<Option<IpAddressRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, IpAddressRow>(
        r#"
        SELECT id, ip, first_seen_at, last_seen_at
        FROM ip_addresses
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

/// Upserts a domain name by exact `domain` string. Extends `last_seen_at` when newer.
pub async fn upsert_domain<'e, E>(
    executor: E,
    domain: &str,
    seen_at: DateTime<Utc>,
) -> Result<i64, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO domains (domain, first_seen_at, last_seen_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT (domain) DO UPDATE SET
            last_seen_at = CASE
                WHEN excluded.last_seen_at > domains.last_seen_at THEN excluded.last_seen_at
                ELSE domains.last_seen_at
            END
        RETURNING id
        "#,
    )
    .bind(domain)
    .bind(seen_at)
    .bind(seen_at)
    .fetch_one(executor)
    .await?;

    Ok(id)
}

/// Loads a domain row by primary key.
pub async fn get_domain<'e, E>(
    executor: E,
    id: i64,
) -> Result<Option<DomainRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let row = sqlx::query_as::<_, DomainRow>(
        r#"
        SELECT id, domain, first_seen_at, last_seen_at
        FROM domains
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(executor)
    .await?;
    Ok(row)
}

/// Upserts IP + domain rows and links them under `source` (multiple domains per IP allowed).
pub async fn upsert_ip_domain(
    pool: &SqlitePool,
    link: &NewIpDomain,
) -> Result<(i64, i64), DatabaseError> {
    let mut tx = pool.begin().await?;

    let ip_id = upsert_ip_address(&mut *tx, &link.ip, link.seen_at).await?;
    let domain_id = upsert_domain(&mut *tx, &link.domain, link.seen_at).await?;

    sqlx::query(
        r#"
        INSERT INTO ip_domains (
            ip_id, domain_id, source, first_seen_at, last_seen_at, confidence
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT (ip_id, domain_id, source) DO UPDATE SET
            last_seen_at = CASE
                WHEN excluded.last_seen_at > ip_domains.last_seen_at THEN excluded.last_seen_at
                ELSE ip_domains.last_seen_at
            END,
            confidence = COALESCE(excluded.confidence, ip_domains.confidence)
        "#,
    )
    .bind(ip_id)
    .bind(domain_id)
    .bind(&link.source)
    .bind(link.seen_at)
    .bind(link.seen_at)
    .bind(link.confidence)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((ip_id, domain_id))
}

/// Lists domain associations for an IP (all sources).
pub async fn list_ip_domains<'e, E>(
    executor: E,
    ip_id: i64,
) -> Result<Vec<IpDomainRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, IpDomainRow>(
        r#"
        SELECT ip_id, domain_id, source, first_seen_at, last_seen_at, confidence
        FROM ip_domains
        WHERE ip_id = ?1
        ORDER BY domain_id, source
        "#,
    )
    .bind(ip_id)
    .fetch_all(executor)
    .await?;
    Ok(rows)
}

/// Lists domain names linked to `ip_id`.
pub async fn list_domains_for_ip<'e, E>(
    executor: E,
    ip_id: i64,
) -> Result<Vec<DomainRow>, DatabaseError>
where
    E: SqliteExecutor<'e>,
{
    let rows = sqlx::query_as::<_, DomainRow>(
        r#"
        SELECT d.id, d.domain, d.first_seen_at, d.last_seen_at
        FROM ip_domains AS link
        JOIN domains AS d ON d.id = link.domain_id
        WHERE link.ip_id = ?1
        ORDER BY d.domain
        "#,
    )
    .bind(ip_id)
    .fetch_all(executor)
    .await?;
    Ok(rows)
}
