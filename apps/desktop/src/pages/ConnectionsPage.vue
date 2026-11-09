<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import AppShell from "../layouts/AppShell.vue";
import { useConnectionsStore } from "../stores/connections";
import { useMonitoringStore } from "../stores/monitoring";
import { CONNECTION_COLUMNS, type ConnectionSortBy } from "../types/connections";
import { copyText } from "../utils/clipboard";

const store = useConnectionsStore();
const monitoring = useMonitoringStore();
const copyNotice = ref<string | null>(null);
let copyTimer: ReturnType<typeof setTimeout> | null = null;

const allVisibleSelected = computed(() => {
  const ids = store.rows.map((row) => row.id);
  return ids.length > 0 && ids.every((id) => store.selectedIds.has(id));
});

onMounted(() => {
  void store.load();
  void monitoring.refreshStatus();
});

function sortIndicator(column: ConnectionSortBy | null): string {
  if (!column || store.sortBy !== column) {
    return "";
  }
  return store.sortDir === "asc" ? " ▲" : " ▼";
}

async function onCopyCell(value: string) {
  const ok = await copyText(value);
  copyNotice.value = ok ? "Copied" : "Copy failed";
  if (copyTimer) {
    clearTimeout(copyTimer);
  }
  copyTimer = setTimeout(() => {
    copyNotice.value = null;
  }, 1200);
}

function cellValue(
  row: (typeof store.rows)[number],
  columnId: (typeof CONNECTION_COLUMNS)[number]["id"],
): string {
  switch (columnId) {
    case "time":
      return row.time;
    case "process":
      return row.process;
    case "pid":
      return String(row.pid);
    case "protocol":
      return row.protocol;
    case "direction":
      return row.direction;
    case "local":
      return row.localAddress;
    case "remote":
      return row.remoteAddress;
    case "domain":
      return row.domain;
    case "network":
      return row.network;
    case "organization":
      return row.organization;
    case "duration":
      return row.duration;
    case "status":
      return row.status;
    default:
      return "";
  }
}
</script>

