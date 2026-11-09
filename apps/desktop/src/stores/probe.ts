import { defineStore } from "pinia";
import { computed, ref } from "vue";
import {
  fetchProbeApps,
  fetchProbeDiff,
  type ProbeApp,
  type ProbeDiffRow,
} from "../services/probe";
import { useMonitoringStore } from "./monitoring";

export type ProbePhase = "idle" | "preparing" | "measuring" | "done";

function nowIso(): string {
  return new Date().toISOString();
}

export const useProbeStore = defineStore("probe", () => {
  const apps = ref<ProbeApp[]>([]);
  const selectedKeys = ref<Set<string>>(new Set());
  const phase = ref<ProbePhase>("idle");
  const prepareStart = ref<string | null>(null);
  const prepareEnd = ref<string | null>(null);
  const measureStart = ref<string | null>(null);
  const measureEnd = ref<string | null>(null);
  const prepareIpCount = ref(0);
  const measureIpCount = ref(0);
  const rows = ref<ProbeDiffRow[]>([]);
  const loadingApps = ref(false);
  const refreshing = ref(false);
  const error = ref<string | null>(null);
  const appFilter = ref("");

  const selectedCount = computed(() => selectedKeys.value.size);

  const filteredApps = computed(() => {
    const q = appFilter.value.trim().toLowerCase();
    if (!q) return apps.value;
    return apps.value.filter((app) => {
      const name = (app.displayName ?? "").toLowerCase();
      const key = app.appKey.toLowerCase();
      return name.includes(q) || key.includes(q);
    });
  });

  const canPrepare = computed(() => phase.value === "idle");
  const canMeasure = computed(() => phase.value === "preparing");
  const canStop = computed(() => phase.value === "measuring");
  const canReset = computed(() => phase.value !== "idle");
  const canSelectApps = computed(() => phase.value !== "idle");

  function sessionWindowEnd(): string | null {
    if (!prepareStart.value) return null;
    if (phase.value === "preparing") return nowIso();
    if (phase.value === "measuring") return nowIso();
    return measureEnd.value ?? prepareEnd.value ?? nowIso();
  }

  async function loadApps() {
    if (!prepareStart.value) {
      apps.value = [];
      return;
    }
    const windowEnd = sessionWindowEnd();
    if (!windowEnd) {
      apps.value = [];
      return;
    }
    loadingApps.value = true;
    try {
      const next = await fetchProbeApps(prepareStart.value, windowEnd);
      apps.value = next;
      // Drop selections that disappeared from the active set.
      const valid = new Set(next.map((app) => app.appKey));
      const kept = [...selectedKeys.value].filter((key) => valid.has(key));
      if (kept.length !== selectedKeys.value.size) {
        selectedKeys.value = new Set(kept);
      }
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      loadingApps.value = false;
    }
  }

  function toggleApp(appKey: string) {
    if (!canSelectApps.value) return;
    const next = new Set(selectedKeys.value);
    if (next.has(appKey)) {
      next.delete(appKey);
    } else {
      next.add(appKey);
    }
    selectedKeys.value = next;
    void refreshDiff();
  }

  function clearSelection() {
    if (!canSelectApps.value) return;
    selectedKeys.value = new Set();
    void refreshDiff();
  }

  async function ensureMonitoringRunning() {
    const monitoring = useMonitoringStore();
    await monitoring.refreshStatus();
    const state = monitoring.status?.state;
    if (state === "running") return;
    if (state === "paused") {
      await monitoring.resume();
      return;
    }
    await monitoring.start(false);
  }

  async function refreshDiff() {
    if (!prepareStart.value) return;
    refreshing.value = true;
    error.value = null;
    try {
      const endNow = nowIso();
      const prepEnd =
        phase.value === "preparing" ? endNow : (prepareEnd.value ?? endNow);
      const measEnd =
        phase.value === "measuring" ? endNow : measureEnd.value;

      const [result] = await Promise.all([
        fetchProbeDiff({
          appKeys: [...selectedKeys.value],
          prepareStart: prepareStart.value,
          prepareEnd: prepEnd,
          measureStart: measureStart.value,
          measureEnd: measEnd,
        }),
        loadApps(),
      ]);
      prepareIpCount.value = result.prepareIpCount;
      measureIpCount.value = result.measureIpCount;
      rows.value = result.rows;
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
    } finally {
      refreshing.value = false;
    }
  }

  async function startPrepare() {
    if (!canPrepare.value) return;
    error.value = null;
    try {
      await ensureMonitoringRunning();
      prepareStart.value = nowIso();
      prepareEnd.value = null;
      measureStart.value = null;
      measureEnd.value = null;
      prepareIpCount.value = 0;
      measureIpCount.value = 0;
      rows.value = [];
      apps.value = [];
      selectedKeys.value = new Set();
      phase.value = "preparing";
      await refreshDiff();
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
      phase.value = "idle";
    }
  }

  async function startMeasure() {
    if (!canMeasure.value) return;
    prepareEnd.value = nowIso();
    measureStart.value = prepareEnd.value;
    measureEnd.value = null;
    phase.value = "measuring";
    await refreshDiff();
  }

  async function stopMeasure() {
    if (!canStop.value) return;
    measureEnd.value = nowIso();
    phase.value = "done";
    await refreshDiff();
  }

  function reset() {
    phase.value = "idle";
    prepareStart.value = null;
    prepareEnd.value = null;
    measureStart.value = null;
    measureEnd.value = null;
    prepareIpCount.value = 0;
    measureIpCount.value = 0;
    rows.value = [];
    apps.value = [];
    selectedKeys.value = new Set();
    error.value = null;
  }

  return {
    apps,
    selectedKeys,
    phase,
    prepareStart,
    prepareEnd,
    measureStart,
    measureEnd,
    prepareIpCount,
    measureIpCount,
    rows,
    loadingApps,
    refreshing,
    error,
    appFilter,
    selectedCount,
    filteredApps,
    canPrepare,
    canMeasure,
    canStop,
    canReset,
    canSelectApps,
    loadApps,
    toggleApp,
    clearSelection,
    startPrepare,
    startMeasure,
    stopMeasure,
    reset,
    refreshDiff,
  };
});
