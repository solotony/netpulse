<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";

const route = useRoute();

const navItems = [
  { to: "/connections", label: "Connections" },
  { to: "/processes", label: "Processes" },
  { to: "/ip-addresses", label: "IP Addresses" },
  { to: "/domains", label: "Domains" },
  { to: "/networks", label: "Networks" },
  { to: "/statistics", label: "Statistics" },
  { to: "/settings", label: "Settings" },
] as const;

const title = computed(() => String(route.meta.title ?? "Network Monitor"));
</script>

<template>
  <div class="shell">
    <aside class="sidebar">
      <div class="brand">Network Monitor</div>
      <nav class="nav">
        <RouterLink
          v-for="item in navItems"
          :key="item.to"
          :to="item.to"
          class="nav-link"
          active-class="active"
        >
          {{ item.label }}
        </RouterLink>
      </nav>
    </aside>
    <section class="main">
      <header class="header">
        <h1>{{ title }}</h1>
      </header>
      <div class="content">
        <slot />
      </div>
    </section>
  </div>
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: 220px 1fr;
  height: 100%;
}

.sidebar {
  border-right: 1px solid var(--border);
  background: var(--bg-sidebar);
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.brand {
  padding: 16px 14px 12px;
  font-size: 13px;
  font-weight: 650;
  letter-spacing: 0.01em;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 0 8px 12px;
}

.nav-link {
  display: block;
  padding: 8px 10px;
  border-radius: 6px;
  color: var(--text);
  font-size: 13px;
}

.nav-link:hover {
  background: color-mix(in srgb, var(--bg-sidebar-active) 70%, transparent);
}

.nav-link.active {
  background: var(--bg-sidebar-active);
  font-weight: 600;
}

.main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: var(--bg);
}

.header {
  border-bottom: 1px solid var(--border);
  background: var(--bg-elevated);
  padding: 12px 16px;
}

.header h1 {
  margin: 0;
  font-size: 16px;
  font-weight: 650;
}

.content {
  flex: 1;
  min-height: 0;
  padding: 12px 16px 16px;
}
</style>
