import { defineStore } from "pinia";
import { ref } from "vue";
import {
  fetchMonitoringStatus,
  fetchSettings,
  pauseMonitoring,
  resumeMonitoring,
  setAutostart,
  setBackgroundMode,
  setStartMonitoringAutomatically,
  startMonitoring,
  stopMonitoring,
} from "../services/monitoring";
import type { AppSettings, MonitoringStatus } from "../types/monitoring";

export const useMonitoringStore = defineStore("monitoring", () => {
  const status = ref<MonitoringStatus | null>(null);
  const settings = ref<AppSettings | null>(null);
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

  return {
    status,
    settings,
    error,
    busy,
    refreshStatus,
    refreshSettings,
    start,
    stop,
    pause,
    resume,
    updateAutostart,
    updateStartAutomatically,
    updateBackgroundMode,
  };
});
