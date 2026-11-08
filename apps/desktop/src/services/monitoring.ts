import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, MonitoringStatus } from "../types/monitoring";

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
