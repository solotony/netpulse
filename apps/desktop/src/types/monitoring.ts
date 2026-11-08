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

/** Persisted application settings. */
export interface AppSettings {
  autostart: boolean;
  startMonitoringAutomatically: boolean;
  backgroundMode: boolean;
}
