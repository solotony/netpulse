<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import {
  addManualEntry,
  fetchGroups,
  type GroupRow,
  type ManualEntryKind,
  type ManualEntryResult,
} from "../services/manual";

const props = defineProps<{
  kind: ManualEntryKind;
  placeholder: string;
}>();

const emit = defineEmits<{
  added: [result: ManualEntryResult];
}>();

const value = ref("");
const groupId = ref<string>("");
const newGroupName = ref("");
const groups = ref<GroupRow[]>([]);
const busy = ref(false);
const error = ref<string | null>(null);
const summary = ref<string | null>(null);

async function loadGroups() {
  try {
    groups.value = await fetchGroups();
  } catch {
    groups.value = [];
  }
}

async function onSubmit() {
  const trimmed = value.value.trim();
  if (!trimmed || busy.value) return;
  busy.value = true;
  error.value = null;
  summary.value = null;
  try {
    const selectedId = groupId.value ? Number(groupId.value) : null;
    const result = await addManualEntry({
      kind: props.kind,
      value: trimmed,
      groupId: selectedId,
      groupName: selectedId ? null : newGroupName.value.trim() || null,
    });
    value.value = "";
    newGroupName.value = "";
    const parts = [
      `Added ${result.kind} ${result.value}`,
      result.addresses.length ? `${result.addresses.length} address(es)` : null,
      result.cidrs.length ? `subnets: ${result.cidrs.join(", ")}` : null,
      result.groupName ? `group: ${result.groupName}` : null,
      ...result.notes,
    ].filter(Boolean);
    summary.value = parts.join(" · ");
    await loadGroups();
    emit("added", result);
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    busy.value = false;
  }
}

watch(groupId, (id) => {
  if (id) newGroupName.value = "";
});

onMounted(() => {
  void loadGroups();
});

defineExpose({ reloadGroups: loadGroups });
</script>

<template>
  <form class="add-bar" @submit.prevent="onSubmit">
    <input v-model="value" type="text" :placeholder="placeholder" :disabled="busy" />
    <select v-model="groupId" :disabled="busy" aria-label="Group">
      <option value="">No group</option>
      <option v-for="g in groups" :key="g.id" :value="String(g.id)">{{ g.name }}</option>
    </select>
    <input
      v-model="newGroupName"
      type="text"
      placeholder="New group name…"
      :disabled="busy || !!groupId"
    />
    <button type="submit" :disabled="busy || !value.trim()">
      {{ busy ? "Adding…" : "Add" }}
    </button>
  </form>
  <p v-if="error" class="error">{{ error }}</p>
  <p v-else-if="summary" class="summary">{{ summary }}</p>
</template>

<style scoped>
.add-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}

.add-bar input,
.add-bar select {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 8px;
  font: inherit;
  font-size: 12px;
  background: var(--bg-elevated);
  color: var(--text);
}

.add-bar input[type="text"]:first-child {
  flex: 1;
  min-width: 160px;
}

.add-bar input[placeholder="New group name…"] {
  min-width: 140px;
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

.error,
.summary {
  font-size: 12px;
  margin: 0 0 8px;
  color: var(--text-muted);
}

.error {
  color: #b42318;
}
</style>
