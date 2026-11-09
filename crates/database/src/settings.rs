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
/// Setting: allow DNS-related lookups (parent gate for domain resolution).
pub const SETTING_DNS_RESOLUTION: &str = "dns_resolution";
/// Setting: perform asynchronous reverse DNS (PTR) lookups.
pub const SETTING_REVERSE_DNS: &str = "reverse_dns";
/// Setting: store/display ASN fields from enrichment.
pub const SETTING_ASN_INFO: &str = "asn_info";
/// Setting: store/display country fields from enrichment.
pub const SETTING_COUNTRY_INFO: &str = "country_info";
/// Setting: enrichment / DNS cache TTL in seconds (`0` = never expire).
pub const SETTING_CACHE_TTL_SECONDS: &str = "cache_ttl_seconds";
/// Setting: automatically delete old connection history.
pub const SETTING_HISTORY_RETENTION_ENABLED: &str = "history_retention_enabled";
/// Setting: retain connection history for this many days when retention is enabled.
pub const SETTING_HISTORY_RETENTION_DAYS: &str = "history_retention_days";
/// Setting: UI language code (`en` only in MVP).
pub const SETTING_LANGUAGE: &str = "language";
/// Setting: UI theme (`system` | `light` | `dark`).
pub const SETTING_THEME: &str = "theme";
/// Setting: date/time display format (`iso` | `locale` | `locale_seconds`).
pub const SETTING_DATE_TIME_FORMAT: &str = "date_time_format";

const DEFAULT_TRUE: &str = "true";
const DEFAULT_FALSE: &str = "false";
const DEFAULT_CACHE_TTL_SECONDS: &str = "86400";
const DEFAULT_RETENTION_DAYS: &str = "30";
const DEFAULT_LANGUAGE: &str = "en";
const DEFAULT_THEME: &str = "system";
const DEFAULT_DATE_TIME_FORMAT: &str = "iso";

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

/// Reads an integer setting, falling back to `default` when unset or unparsable.
pub async fn get_i64_setting(
    pool: &SqlitePool,
    key: &str,
    default: i64,
) -> Result<i64, DatabaseError> {
    let value = get_setting(pool, key).await?;
    Ok(match value.as_deref() {
        Some(raw) => raw.parse::<i64>().unwrap_or(default),
        None => default,
    })
}

/// Persists an integer setting.
pub async fn set_i64_setting(
    pool: &SqlitePool,
    key: &str,
    value: i64,
) -> Result<(), DatabaseError> {
    set_setting(pool, key, &value.to_string()).await
}

/// Ensures default settings rows exist without overwriting user values.
pub async fn ensure_default_settings(pool: &SqlitePool) -> Result<(), DatabaseError> {
    for (key, value) in [
        (SETTING_AUTOSTART, DEFAULT_FALSE),
        (SETTING_START_MONITORING_AUTOMATICALLY, DEFAULT_FALSE),
        (SETTING_BACKGROUND_MODE, DEFAULT_TRUE),
        (SETTING_IP_ENRICHMENT, DEFAULT_TRUE),
        (SETTING_DNS_RESOLUTION, DEFAULT_TRUE),
        (SETTING_REVERSE_DNS, DEFAULT_TRUE),
        (SETTING_ASN_INFO, DEFAULT_TRUE),
        (SETTING_COUNTRY_INFO, DEFAULT_TRUE),
        (SETTING_CACHE_TTL_SECONDS, DEFAULT_CACHE_TTL_SECONDS),
        (SETTING_HISTORY_RETENTION_ENABLED, DEFAULT_FALSE),
        (SETTING_HISTORY_RETENTION_DAYS, DEFAULT_RETENTION_DAYS),
        (SETTING_LANGUAGE, DEFAULT_LANGUAGE),
        (SETTING_THEME, DEFAULT_THEME),
        (SETTING_DATE_TIME_FORMAT, DEFAULT_DATE_TIME_FORMAT),
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
