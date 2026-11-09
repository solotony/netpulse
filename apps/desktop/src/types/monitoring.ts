/** Backend monitoring lifecycle state. */
export type MonitoringState = "stopped" | "running" | "paused" | "failed";

/** Collector backend used by `start_monitoring`. */
export type CollectorBackend = "native" | "mock";

/** Status returned by monitoring Tauri commands. */
export interface MonitoringStatus {
  state: MonitoringState;
  lastError: string | null;
  backend: CollectorBackend | null;
}

/** UI theme preference. */
export type ThemePreference = "system" | "light" | "dark";

/** Date/time display format preference. */
export type DateTimeFormatPreference = "iso" | "locale" | "locale_seconds";

/** Persisted application settings. */
export interface AppSettings {
  autostart: boolean;
  startMonitoringAutomatically: boolean;
  backgroundMode: boolean;
  dnsResolution: boolean;
  reverseDns: boolean;
  ipEnrichment: boolean;
  asnInfo: boolean;
  countryInfo: boolean;
  cacheTtlSeconds: number;
  historyRetentionEnabled: boolean;
  historyRetentionDays: number;
  language: string;
  theme: ThemePreference;
  dateTimeFormat: DateTimeFormatPreference;
}

/** Database metadata for Settings → Data. */
export interface DatabaseInfo {
  path: string | null;
  sizeBytes: number | null;
  firstRecordedAt: string | null;
  lastRecordedAt: string | null;
  connectionCount: number;
}
