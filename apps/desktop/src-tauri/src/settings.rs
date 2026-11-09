//! Persisted app settings exposed to the UI.

use database::{
    get_bool_setting, get_i64_setting, get_setting, set_bool_setting, set_i64_setting, set_setting,
    Database, SETTING_ASN_INFO, SETTING_AUTOSTART, SETTING_BACKGROUND_MODE,
    SETTING_CACHE_TTL_SECONDS, SETTING_COUNTRY_INFO, SETTING_DATE_TIME_FORMAT,
    SETTING_DNS_RESOLUTION, SETTING_HISTORY_RETENTION_DAYS, SETTING_HISTORY_RETENTION_ENABLED,
    SETTING_IP_ENRICHMENT, SETTING_LANGUAGE, SETTING_REVERSE_DNS,
    SETTING_START_MONITORING_AUTOMATICALLY, SETTING_THEME,
};
use serde::Serialize;

const ALLOWED_THEMES: &[&str] = &["system", "light", "dark"];
const ALLOWED_DATE_TIME_FORMATS: &[&str] = &["iso", "locale", "locale_seconds"];
const ALLOWED_LANGUAGES: &[&str] = &["en"];

/// Settings snapshot for the Settings page.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub autostart: bool,
    pub start_monitoring_automatically: bool,
    pub background_mode: bool,
    pub dns_resolution: bool,
    pub reverse_dns: bool,
    pub ip_enrichment: bool,
    pub asn_info: bool,
    pub country_info: bool,
    pub cache_ttl_seconds: i64,
    pub history_retention_enabled: bool,
    pub history_retention_days: i64,
    pub language: String,
    pub theme: String,
    pub date_time_format: String,
}

/// On-disk database metadata for Settings → Data.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfoDto {
    pub path: Option<String>,
    pub size_bytes: Option<u64>,
    pub first_recorded_at: Option<String>,
    pub last_recorded_at: Option<String>,
    pub connection_count: i64,
}

impl AppSettings {
    pub async fn load(db: &Database) -> Result<Self, String> {
        Ok(Self {
            autostart: get_bool_setting(db.pool(), SETTING_AUTOSTART, false)
                .await
                .map_err(|err| err.to_string())?,
            start_monitoring_automatically: get_bool_setting(
                db.pool(),
                SETTING_START_MONITORING_AUTOMATICALLY,
                false,
            )
            .await
            .map_err(|err| err.to_string())?,
            background_mode: get_bool_setting(db.pool(), SETTING_BACKGROUND_MODE, true)
                .await
                .map_err(|err| err.to_string())?,
            dns_resolution: get_bool_setting(db.pool(), SETTING_DNS_RESOLUTION, true)
                .await
                .map_err(|err| err.to_string())?,
            reverse_dns: get_bool_setting(db.pool(), SETTING_REVERSE_DNS, true)
                .await
                .map_err(|err| err.to_string())?,
            ip_enrichment: get_bool_setting(db.pool(), SETTING_IP_ENRICHMENT, true)
                .await
                .map_err(|err| err.to_string())?,
            asn_info: get_bool_setting(db.pool(), SETTING_ASN_INFO, true)
                .await
                .map_err(|err| err.to_string())?,
            country_info: get_bool_setting(db.pool(), SETTING_COUNTRY_INFO, true)
                .await
                .map_err(|err| err.to_string())?,
            cache_ttl_seconds: get_i64_setting(db.pool(), SETTING_CACHE_TTL_SECONDS, 86400)
                .await
                .map_err(|err| err.to_string())?,
            history_retention_enabled: get_bool_setting(
                db.pool(),
                SETTING_HISTORY_RETENTION_ENABLED,
                false,
            )
            .await
            .map_err(|err| err.to_string())?,
            history_retention_days: get_i64_setting(db.pool(), SETTING_HISTORY_RETENTION_DAYS, 30)
                .await
                .map_err(|err| err.to_string())?,
            language: get_setting(db.pool(), SETTING_LANGUAGE)
                .await
                .map_err(|err| err.to_string())?
                .unwrap_or_else(|| "en".into()),
            theme: get_setting(db.pool(), SETTING_THEME)
                .await
                .map_err(|err| err.to_string())?
                .unwrap_or_else(|| "system".into()),
            date_time_format: get_setting(db.pool(), SETTING_DATE_TIME_FORMAT)
                .await
                .map_err(|err| err.to_string())?
                .unwrap_or_else(|| "iso".into()),
        })
    }
}

