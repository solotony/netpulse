<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import AppShell from "../layouts/AppShell.vue";
import { useMonitoringStore } from "../stores/monitoring";
import type { DateTimeFormatPreference, ThemePreference } from "../types/monitoring";
import { formatBytes, formatDateTime } from "../utils/format";

const store = useMonitoringStore();
const confirmClear = ref(false);
const retentionDaysDraft = ref(30);
const cacheTtlDraft = ref(86400);

const dateFormat = computed(
  () => store.settings?.dateTimeFormat ?? ("iso" as DateTimeFormatPreference),
);

onMounted(async () => {
  await Promise.all([
    store.refreshSettings(),
    store.refreshStatus(),
    store.refreshDatabaseInfo(),
  ]);
  retentionDaysDraft.value = store.settings?.historyRetentionDays ?? 30;
  cacheTtlDraft.value = store.settings?.cacheTtlSeconds ?? 86400;
});

async function onClearHistory() {
  if (!confirmClear.value) {
    confirmClear.value = true;
    return;
  }
  confirmClear.value = false;
  await store.clearConnectionHistory();
}

async function saveRetention() {
  const days = Math.max(1, Math.floor(Number(retentionDaysDraft.value) || 1));
  retentionDaysDraft.value = days;
  await store.updateHistoryRetention(
    store.settings?.historyRetentionEnabled ?? false,
    days,
  );
}

async function saveCacheTtl() {
  const seconds = Math.max(0, Math.floor(Number(cacheTtlDraft.value) || 0));
  cacheTtlDraft.value = seconds;
  await store.updateCacheTtlSeconds(seconds);
}
</script>

<template>
  <AppShell>
    <p v-if="store.error" class="error">{{ store.error }}</p>

    <section class="section">
      <h2>Monitoring</h2>
      <p class="status-line">
        Status:
        <span class="state" :data-state="store.status?.state ?? 'stopped'">
          {{ store.status?.state ?? "stopped" }}
        </span>
        <span v-if="store.status?.backend" class="meta">
          ({{ store.status.backend }})
        </span>
      </p>
      <p v-if="store.status?.lastError" class="error">{{ store.status.lastError }}</p>
      <div class="actions">
        <button
          type="button"
          :disabled="store.busy || store.status?.state === 'running' || store.status?.state === 'paused'"
          @click="store.start(false)"
        >
          Start
        </button>
        <button
          type="button"
          :disabled="store.busy || store.status?.state !== 'running'"
          @click="store.pause()"
        >
          Pause
        </button>
        <button
          type="button"
          :disabled="store.busy || store.status?.state !== 'paused'"
          @click="store.resume()"
        >
          Resume
        </button>
        <button
          type="button"
          :disabled="store.busy || store.status?.state === 'stopped'"
          @click="store.stop()"
        >
          Stop
        </button>
      </div>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.startMonitoringAutomatically ?? false"
          :disabled="store.busy || !store.settings"
          @change="
            store.updateStartAutomatically(($event.target as HTMLInputElement).checked)
          "
        />
        Start monitoring automatically
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.autostart ?? false"
          :disabled="store.busy || !store.settings"
          @change="store.updateAutostart(($event.target as HTMLInputElement).checked)"
        />
        Start application at login
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.backgroundMode ?? true"
          :disabled="store.busy || !store.settings"
          @change="store.updateBackgroundMode(($event.target as HTMLInputElement).checked)"
        />
        Run in background after closing window
      </label>
    </section>

    <section class="section">
      <h2>Data</h2>
      <dl class="meta-list">
        <div>
          <dt>Database location</dt>
          <dd class="mono">{{ store.databaseInfo?.path ?? "—" }}</dd>
        </div>
        <div>
          <dt>Database size</dt>
          <dd>{{ formatBytes(store.databaseInfo?.sizeBytes) }}</dd>
        </div>
        <div>
          <dt>First recorded event</dt>
          <dd>
            {{ formatDateTime(store.databaseInfo?.firstRecordedAt, dateFormat) }}
          </dd>
        </div>
        <div>
          <dt>Last recorded event</dt>
          <dd>
            {{ formatDateTime(store.databaseInfo?.lastRecordedAt, dateFormat) }}
          </dd>
        </div>
        <div>
          <dt>Connections stored</dt>
          <dd>{{ store.databaseInfo?.connectionCount ?? 0 }}</dd>
        </div>
      </dl>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.historyRetentionEnabled ?? false"
          :disabled="store.busy || !store.settings"
          @change="
            store.updateHistoryRetention(
              ($event.target as HTMLInputElement).checked,
              Math.max(1, Math.floor(Number(retentionDaysDraft) || 30)),
            )
          "
        />
        Automatically delete old history
      </label>
      <label class="field">
        <span>Retention days</span>
        <input
          type="number"
          min="1"
          step="1"
          v-model.number="retentionDaysDraft"
          :disabled="store.busy || !store.settings"
          @change="saveRetention()"
        />
      </label>
      <div class="actions">
        <button
          type="button"
          class="danger"
          :disabled="store.busy"
          @click="onClearHistory()"
        >
          {{ confirmClear ? "Confirm clear history" : "Clear history" }}
        </button>
        <button
          v-if="confirmClear"
          type="button"
          :disabled="store.busy"
          @click="confirmClear = false"
        >
          Cancel
        </button>
      </div>
    </section>

    <section class="section">
      <h2>Enrichment</h2>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.dnsResolution ?? true"
          :disabled="store.busy || !store.settings"
          @change="store.updateDnsResolution(($event.target as HTMLInputElement).checked)"
        />
        Enable DNS resolution
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.reverseDns ?? true"
          :disabled="store.busy || !store.settings"
          @change="store.updateReverseDns(($event.target as HTMLInputElement).checked)"
        />
        Enable reverse DNS
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.ipEnrichment ?? true"
          :disabled="store.busy || !store.settings"
          @change="store.updateIpEnrichment(($event.target as HTMLInputElement).checked)"
        />
        Enable IP registration lookups (RDAP)
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.asnInfo ?? true"
          :disabled="store.busy || !store.settings"
          @change="store.updateAsnInfo(($event.target as HTMLInputElement).checked)"
        />
        Enable ASN information
      </label>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.countryInfo ?? true"
          :disabled="store.busy || !store.settings"
          @change="store.updateCountryInfo(($event.target as HTMLInputElement).checked)"
        />
        Enable country information
      </label>
      <label class="field">
        <span>Cache TTL (seconds, 0 = never expire)</span>
        <input
          type="number"
          min="0"
          step="1"
          v-model.number="cacheTtlDraft"
          :disabled="store.busy || !store.settings"
          @change="saveCacheTtl()"
        />
      </label>
    </section>

    <section class="section">
      <h2>Interface</h2>
      <label class="field">
        <span>Language</span>
        <select
          :value="store.settings?.language ?? 'en'"
          :disabled="store.busy || !store.settings"
          @change="store.updateLanguage(($event.target as HTMLSelectElement).value)"
        >
          <option value="en">English</option>
        </select>
      </label>
      <label class="field">
        <span>Theme</span>
        <select
          :value="store.settings?.theme ?? 'system'"
          :disabled="store.busy || !store.settings"
          @change="
            store.updateTheme(($event.target as HTMLSelectElement).value as ThemePreference)
          "
        >
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </label>
      <label class="field">
        <span>Date and time format</span>
        <select
          :value="store.settings?.dateTimeFormat ?? 'iso'"
          :disabled="store.busy || !store.settings"
          @change="
            store.updateDateTimeFormat(
              ($event.target as HTMLSelectElement).value as DateTimeFormatPreference,
            )
          "
        >
          <option value="iso">ISO 8601</option>
          <option value="locale">Locale</option>
          <option value="locale_seconds">Locale with seconds</option>
        </select>
      </label>
    </section>
  </AppShell>