<template>
  <AppShell>
    <div class="toolbar">
      <button type="button" :disabled="store.loading" @click="store.load()">
        {{ store.loading ? "Loading…" : "Refresh" }}
      </button>
      <span v-if="store.page" class="meta">
        {{ store.page.total }} connection{{ store.page.total === 1 ? "" : "s" }}
        <template v-if="store.selectedIds.size">
          · {{ store.selectedIds.size }} selected
        </template>
      </span>
      <span v-if="copyNotice" class="meta notice">{{ copyNotice }}</span>
      <span class="collector" :data-state="monitoring.status?.state ?? 'stopped'">
        Collector: {{ monitoring.status?.state ?? "stopped" }}
      </span>
    </div>

    <form class="filters" @submit.prevent="store.load({ resetPage: true })">
      <label>
        Search
        <input v-model="store.filter.text" type="search" placeholder="Process, IP, domain…" />
      </label>
      <label>
        Process
        <input v-model="store.filter.processName" type="text" />
      </label>
      <label>
        IP
        <input v-model="store.filter.remoteIp" type="text" />
      </label>
      <label>
        Domain
        <input v-model="store.filter.domain" type="text" />
      </label>
      <label>
        Network
        <input v-model="store.filter.network" type="text" />
      </label>
      <label>
        Protocol
        <select v-model="store.filter.protocol">
          <option value="">Any</option>
          <option value="tcp">TCP</option>
          <option value="udp">UDP</option>
        </select>
      </label>
      <label>
        From
        <input v-model="store.filter.startedFrom" type="datetime-local" />
      </label>
      <label>
        To
        <input v-model="store.filter.startedTo" type="datetime-local" />
      </label>
      <label class="check">
        <input v-model="store.filter.activeOnly" type="checkbox" />
        Active only
      </label>
      <button type="submit" :disabled="store.loading">Apply</button>
    </form>

    <p v-if="store.error" class="error">{{ store.error }}</p>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th class="check-col">
              <input
                type="checkbox"
                :checked="allVisibleSelected"
                :disabled="store.rows.length === 0"
                @change="store.toggleSelectAllVisible()"
              />
            </th>
            <th
              v-for="column in CONNECTION_COLUMNS"
              :key="column.id"
              :class="{ sortable: column.sortBy }"
              @click="column.sortBy && store.toggleSort(column.sortBy)"
            >
              {{ column.label }}{{ sortIndicator(column.sortBy) }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="!store.loading && store.rows.length === 0">
            <td :colspan="CONNECTION_COLUMNS.length + 1" class="empty">No connections yet.</td>
          </tr>
          <tr
            v-for="row in store.rows"
            :key="row.id"
            :class="{ selected: store.selectedIds.has(row.id) }"
          >
            <td class="check-col">
              <input
                type="checkbox"
                :checked="store.selectedIds.has(row.id)"
                @change="store.toggleSelected(row.id)"
              />
            </td>
            <td
              v-for="column in CONNECTION_COLUMNS"
              :key="column.id"
              :class="{ mono: column.id === 'time' || column.id === 'pid' || column.id === 'local' || column.id === 'remote' }"
              :title="'Double-click to copy'"
              @dblclick="onCopyCell(row.cells[column.id] ?? cellValue(row, column.id))"
            >
              <template v-if="column.id === 'status'">
                <span class="status" :data-status="row.status">{{ row.status }}</span>
              </template>
              <template v-else>
                {{ cellValue(row, column.id) }}
              </template>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div class="pager">
      <button
        type="button"
        :disabled="store.loading || store.pageNumber <= 1"
        @click="store.goToPage(store.pageNumber - 1)"
      >
        Previous
      </button>
      <span class="meta">
        Page {{ store.pageNumber }} / {{ store.totalPages }}
      </span>
      <button
        type="button"
        :disabled="store.loading || store.pageNumber >= store.totalPages"
        @click="store.goToPage(store.pageNumber + 1)"
      >
        Next
      </button>
    </div>
  </AppShell>
</template>

<style scoped>
.toolbar,
.pager {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 10px;
}

.pager {
  margin-top: 10px;
  margin-bottom: 0;
}

.toolbar button,
.pager button,
.filters button {
  border: 1px solid var(--border);
  background: var(--bg-elevated);
  color: var(--text);
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 12px;
  cursor: pointer;
}

.toolbar button:disabled,
.pager button:disabled,
.filters button:disabled {
  opacity: 0.6;
  cursor: default;
}

.filters {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 10px;
  align-items: end;
  margin-bottom: 10px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elevated);
}

.filters label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 11px;
  color: var(--text-muted);
}

.filters input,
.filters select {
  min-width: 120px;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 5px 8px;
  font: inherit;
  font-size: 12px;
  color: var(--text);
  background: var(--bg);
}

.filters .check {
  flex-direction: row;
  align-items: center;
  gap: 6px;
  padding-bottom: 6px;
}

.meta,
.collector,
.error,
.empty {
  font-size: 12px;
  color: var(--text-muted);
}

.notice {
  color: var(--accent);
}

.collector {
  margin-left: auto;
  text-transform: lowercase;
}

.collector[data-state="running"] {
  color: var(--status-active);
  font-weight: 600;
}

.collector[data-state="paused"] {
  color: #a15c00;
  font-weight: 600;
}

.collector[data-state="failed"] {
  color: #b42318;
  font-weight: 600;
}

.error {
  color: #b42318;
  margin: 0 0 8px;
}

.table-wrap {
  height: calc(100% - 170px);
  min-height: 200px;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elevated);
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}

th,
td {
  padding: 8px 10px;
  border-bottom: 1px solid var(--border);
  text-align: left;
  white-space: nowrap;
  vertical-align: top;
}

th {
  position: sticky;
  top: 0;
  background: var(--bg-elevated);
  font-weight: 650;
  z-index: 1;
}

th.sortable {
  cursor: pointer;
  user-select: none;
}

th.sortable:hover {
  color: var(--accent);
}

.check-col {
  width: 28px;
}

tbody tr:hover td {
  background: var(--row-hover);
}

tbody tr.selected td {
  background: color-mix(in srgb, var(--accent) 12%, var(--bg-elevated));
}

.mono {
  font-family: var(--mono);
  font-size: 11px;
}

.status {
  text-transform: lowercase;
}

.status[data-status="active"] {
  color: var(--status-active);
  font-weight: 600;
}

.status[data-status="closed"] {
  color: var(--status-closed);
}
</style>
