<script setup lang="ts">
import { onMounted } from "vue";
import AppShell from "../layouts/AppShell.vue";
import { useMonitoringStore } from "../stores/monitoring";

const store = useMonitoringStore();

onMounted(() => {
  void store.refreshSettings();
  void store.refreshStatus();
});
</script>

<template>
  <AppShell>
    <p v-if="store.error" class="error">{{ store.error }}</p>

    <section class="section">
      <h2>Collector</h2>
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
    </section>

    <section class="section">
      <h2>Background</h2>
      <label class="toggle">
        <input
          type="checkbox"
          :checked="store.settings?.autostart ?? false"
          :disabled="store.busy || !store.settings"
          @change="store.updateAutostart(($event.target as HTMLInputElement).checked)"
        />
        Start at login
      </label>
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
          :checked="store.settings?.backgroundMode ?? true"
          :disabled="store.busy || !store.settings"
          @change="store.updateBackgroundMode(($event.target as HTMLInputElement).checked)"
        />
        Hide to tray when closing the window
      </label>
    </section>
  </AppShell>
</template>

<style scoped>
.section {
  margin-bottom: 20px;
}

.section h2 {
  margin: 0 0 8px;
  font-size: 13px;
  font-weight: 650;
}

.status-line,
.meta,
.toggle {
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
}

.actions button {
  border: 1px solid var(--border);
  background: var(--bg-elevated);
  color: var(--text);
  border-radius: 6px;
  padding: 6px 10px;
  font-size: 12px;
  cursor: pointer;
}

.actions button:disabled {
  opacity: 0.55;
  cursor: default;
}

.toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 8px 0;
}

.error {
  color: #b42318;
  font-size: 12px;
  margin: 0 0 10px;
}
</style>
