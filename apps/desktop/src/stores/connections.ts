import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { fetchMockConnections } from "../services/connections";
import {
  mapConnectionRow,
  type ConnectionTableRow,
  type ConnectionsPage,
} from "../types/connections";

export const useConnectionsStore = defineStore("connections", () => {
  const page = ref<ConnectionsPage | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const rows = computed<ConnectionTableRow[]>(() => {
    if (!page.value) {
      return [];
    }
    return page.value.rows.map((row) => mapConnectionRow(row));
  });

  async function load(pageNumber = 1, pageSize = 50) {
    loading.value = true;
    error.value = null;
    try {
      page.value = await fetchMockConnections({ page: pageNumber, pageSize });
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
      page.value = null;
    } finally {
      loading.value = false;
    }
  }

  return {
    page,
    loading,
    error,
    rows,
    load,
  };
});
