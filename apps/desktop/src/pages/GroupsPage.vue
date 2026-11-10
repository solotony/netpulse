<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import AppShell from "../layouts/AppShell.vue";
import ConnectionHistoryPanel from "../components/ConnectionHistoryPanel.vue";
import {
  createGroup,
  deleteGroup,
  fetchGroupNetworks,
  fetchGroups,
  renameGroup,
  type GroupNetworkRow,
  type GroupRow,
} from "../services/manual";
import type { ConnectionsFilter } from "../types/connections";

const loading = ref(false);
const error = ref<string | null>(null);
const groups = ref<GroupRow[]>([]);
const selected = ref<GroupRow | null>(null);
const members = ref<GroupNetworkRow[]>([]);
const membersLoading = ref(false);
const selectedMember = ref<GroupNetworkRow | null>(null);

const newName = ref("");
const renameValue = ref("");
const busy = ref(false);

const historyFilter = computed<ConnectionsFilter | null>(() =>
  selectedMember.value ? { network: selectedMember.value.cidr } : null,
);

async function loadGroups() {
  loading.value = true;
  error.value = null;
  try {
    groups.value = await fetchGroups();
    if (selected.value) {
      selected.value = groups.value.find((g) => g.id === selected.value?.id) ?? null;
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    groups.value = [];
  } finally {
    loading.value = false;
  }
}

async function loadMembers(groupId: number) {
  membersLoading.value = true;
  try {
    members.value = await fetchGroupNetworks(groupId);
    if (selectedMember.value) {
      selectedMember.value =
        members.value.find((m) => m.id === selectedMember.value?.id) ?? null;
    }
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    members.value = [];
  } finally {
    membersLoading.value = false;
  }
}

async function onCreate() {
  const name = newName.value.trim();
  if (!name || busy.value) return;
  busy.value = true;
  error.value = null;
  try {
    await createGroup(name);
    newName.value = "";
    await loadGroups();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

async function onRename() {
  if (!selected.value || busy.value) return;
  const name = renameValue.value.trim();
  if (!name) return;
  busy.value = true;
  error.value = null;
  try {
    await renameGroup(selected.value.id, name);
    await loadGroups();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

async function onDelete() {
  if (!selected.value || busy.value) return;
  busy.value = true;
  error.value = null;
  try {
    await deleteGroup(selected.value.id);
    selected.value = null;
    members.value = [];
    selectedMember.value = null;
    await loadGroups();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

function selectGroup(row: GroupRow) {
  selected.value = row;
  renameValue.value = row.name;
  selectedMember.value = null;
}

watch(selected, (row) => {
  if (row) {
    void loadMembers(row.id);
  } else {
    members.value = [];
  }
});

onMounted(() => {
  void loadGroups();
});
</script>

<template>
  <AppShell>
    <div class="layout" :class="{ split: selectedMember }">
      <section class="main-pane">
        <form class="toolbar" @submit.prevent="onCreate">
          <input v-model="newName" type="text" placeholder="New group name…" :disabled="busy" />
          <button type="submit" :disabled="busy || !newName.trim()">Create group</button>
          <button type="button" :disabled="loading" @click="loadGroups">
            {{ loading ? "Loading…" : "Refresh" }}
          </button>
          <span class="meta">{{ groups.length }} group{{ groups.length === 1 ? "" : "s" }}</span>
        </form>

        <p v-if="error" class="error">{{ error }}</p>

        <div class="split-inner">
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Group</th>
                  <th>Subnets</th>
                  <th>Created</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="!loading && groups.length === 0">
                  <td colspan="3" class="empty">No groups yet.</td>
                </tr>
                <tr
                  v-for="row in groups"
                  :key="row.id"
                  :class="{ selected: selected?.id === row.id }"
                  @click="selectGroup(row)"
                >
                  <td>{{ row.name }}</td>
                  <td>{{ row.networkCount }}</td>
                  <td class="mono">{{ row.createdAt }}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <div v-if="selected" class="detail">
            <form class="toolbar" @submit.prevent="onRename">
              <input v-model="renameValue" type="text" :disabled="busy" />
              <button type="submit" :disabled="busy || !renameValue.trim()">Rename</button>
              <button type="button" class="danger" :disabled="busy" @click="onDelete">
                Delete
              </button>
            </form>
            <p class="meta">
              Members of {{ selected.name }}
              {{ membersLoading ? "(loading…)" : "" }}
            </p>
            <div class="table-wrap members">
              <table>
                <thead>
                  <tr>
                    <th>CIDR</th>
                    <th>NetName</th>
                    <th>Organization</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-if="!membersLoading && members.length === 0">
                    <td colspan="3" class="empty">No subnets in this group.</td>
                  </tr>
                  <tr
                    v-for="row in members"
                    :key="row.id"
                    :class="{ selected: selectedMember?.id === row.id }"
                    @click="selectedMember = row"
                  >
                    <td class="mono">{{ row.cidr }}</td>
                    <td>{{ row.netName || "—" }}</td>
                    <td>{{ row.organization || "—" }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </section>

      <ConnectionHistoryPanel
        v-if="selectedMember && historyFilter"
        :title="selectedMember.cidr"
        :filter="historyFilter"
        @close="selectedMember = null"
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

.split-inner {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr);
  gap: 12px;
  flex: 1;
  min-height: 0;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
  flex-wrap: wrap;
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

button.danger {
  color: #b42318;
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

.table-wrap,
.detail {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.table-wrap {
  flex: 1;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-elevated);
}

.table-wrap.members {
  flex: 1;
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
