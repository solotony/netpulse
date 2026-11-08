//! Persisted app settings exposed to the UI.

use database::{
    get_bool_setting, set_bool_setting, Database, SETTING_AUTOSTART, SETTING_BACKGROUND_MODE,
    SETTING_START_MONITORING_AUTOMATICALLY,
};
use serde::Serialize;

/// Settings snapshot for the Settings page.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub autostart: bool,
    pub start_monitoring_automatically: bool,
    pub background_mode: bool,
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
