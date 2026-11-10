<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import AppShell from "../layouts/AppShell.vue";
import { useProbeStore } from "../stores/probe";
import { copyText } from "../utils/clipboard";

const store = useProbeStore();
const copyNotice = ref<string | null>(null);
let copyTimer: ReturnType<typeof setTimeout> | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;

function syncPolling() {
  const live = store.phase === "preparing" || store.phase === "measuring";
  if (live && !pollTimer) {
    pollTimer = setInterval(() => {
      void store.refreshDiff();
    }, 1000);
  } else if (!live && pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

onUnmounted(() => {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  if (copyTimer) {
    clearTimeout(copyTimer);
  }
});

watch(
  () => store.phase,
  () => syncPolling(),
);

onMounted(() => {
  syncPolling();
});

function phaseLabel(phase: string): string {
  switch (phase) {
    case "preparing":
      return "Preparing…";
    case "measuring":
      return "Measuring…";
    case "done":
      return "Done";
    default:
      return "Idle";
  }
}

function appLabel(app: { displayName: string | null; appKey: string }): string {
  const name = app.displayName?.trim();
  if (name) return name;
  const key = app.appKey;
  const slash = key.lastIndexOf("/");
  return slash >= 0 ? key.slice(slash + 1) : key;
}

async function onCopy(value: string) {
  const ok = await copyText(value);
  copyNotice.value = ok ? "Copied" : "Copy failed";
  if (copyTimer) clearTimeout(copyTimer);
  copyTimer = setTimeout(() => {
    copyNotice.value = null;
  }, 1200);
}
</script>

<template>
  <AppShell>
    <div class="layout">
      <section class="picker">
        <div class="toolbar">
          <input
            v-model="store.appFilter"
            type="search"
            placeholder="Search applications…"
            :disabled="store.phase === 'idle'"
          />
          <button
            type="button"
            :disabled="!store.canSelectApps || store.loadingApps"
            @click="store.loadApps()"
          >
            {{ store.loadingApps ? "Loading…" : "Refresh" }}
          </button>
          <button
            type="button"
            :disabled="!store.canSelectApps || store.selectedCount === 0"
            @click="store.clearSelection()"
          >
            Clear
          </button>
        </div>

        <p class="meta">
          {{
            store.selectedCount === 0
              ? `All apps · ${store.filteredApps.length} active`
              : `${store.selectedCount} selected · ${store.filteredApps.length} active`
          }}
        </p>

        <div class="app-list">
          <label
            v-for="app in store.filteredApps"
            :key="app.appKey"
            class="app-row"
            :class="{ selected: store.selectedKeys.has(app.appKey) }"
          >
            <input
              type="checkbox"
              :checked="store.selectedKeys.has(app.appKey)"
              :disabled="!store.canSelectApps"
              @change="store.toggleApp(app.appKey)"
            />
            <span class="app-name">{{ appLabel(app) }}</span>
            <span class="app-key mono">{{ app.appKey }}</span>
            <span class="app-count">{{ app.processCount }}</span>
          </label>
          <p v-if="store.phase === 'idle'" class="empty">
            Press Prepare to start. Applications with network activity will appear here.
          </p>
          <p v-else-if="!store.loadingApps && store.filteredApps.length === 0" class="empty">
            Waiting for network activity…
          </p>
        </div>
      </section>

      <section class="results">
        <div class="controls">
          <button
            type="button"
            class="primary"
            :disabled="!store.canPrepare"
            @click="store.startPrepare()"
          >
            Prepare
          </button>
          <button
            type="button"
            class="primary"
            :disabled="!store.canMeasure"
            @click="store.startMeasure()"
          >
            Measure
          </button>
          <button type="button" :disabled="!store.canStop" @click="store.stopMeasure()">
            Stop
          </button>
          <button type="button" :disabled="!store.canReset" @click="store.reset()">
            Reset
          </button>
          <span class="phase" :data-phase="store.phase">{{ phaseLabel(store.phase) }}</span>
          <span v-if="copyNotice" class="notice">{{ copyNotice }}</span>
        </div>

        <div class="stats">
          <div class="stat">
            <div class="stat-label">Baseline IPs</div>
            <div class="stat-value">{{ store.prepareIpCount }}</div>
          </div>
          <div class="stat">
            <div class="stat-label">Measure IPs</div>
            <div class="stat-value">{{ store.measureIpCount }}</div>
          </div>
          <div class="stat">
            <div class="stat-label">New IPs</div>
            <div class="stat-value accent">{{ store.newIpCount }}</div>
          </div>
        </div>

        <p v-if="store.error" class="error">{{ store.error }}</p>
        <p v-else-if="store.phase === 'idle'" class="hint">
          Press <strong>Prepare</strong> for a baseline, then <strong>Measure</strong> during the
          action you care about. Optionally select applications from the left list to narrow the
          result; with nothing selected, every app is included.
        </p>
        <p v-else-if="store.phase === 'measuring' || store.phase === 'done'" class="hint">
          Gray addresses were collected during prepare. Bold addresses appeared during measure.
        </p>

        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Remote IP</th>
                <th>Domains</th>
                <th>Ports</th>
                <th>Processes</th>
                <th>First seen</th>
              </tr>
            </thead>
            <tbody>
              <tr v-if="store.phase !== 'idle' && store.rows.length === 0">
                <td colspan="5" class="empty">
                  {{
                    store.phase === "preparing"
                      ? "Collecting baseline…"
                      : store.phase === "measuring"
                        ? "No new addresses yet…"
                        : "No new addresses in this measure."
                  }}
                </td>
              </tr>
              <tr
                v-for="row in store.rows"
                :key="row.remoteIp"
                :class="{
                  'is-baseline': store.phase !== 'preparing' && row.origin === 'baseline',
                  'is-new': row.origin === 'new',
                }"
              >
                <td class="mono clickable" @click="onCopy(row.remoteIp)">{{ row.remoteIp }}</td>
                <td
                  class="clickable"
                  @click="row.domains && onCopy(row.domains)"
                >
                  {{ row.domains || "—" }}
                </td>
                <td class="mono">{{ row.ports || "—" }}</td>
                <td>{{ row.processNames || "—" }}</td>
                <td class="mono">{{ row.firstSeen }}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>
    </div>
  </AppShell>
</template>

<style scoped>
.layout {
  display: grid;
  grid-template-columns: minmax(260px, 0.9fr) minmax(0, 1.4fr);
  gap: 12px;
  height: 100%;
  min-height: 0;
}

.picker,
.results {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
}

.toolbar,
.controls {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  margin-bottom: 10px;
}

.toolbar input {
  flex: 1;
  min-width: 140px;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 8px;
  font: inherit;
  font-size: 12px;
  background: var(--bg);
  color: var(--text);
}

button {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 10px;
  font: inherit;
  font-size: 12px;
  background: var(--bg);
  color: var(--text);
  cursor: pointer;
}

button.primary {
  background: color-mix(in srgb, var(--accent) 18%, var(--bg));
  border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  color: var(--accent);
  font-weight: 600;
}

button:disabled {
  opacity: 0.45;
  cursor: default;
}

.meta,
.hint,
.notice {
  margin: 0 0 8px;
  font-size: 12px;
  color: var(--text-muted);
}

.error {
  margin: 0 0 8px;
  font-size: 12px;
  color: #b00020;
}

.app-list,
.table-wrap {
  flex: 1;
  min-height: 0;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 6px;
}

.app-row {
  display: grid;
  grid-template-columns: 18px minmax(80px, 0.7fr) minmax(0, 1.3fr) 36px;
  gap: 8px;
  align-items: center;
  padding: 7px 10px;
  border-bottom: 1px solid var(--border);
  font-size: 12px;
  cursor: pointer;
}

.app-row:hover {
  background: var(--row-hover);
}

.app-row.selected {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
}

.app-name {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.app-key {
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
}

.app-count {
  text-align: right;
  color: var(--text-muted);
}

.phase {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted);
}

.phase[data-phase="preparing"],
.phase[data-phase="measuring"] {
  color: var(--accent);
}

.phase[data-phase="done"] {
  color: var(--status-active);
}

.stats {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin-bottom: 10px;
}

.stat {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px 10px;
  background: var(--bg);
}

.stat-label {
  font-size: 11px;
  color: var(--text-muted);
}

.stat-value {
  margin-top: 2px;
  font-size: 18px;
  font-weight: 650;
  font-variant-numeric: tabular-nums;
}

.stat-value.accent {
  color: var(--accent);
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

th,
td {
  padding: 7px 8px;
  border-bottom: 1px solid var(--border);
  text-align: left;
  vertical-align: top;
}

th {
  position: sticky;
  top: 0;
  background: var(--bg-elevated);
  font-weight: 600;
  z-index: 1;
}

.mono {
  font-family: var(--mono);
  font-size: 11px;
}

.clickable {
  cursor: copy;
}

.empty {
  padding: 16px;
  color: var(--text-muted);
  text-align: center;
}

tbody tr.is-baseline td {
  color: var(--text-muted);
  font-weight: 400;
}

tbody tr.is-new td {
  font-weight: 700;
}

@media (max-width: 960px) {
  .layout {
    grid-template-columns: 1fr;
  }
}
</style>
