<script setup lang="ts">
import { onMounted } from "vue";
import { RouterView } from "vue-router";
import { useMonitoringStore } from "./stores/monitoring";

const monitoring = useMonitoringStore();

onMounted(() => {
  void monitoring.refreshSettings();
});
</script>

<template>
  <RouterView />
</template>
