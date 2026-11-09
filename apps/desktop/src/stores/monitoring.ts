import { defineStore } from "pinia";
import { ref } from "vue";
import {
  clearHistory,
  fetchDatabaseInfo,
  fetchMonitoringStatus,
  fetchSettings,
  pauseMonitoring,
  resumeMonitoring,
  setAsnInfo,
  setAutostart,
  setBackgroundMode,
  setCacheTtlSeconds,
  setCountryInfo,
  setDateTimeFormat,
  setDnsResolution,
  setHistoryRetention,
  setIpEnrichment,
  setLanguage,
  setReverseDns,
  setStartMonitoringAutomatically,
  setTheme,
  startMonitoring,
  stopMonitoring,
} from "../services/monitoring";
import type {
  AppSettings,
  DatabaseInfo,
  DateTimeFormatPreference,
  MonitoringStatus,
  ThemePreference,
} from "../types/monitoring";
import { applyTheme } from "../utils/format";

export const useMonitoringStore = defineStore("monitoring", () => {
  const status = ref<MonitoringStatus | null>(null);
  const settings = ref<AppSettings | null>(null);
  const databaseInfo = ref<DatabaseInfo | null>(null);
  const error = ref<string | null>(null);
  const busy = ref(false);

  async function refreshStatus() {
    try {
      status.value = await fetchMonitoringStatus();
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    }
  }

  async function refreshSettings() {
    try {
      settings.value = await fetchSettings();
      if (settings.value) {
        applyTheme(settings.value.theme);
      }
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    }
  }

  async function refreshDatabaseInfo() {
    try {
      databaseInfo.value = await fetchDatabaseInfo();
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    }
  }

  async function withBusy(action: () => Promise<void>) {
    busy.value = true;
    error.value = null;
    try {
      await action();
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      busy.value = false;
    }
  }

  async function start(useMock = false) {
    await withBusy(async () => {
      status.value = await startMonitoring(useMock);
    });
  }

  async function stop() {
    await withBusy(async () => {
      status.value = await stopMonitoring();
    });
  }

  async function pause() {
    await withBusy(async () => {
      status.value = await pauseMonitoring();
    });
  }

  async function resume() {
    await withBusy(async () => {
      status.value = await resumeMonitoring();
    });
  }

  async function updateAutostart(enabled: boolean) {
    await withBusy(async () => {
      settings.value = await setAutostart(enabled);
    });
  }

  async function updateStartAutomatically(enabled: boolean) {
    await withBusy(async () => {
      settings.value = await setStartMonitoringAutomatically(enabled);
    });
  }

  async function updateBackgroundMode(enabled: boolean) {
    await withBusy(async () => {
      settings.value = await setBackgroundMode(enabled);
    });
  }

  async function updateDnsResolution(enabled: boolean) {
    await withBusy(async () => {
      settings.value = await setDnsResolution(enabled);
    });
  }

  async function updateReverseDns(enabled: boolean) {
    await withBusy(async () => {
      settings.value = await setReverseDns(enabled);
    });
  }

  async function updateIpEnrichment(enabled: boolean) {
    await withBusy(async () => {
      settings.value = await setIpEnrichment(enabled);
    });
  }

  async function updateAsnInfo(enabled: boolean) {
    await withBusy(async () => {
      settings.value = await setAsnInfo(enabled);
    });
  }

  async function updateCountryInfo(enabled: boolean) {
    await withBusy(async () => {
      settings.value = await setCountryInfo(enabled);
    });
  }

  async function updateCacheTtlSeconds(seconds: number) {
    await withBusy(async () => {
      settings.value = await setCacheTtlSeconds(seconds);
    });
  }

  async function updateHistoryRetention(enabled: boolean, days: number) {
    await withBusy(async () => {
      settings.value = await setHistoryRetention(enabled, days);
      databaseInfo.value = await fetchDatabaseInfo();
    });
  }

  async function updateLanguage(language: string) {
    await withBusy(async () => {
      settings.value = await setLanguage(language);
    });
  }

  async function updateTheme(theme: ThemePreference) {
    await withBusy(async () => {
      settings.value = await setTheme(theme);
      applyTheme(theme);
    });
  }

  async function updateDateTimeFormat(format: DateTimeFormatPreference) {
    await withBusy(async () => {
      settings.value = await setDateTimeFormat(format);
    });
  }

  async function clearConnectionHistory() {
    await withBusy(async () => {
      databaseInfo.value = await clearHistory();
    });
  }

  return {
    status,
    settings,
    databaseInfo,
    error,
    busy,
    refreshStatus,
    refreshSettings,
    refreshDatabaseInfo,
    start,
    stop,
    pause,
    resume,
    updateAutostart,
    updateStartAutomatically,
    updateBackgroundMode,
    updateDnsResolution,
    updateReverseDns,
    updateIpEnrichment,
    updateAsnInfo,
    updateCountryInfo,
    updateCacheTtlSeconds,
    updateHistoryRetention,
    updateLanguage,
    updateTheme,
    updateDateTimeFormat,
    clearConnectionHistory,
  };
});
