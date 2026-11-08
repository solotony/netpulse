//! Transactional insert / update helpers for processes and connections.

use sqlx::{SqliteExecutor, SqlitePool};

use crate::error::DatabaseError;
use crate::models::{ConnectionUpdate, NewConnection, NewProcess, ProcessUpdate};

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
