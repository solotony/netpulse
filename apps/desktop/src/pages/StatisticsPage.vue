<script setup lang="ts">
import { onMounted, ref } from "vue";
import AppShell from "../layouts/AppShell.vue";
import { fetchStatistics } from "../services/connections";
import {
  normalizeStatsQuery,
  type StatsGrouping,
  type StatsTable,
} from "../types/connections";

const groupings: Array<{ value: StatsGrouping; label: string }> = [
  { value: "process", label: "Process" },
  { value: "ip", label: "IP address" },
  { value: "domain", label: "Domain" },
  { value: "network", label: "Network" },
  { value: "organization", label: "Organization" },
  { value: "day", label: "Day" },
  { value: "week", label: "Week" },
  { value: "month", label: "Month" },
];

const defaults = normalizeStatsQuery();
const groupBy = ref<StatsGrouping>(defaults.groupBy);
const from = ref(defaults.from);
const to = ref(defaults.to);
const loading = ref(false);
const error = ref<string | null>(null);
const table = ref<StatsTable | null>(null);

async function load() {
  loading.value = true;
  error.value = null;
  try {
    table.value = await fetchStatistics({
      groupBy: groupBy.value,
      from: from.value,
      to: to.value,
    });
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    table.value = null;
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
    <form class="toolbar" @submit.prevent="load">
      <label>
        Group by
        <select v-model="groupBy">
          <option v-for="item in groupings" :key="item.value" :value="item.value">
            {{ item.label }}
          </option>
        </select>
      </label>
      <label>
        From
        <input v-model="from" type="date" />
      </label>
      <label>
        To
        <input v-model="to" type="date" />
      </label>
      <button type="submit" :disabled="loading">
        {{ loading ? "Loading…" : "Apply" }}
      </button>
      <span v-if="table" class="meta">{{ table.rows.length }} row{{ table.rows.length === 1 ? "" : "s" }}</span>
    </form>

    <p v-if="error" class="error">{{ error }}</p>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th v-for="column in table?.columns ?? ['—', 'Connections', '—', '—']" :key="column">
              {{ column }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="!loading && (!table || table.rows.length === 0)">
            <td colspan="4" class="empty">No statistics for this range.</td>
          </tr>
          <tr v-for="(row, index) in table?.rows ?? []" :key="`${row.label}-${index}`">
            <td>{{ row.label || "—" }}</td>
            <td class="num">{{ row.connections }}</td>
            <td class="num">{{ row.metricA }}</td>
            <td class="num">{{ row.metricB }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </AppShell>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: end;
  margin-bottom: 10px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elevated);
}

.toolbar label {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 11px;
  color: var(--text-muted);
}

.toolbar input,
.toolbar select {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 5px 8px;
  font: inherit;
  font-size: 12px;
  color: var(--text);
  background: var(--bg);
}

button {
  border: 1px solid var(--border);
  background: var(--bg);
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
  height: calc(100% - 80px);
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

.num {
  text-align: right;
  font-variant-numeric: tabular-nums;
}

tbody tr:hover td {
  background: var(--row-hover);
}
</style>
