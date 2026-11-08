//! Key-value application settings persisted in SQLite.

use sqlx::SqlitePool;

use crate::DatabaseError;

/// Setting: launch the app when the user logs in.
pub const SETTING_AUTOSTART: &str = "autostart";
/// Setting: start the collector automatically when the app launches.
pub const SETTING_START_MONITORING_AUTOMATICALLY: &str = "start_monitoring_automatically";
/// Setting: hide to tray on window close instead of quitting.
pub const SETTING_BACKGROUND_MODE: &str = "background_mode";
/// Setting: perform asynchronous IP enrichment (RDAP) for public remote IPs.
pub const SETTING_IP_ENRICHMENT: &str = "ip_enrichment";

const DEFAULT_TRUE: &str = "true";
const DEFAULT_FALSE: &str = "false";

/// Reads a raw setting value, or `None` when unset.
pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, DatabaseError> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(value)
}

/// Upserts a setting value.
pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<(), DatabaseError> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value) VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Reads a boolean setting, falling back to `default` when unset or unparsable.
pub async fn get_bool_setting(
    pool: &SqlitePool,
    key: &str,
    default: bool,
) -> Result<bool, DatabaseError> {
    let value = get_setting(pool, key).await?;
    Ok(match value.as_deref() {
        Some("true") | Some("1") | Some("yes") => true,
        Some("false") | Some("0") | Some("no") => false,
        Some(_) | None => default,
    })
}

/// Persists a boolean setting as `"true"` / `"false"`.
pub async fn set_bool_setting(
    pool: &SqlitePool,
    key: &str,
    value: bool,
) -> Result<(), DatabaseError> {
    set_setting(
        pool,
        key,
        if value { DEFAULT_TRUE } else { DEFAULT_FALSE },
    )
    .await
}

/// Ensures default settings rows exist without overwriting user values.
pub async fn ensure_default_settings(pool: &SqlitePool) -> Result<(), DatabaseError> {
    for (key, value) in [
        (SETTING_AUTOSTART, DEFAULT_FALSE),
        (SETTING_START_MONITORING_AUTOMATICALLY, DEFAULT_FALSE),
        (SETTING_BACKGROUND_MODE, DEFAULT_TRUE),
        (SETTING_IP_ENRICHMENT, DEFAULT_TRUE),
    ] {
        sqlx::query(
            r#"
            INSERT INTO settings (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO NOTHING
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    }
    Ok(())
}