</template>

<style scoped>
.section {
  margin-bottom: 24px;
}

.section h2 {
  margin: 0 0 8px;
  font-size: 13px;
  font-weight: 650;
}

.status-line,
.meta,
.toggle,
.field,
.meta-list {
  font-size: 12px;
  color: var(--text);
}

.meta {
  color: var(--text-muted);
  margin-left: 4px;
}

.state {
  text-transform: lowercase;
  font-weight: 600;
}

.state[data-state="running"] {
  color: var(--status-active);
}

.state[data-state="paused"] {
  color: #a15c00;
}

.state[data-state="failed"] {
  color: #b42318;
}

.state[data-state="stopped"] {
  color: var(--text-muted);
}

.actions {
  display: flex;
  gap: 8px;
  margin-top: 10px;
  flex-wrap: wrap;
}

.actions button,
.field select,
.field input {
  border: 1px solid var(--border);
  background: var(--bg-elevated);
  color: var(--text);
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 12px;
}

.actions button {
  cursor: pointer;
}

.actions button:disabled,
.field select:disabled,
.field input:disabled {
  opacity: 0.55;
  cursor: default;
}

.actions button.danger {
  border-color: #b42318;
  color: #b42318;
}

.toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 8px 0;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 10px 0;
  max-width: 360px;
}

.meta-list {
  display: grid;
  gap: 8px;
  margin: 0 0 12px;
}

.meta-list div {
  display: grid;
  grid-template-columns: 160px 1fr;
  gap: 8px;
}

.meta-list dt {
  color: var(--text-muted);
  margin: 0;
}

.meta-list dd {
  margin: 0;
  word-break: break-all;
}

.mono {
  font-family: var(--mono);
}

.error {
  color: #b42318;
  font-size: 12px;
  margin: 0 0 10px;
}
</style>
