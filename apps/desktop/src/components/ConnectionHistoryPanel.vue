<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { fetchConnections } from "../services/connections";
import { exportConnections, type ExportFormat } from "../services/export";
import { useMonitoringStore } from "../stores/monitoring";
import {
  mapConnectionRow,
  type ConnectionTableRow,
  type ConnectionsFilter,
} from "../types/connections";

const props = defineProps<{
  title: string;
  filter: ConnectionsFilter;
}>();

const emit = defineEmits<{
  close: [];
}>();

const monitoring = useMonitoringStore();
const loading = ref(false);
const exporting = ref(false);
const error = ref<string | null>(null);
const exportNotice = ref<string | null>(null);
const rows = ref<ConnectionTableRow[]>([]);
const total = ref(0);
let exportTimer: ReturnType<typeof setTimeout> | null = null;

const filterKey = computed(() => JSON.stringify(props.filter));

async function load() {
  loading.value = true;
  error.value = null;
  try {
    const page = await fetchConnections({
      page: 1,
      pageSize: 100,
      sortBy: "startedAt",
      sortDir: "desc",
      filter: props.filter,
    });
    const format = monitoring.settings?.dateTimeFormat ?? "iso";
    rows.value = page.rows.map((row) => mapConnectionRow(row, Date.now(), format));
    total.value = page.total;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    rows.value = [];
    total.value = 0;
  } finally {
    loading.value = false;
  }
}

function showExportNotice(message: string) {
  exportNotice.value = message;
  if (exportTimer) {
    clearTimeout(exportTimer);
  }
  exportTimer = setTimeout(() => {
    exportNotice.value = null;
  }, 3500);
}

async function onExport(format: ExportFormat) {
  if (exporting.value) {
    return;
  }
  exporting.value = true;
  try {
    const result = await exportConnections({
      format,
      filter: props.filter,
    });
    if (result.cancelled) {
      showExportNotice("Export cancelled");
    } else {
      showExportNotice(`Exported ${result.rowsWritten} row(s)`);
    }
  } catch (err) {
    showExportNotice(err instanceof Error ? err.message : String(err));
  } finally {
    exporting.value = false;
  }
}

watch(
  filterKey,
  () => {
    void load();
  },
  { immediate: true },
);
</script>

<template>
  <aside class="panel">
    <header class="panel-header">
      <div>
        <h2>{{ title }}</h2>
        <p class="meta">{{ total }} connection{{ total === 1 ? "" : "s" }}</p>
        <p v-if="exportNotice" class="meta notice">{{ exportNotice }}</p>
      </div>
      <div class="header-actions">
        <button
          type="button"
          class="close"
          :disabled="exporting || loading || total === 0"
          @click="onExport('txt')"
        >
          Export .txt
        </button>
        <button
          type="button"
          class="close"
          :disabled="exporting || loading || total === 0"
          @click="onExport('tsv')"
        >
          Export .tsv
        </button>
        <button type="button" class="close" @click="emit('close')">Close</button>
      </div>
    </header>

    <p v-if="error" class="error">{{ error }}</p>
    <p v-else-if="loading" class="meta">Loading…</p>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Time</th>
            <th>Process</th>
            <th>Protocol</th>
            <th>Remote</th>
            <th>Domain</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="!loading && rows.length === 0">
            <td colspan="6" class="empty">No related connections.</td>
          </tr>
          <tr v-for="row in rows" :key="row.id">
            <td class="mono">{{ row.time }}</td>
            <td>{{ row.process }}</td>
            <td>{{ row.protocol }}</td>
            <td class="mono">{{ row.remoteAddress }}</td>
            <td>{{ row.domain }}</td>
            <td>
              <span class="status" :data-status="row.status">{{ row.status }}</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </aside>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  min-width: 0;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elevated);
  overflow: hidden;
}

.panel-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
}

.panel-header h2 {
  margin: 0;
  font-size: 13px;
  font-weight: 650;
}

.header-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  justify-content: flex-end;
}

.close {
  border: 1px solid var(--border);
  background: var(--bg);
  color: var(--text);
  border-radius: 6px;
  padding: 4px 8px;
  font-size: 12px;
  cursor: pointer;
}

.close:disabled {
  opacity: 0.6;
  cursor: default;
}

.notice {
  color: var(--accent);
}

.meta,
.error,
.empty {
  font-size: 12px;
  color: var(--text-muted);
}

.panel-header .meta {
  margin: 2px 0 0;
}

.error {
  color: #b42318;
  margin: 8px 12px;
}

.meta {
  margin: 8px 12px;
}

.table-wrap {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

th,
td {
  padding: 7px 10px;
  border-bottom: 1px solid var(--border);
  text-align: left;
  white-space: nowrap;
}

th {
  position: sticky;
  top: 0;
  background: var(--bg-elevated);
  font-weight: 650;
}

.mono {
  font-family: var(--mono);
  font-size: 11px;
}

.status[data-status="active"] {
  color: var(--status-active);
  font-weight: 600;
}

.status[data-status="closed"] {
  color: var(--status-closed);
}
</style>
