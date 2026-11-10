import { defineStore } from "pinia";
import { computed, reactive, ref } from "vue";
import { fetchConnections } from "../services/connections";
import {
  mapConnectionRow,
  type ConnectionSortBy,
  type ConnectionTableRow,
  type ConnectionsFilter,
  type ConnectionsPage,
  type SortDir,
} from "../types/connections";
import { useMonitoringStore } from "./monitoring";

export const useConnectionsStore = defineStore("connections", () => {
  const monitoring = useMonitoringStore();
  const page = ref<ConnectionsPage | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const pageNumber = ref(1);
  const pageSize = ref(50);
  const sortBy = ref<ConnectionSortBy>("startedAt");
  const sortDir = ref<SortDir>("desc");
  const filter = reactive<ConnectionsFilter>({
    text: "",
    processName: "",
    remoteIp: "",
    domain: "",
    network: "",
    protocol: "",
    startedFrom: "",
    startedTo: "",
    activeOnly: false,
  });
  const selectedIds = ref<Set<number>>(new Set());

  const rows = computed<ConnectionTableRow[]>(() => {
    if (!page.value) {
      return [];
    }
    const format = monitoring.settings?.dateTimeFormat ?? "iso";
    return page.value.rows.map((row) => mapConnectionRow(row, Date.now(), format));
  });

  const totalPages = computed(() => {
    if (!page.value || page.value.pageSize <= 0) {
      return 1;
    }
    return Math.max(1, Math.ceil(page.value.total / page.value.pageSize));
  });

  async function load(options: { resetPage?: boolean } = {}) {
    if (options.resetPage) {
      pageNumber.value = 1;
    }
    loading.value = true;
    error.value = null;
    try {
      page.value = await fetchConnections({
        page: pageNumber.value,
        pageSize: pageSize.value,
        sortBy: sortBy.value,
        sortDir: sortDir.value,
        filter: { ...filter },
      });
      // Drop selections that are no longer on the current page.
      const visible = new Set(page.value.rows.map((row) => row.id));
      selectedIds.value = new Set(
        [...selectedIds.value].filter((id) => visible.has(id)),
      );
    } catch (err) {
      error.value = err instanceof Error ? err.message : String(err);
      page.value = null;
    } finally {
      loading.value = false;
    }
  }

  function toggleSort(column: ConnectionSortBy) {
    if (sortBy.value === column) {
      sortDir.value = sortDir.value === "asc" ? "desc" : "asc";
    } else {
      sortBy.value = column;
      sortDir.value = column === "startedAt" ? "desc" : "asc";
    }
    void load({ resetPage: true });
  }

  function toggleSelected(id: number) {
    const next = new Set(selectedIds.value);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    selectedIds.value = next;
  }

  function toggleSelectAllVisible() {
    const ids = rows.value.map((row) => row.id);
    const allSelected = ids.length > 0 && ids.every((id) => selectedIds.value.has(id));
    const next = new Set(selectedIds.value);
    if (allSelected) {
      for (const id of ids) {
        next.delete(id);
      }
    } else {
      for (const id of ids) {
        next.add(id);
      }
    }
    selectedIds.value = next;
  }

  function clearSelection() {
    selectedIds.value = new Set();
  }

  async function goToPage(next: number) {
    pageNumber.value = Math.max(1, Math.min(totalPages.value, next));
    await load();
  }

  return {
    page,
    loading,
    error,
    pageNumber,
    pageSize,
    sortBy,
    sortDir,
    filter,
    selectedIds,
    rows,
    totalPages,
    load,
    toggleSort,
    toggleSelected,
    toggleSelectAllVisible,
    clearSelection,
    goToPage,
  };
});
