import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  DatabaseInfo,
  DateTimeFormatPreference,
  MonitoringStatus,
  ThemePreference,
} from "../types/monitoring";

export async function fetchMonitoringStatus(): Promise<MonitoringStatus> {
  return invoke<MonitoringStatus>("monitoring_status");
}

export async function startMonitoring(useMock = false): Promise<MonitoringStatus> {
  return invoke<MonitoringStatus>("start_monitoring", { useMock });
}

export async function stopMonitoring(): Promise<MonitoringStatus> {
  return invoke<MonitoringStatus>("stop_monitoring");
}

export async function pauseMonitoring(): Promise<MonitoringStatus> {
  return invoke<MonitoringStatus>("pause_monitoring");
}

export async function resumeMonitoring(): Promise<MonitoringStatus> {
  return invoke<MonitoringStatus>("resume_monitoring");
}

export async function fetchSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function setAutostart(enabled: boolean): Promise<AppSettings> {
  return invoke<AppSettings>("set_autostart", { enabled });
}

export async function setStartMonitoringAutomatically(
  enabled: boolean,
): Promise<AppSettings> {
  return invoke<AppSettings>("set_start_monitoring_automatically", { enabled });
}

export async function setBackgroundMode(enabled: boolean): Promise<AppSettings> {
  return invoke<AppSettings>("set_background_mode", { enabled });
}

export async function setDnsResolution(enabled: boolean): Promise<AppSettings> {
  return invoke<AppSettings>("set_dns_resolution", { enabled });
}

export async function setReverseDns(enabled: boolean): Promise<AppSettings> {
  return invoke<AppSettings>("set_reverse_dns", { enabled });
}

export async function setIpEnrichment(enabled: boolean): Promise<AppSettings> {
  return invoke<AppSettings>("set_ip_enrichment", { enabled });
}

export async function setAsnInfo(enabled: boolean): Promise<AppSettings> {
  return invoke<AppSettings>("set_asn_info", { enabled });
}

export async function setCountryInfo(enabled: boolean): Promise<AppSettings> {
  return invoke<AppSettings>("set_country_info", { enabled });
}

export async function setCacheTtlSeconds(seconds: number): Promise<AppSettings> {
  return invoke<AppSettings>("set_cache_ttl_seconds", { seconds });
}

export async function setHistoryRetention(
  enabled: boolean,
  days: number,
): Promise<AppSettings> {
  return invoke<AppSettings>("set_history_retention", { enabled, days });
}

export async function setLanguage(language: string): Promise<AppSettings> {
  return invoke<AppSettings>("set_language", { language });
}

export async function setTheme(theme: ThemePreference): Promise<AppSettings> {
  return invoke<AppSettings>("set_theme", { theme });
}

export async function setDateTimeFormat(
  format: DateTimeFormatPreference,
): Promise<AppSettings> {
  return invoke<AppSettings>("set_date_time_format", { format });
}

export async function fetchDatabaseInfo(): Promise<DatabaseInfo> {
  return invoke<DatabaseInfo>("get_database_info");
}

export async function clearHistory(): Promise<DatabaseInfo> {
  return invoke<DatabaseInfo>("clear_history");
}