impl DatabaseInfoDto {
    pub async fn load(db: &Database) -> Result<Self, String> {
        let info = db.database_info().await.map_err(|err| err.to_string())?;
        Ok(Self {
            path: info.path,
            size_bytes: info.size_bytes,
            first_recorded_at: info.first_recorded_at.map(|t| t.to_rfc3339()),
            last_recorded_at: info.last_recorded_at.map(|t| t.to_rfc3339()),
            connection_count: info.connection_count,
        })
    }
}

pub async fn set_autostart_setting(db: &Database, enabled: bool) -> Result<AppSettings, String> {
    set_bool_setting(db.pool(), SETTING_AUTOSTART, enabled)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_start_monitoring_automatically(
    db: &Database,
    enabled: bool,
) -> Result<AppSettings, String> {
    set_bool_setting(db.pool(), SETTING_START_MONITORING_AUTOMATICALLY, enabled)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_background_mode(db: &Database, enabled: bool) -> Result<AppSettings, String> {
    set_bool_setting(db.pool(), SETTING_BACKGROUND_MODE, enabled)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_dns_resolution(db: &Database, enabled: bool) -> Result<AppSettings, String> {
    set_bool_setting(db.pool(), SETTING_DNS_RESOLUTION, enabled)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_reverse_dns(db: &Database, enabled: bool) -> Result<AppSettings, String> {
    set_bool_setting(db.pool(), SETTING_REVERSE_DNS, enabled)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_ip_enrichment(db: &Database, enabled: bool) -> Result<AppSettings, String> {
    set_bool_setting(db.pool(), SETTING_IP_ENRICHMENT, enabled)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_asn_info(db: &Database, enabled: bool) -> Result<AppSettings, String> {
    set_bool_setting(db.pool(), SETTING_ASN_INFO, enabled)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_country_info(db: &Database, enabled: bool) -> Result<AppSettings, String> {
    set_bool_setting(db.pool(), SETTING_COUNTRY_INFO, enabled)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_cache_ttl_seconds(db: &Database, seconds: i64) -> Result<AppSettings, String> {
    if seconds < 0 {
        return Err("cache TTL must be >= 0".into());
    }
    set_i64_setting(db.pool(), SETTING_CACHE_TTL_SECONDS, seconds)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_history_retention(
    db: &Database,
    enabled: bool,
    days: i64,
) -> Result<AppSettings, String> {
    if days < 1 {
        return Err("retention days must be >= 1".into());
    }
    set_bool_setting(db.pool(), SETTING_HISTORY_RETENTION_ENABLED, enabled)
        .await
        .map_err(|err| err.to_string())?;
    set_i64_setting(db.pool(), SETTING_HISTORY_RETENTION_DAYS, days)
        .await
        .map_err(|err| err.to_string())?;
    if enabled {
        let _ = db
            .apply_history_retention()
            .await
            .map_err(|err| err.to_string())?;
    }
    AppSettings::load(db).await
}

pub async fn set_language(db: &Database, language: String) -> Result<AppSettings, String> {
    if !ALLOWED_LANGUAGES.contains(&language.as_str()) {
        return Err(format!("unsupported language: {language}"));
    }
    set_setting(db.pool(), SETTING_LANGUAGE, &language)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_theme(db: &Database, theme: String) -> Result<AppSettings, String> {
    if !ALLOWED_THEMES.contains(&theme.as_str()) {
        return Err(format!("unsupported theme: {theme}"));
    }
    set_setting(db.pool(), SETTING_THEME, &theme)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn set_date_time_format(db: &Database, format: String) -> Result<AppSettings, String> {
    if !ALLOWED_DATE_TIME_FORMATS.contains(&format.as_str()) {
        return Err(format!("unsupported date/time format: {format}"));
    }
    set_setting(db.pool(), SETTING_DATE_TIME_FORMAT, &format)
        .await
        .map_err(|err| err.to_string())?;
    AppSettings::load(db).await
}

pub async fn clear_history(db: &Database) -> Result<DatabaseInfoDto, String> {
    db.clear_connection_history()
        .await
        .map_err(|err| err.to_string())?;
    DatabaseInfoDto::load(db).await
}
