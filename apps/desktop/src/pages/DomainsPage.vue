<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import AppShell from "../layouts/AppShell.vue";
import ConnectionHistoryPanel from "../components/ConnectionHistoryPanel.vue";
import ManualAddBar from "../components/ManualAddBar.vue";
import { fetchDomains, type DomainRow } from "../services/connections";
import type { ConnectionsFilter } from "../types/connections";

const loading = ref(false);
const error = ref<string | null>(null);
const rows = ref<DomainRow[]>([]);
const total = ref(0);
const page = ref(1);
const pageSize = 50;
const text = ref("");
const selected = ref<DomainRow | null>(null);

const totalPages = computed(() => Math.max(1, Math.ceil(total.value / pageSize)));
const historyFilter = computed<ConnectionsFilter | null>(() =>
  selected.value ? { domain: selected.value.domain } : null,
);

async function load(resetPage = false) {
  if (resetPage) page.value = 1;
  loading.value = true;
  error.value = null;
  try {
    const result = await fetchDomains({
      page: page.value,
      pageSize,
      text: text.value,
    });
    rows.value = result.rows;
    total.value = result.total;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    rows.value = [];
    total.value = 0;
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void load();
});
</script>

<template>
  <AppShell>
    <div class="layout" :class="{ split: selected }">
      <section class="main-pane">
        <ManualAddBar
          kind="domain"
          placeholder="Add domain (e.g. chatgpt.com)…"
          @added="load(true)"
        />

        <form class="toolbar" @submit.prevent="load(true)">
          <input v-model="text" type="search" placeholder="Search domains…" />
          <button type="submit" :disabled="loading">{{ loading ? "Loading…" : "Search" }}</button>
          <span class="meta">{{ total }} domain{{ total === 1 ? "" : "s" }}</span>
        </form>

        <p v-if="error" class="error">{{ error }}</p>

        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Domain</th>
                <th>IPs</th>
                <th>Processes</th>
                <th>First seen</th>
                <th>Last seen</th>
                <th>Connections</th>
                <th>Resolution source</th>
              </tr>
            </thead>
            <tbody>
              <tr v-if="!loading && rows.length === 0">
                <td colspan="7" class="empty">No domains yet.</td>
              </tr>
              <tr
                v-for="row in rows"
                :key="row.id"
                :class="{ selected: selected?.id === row.id }"
                @click="selected = row"
              >
                <td>{{ row.domain }}</td>
                <td>{{ row.ipCount }}</td>
                <td>{{ row.processCount }}</td>
                <td class="mono">{{ row.firstSeenAt }}</td>
                <td class="mono">{{ row.lastSeenAt }}</td>
                <td>{{ row.connectionCount }}</td>
                <td>{{ row.sources || "—" }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <div class="pager">
          <button type="button" :disabled="page <= 1 || loading" @click="page--; load()">
            Previous
          </button>
          <span class="meta">Page {{ page }} / {{ totalPages }}</span>
          <button
            type="button"
            :disabled="page >= totalPages || loading"
            @click="page++; load()"
          >
            Next
          </button>
        </div>
      </section>

      <ConnectionHistoryPanel
        v-if="selected && historyFilter"
        :title="selected.domain"
        :filter="historyFilter"
        @close="selected = null"
      />
    </div>
  </AppShell>
</template>

<style scoped>
.layout {
  display: grid;
  grid-template-columns: 1fr;
  gap: 12px;
  height: 100%;
  min-height: 0;
}

.layout.split {
  grid-template-columns: minmax(0, 1.4fr) minmax(280px, 1fr);
}

.main-pane {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.toolbar,
.pager {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.pager {
  margin-top: 10px;
  margin-bottom: 0;
}

.toolbar input {
  flex: 1;
  min-width: 160px;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 8px;
  font: inherit;
  font-size: 12px;
  background: var(--bg-elevated);
  color: var(--text);
}

button {
  border: 1px solid var(--border);
  background: var(--bg-elevated);
  color: var(--text);
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 12px;
  cursor: pointer;
}

button:disabled {
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
  flex: 1;
  min-height: 0;
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
}

th {
  position: sticky;
  top: 0;
  background: var(--bg-elevated);
  font-weight: 650;
  z-index: 1;
}

tbody tr {
  cursor: pointer;
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
</style>
