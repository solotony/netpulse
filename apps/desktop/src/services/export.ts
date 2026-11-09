import { invoke } from "@tauri-apps/api/core";
import type { ConnectionsFilter } from "../types/connections";

export type ExportFormat = "txt" | "tsv";

export interface ExportResult {
  path: string;
  rowsWritten: number;
  cancelled: boolean;
}

function toFilterPayload(filter: ConnectionsFilter = {}): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  if (filter.text) payload.text = filter.text;
  if (filter.processId != null) payload.processId = filter.processId;
  if (filter.processName) payload.processName = filter.processName;
  if (filter.remoteIp) payload.remoteIp = filter.remoteIp;
  if (filter.domain) payload.domain = filter.domain;
  if (filter.network) payload.network = filter.network;
  if (filter.protocol) payload.protocol = filter.protocol;
  if (filter.startedFrom) payload.startedFrom = filter.startedFrom;
  if (filter.startedTo) payload.startedTo = filter.startedTo;
  if (filter.activeOnly) payload.activeOnly = true;
  return payload;
}

/** Streams connection history to a TXT/TSV file via the backend (save dialog when path omitted). */
export async function exportConnections(options: {
  format: ExportFormat;
  filter?: ConnectionsFilter;
  selectedIds?: number[];
  path?: string;
}): Promise<ExportResult> {
  return invoke<ExportResult>("export_connection_history", {
    request: {
      format: options.format,
      filter: toFilterPayload(options.filter ?? {}),
      selectedIds: options.selectedIds,
      path: options.path,
    },
  });
}
