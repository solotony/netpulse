<script setup lang="ts">
import { onMounted } from "vue";
import AppShell from "../layouts/AppShell.vue";
import { useConnectionsStore } from "../stores/connections";

const store = useConnectionsStore();

onMounted(() => {
  void store.load();
});
</script>

<template>
  <AppShell>
    <div class="toolbar">
      <button type="button" :disabled="store.loading" @click="store.load()">
        {{ store.loading ? "Loading…" : "Refresh" }}
      </button>
      <span v-if="store.page" class="meta">
        {{ store.page.total }} connection{{ store.page.total === 1 ? "" : "s" }}
      </span>
    </div>

    <p v-if="store.error" class="error">{{ store.error }}</p>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Time</th>
            <th>Process</th>
            <th>PID</th>
            <th>Protocol</th>
            <th>Direction</th>
            <th>Local Address</th>
            <th>Remote Address</th>
            <th>Domain</th>
            <th>Network</th>
            <th>Organization</th>
            <th>Duration</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="!store.loading && store.rows.length === 0">
            <td colspan="12" class="empty">No connections yet.</td>
          </tr>
          <tr v-for="row in store.rows" :key="row.id">
            <td class="mono">{{ row.time }}</td>
            <td>{{ row.process }}</td>
            <td class="mono">{{ row.pid }}</td>
            <td>{{ row.protocol }}</td>
            <td>{{ row.direction }}</td>
            <td class="mono">{{ row.localAddress }}</td>
            <td class="mono">{{ row.remoteAddress }}</td>
            <td>{{ row.domain }}</td>
            <td>{{ row.network }}</td>
            <td>{{ row.organization }}</td>
            <td>{{ row.duration }}</td>
            <td>
              <span class="status" :data-status="row.status">{{ row.status }}</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </AppShell>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 10px;
}

.toolbar button {
  border: 1px solid var(--border);
  background: var(--bg-elevated);
  color: var(--text);
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 12px;
  cursor: pointer;
}

.toolbar button:disabled {
  opacity: 0.6;
  cursor: default;
}

.meta,
.error,
.empty {
  font-size: 12px;
  color: var(--text-muted);
}

.error {
  color: #b42318;
  margin: 0 0 8px;
}

.table-wrap {
  height: calc(100% - 40px);
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

tbody tr:hover td {
  background: var(--row-hover);
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
