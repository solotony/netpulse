<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";
import AppShell from "../layouts/AppShell.vue";

const route = useRoute();
const title = computed(() => String(route.meta.title ?? "Page"));
</script>

<template>
  <AppShell>
    <p class="placeholder">
      {{ title }} will be available in a later step.
    </p>
  </AppShell>
</template>

<style scoped>
.placeholder {
  margin: 0;
  color: var(--text-muted);
  font-size: 13px;
}
</style>
