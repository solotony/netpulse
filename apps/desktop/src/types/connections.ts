import type { DateTimeFormatPreference } from "./monitoring";
import { formatDateTime } from "../utils/format";

/** Backend connection row returned by `query_connection_history`. */
export interface ConnectionRow {
  id: number;
  processId: number;
  startedAt: string;
  processName: string | null;
  pid: number;
  protocol: string;
  direction: string;
  localIp: string;
  localPort: number;
  remoteIp: string;
  remotePort: number;
  domain: string | null;
  network: string | null;
  organization: string | null;
  endedAt: string | null;
  status: string;
}

/** Sortable connection history columns (matches Tauri parser keys). */
export type ConnectionSortBy =
  | "startedAt"
  | "endedAt"
  | "process"
  | "pid"
  | "protocol"
  | "direction"
  | "local"
  | "remote"
  | "domain"
  | "network"
  | "organization"
  | "id";

export type SortDir = "asc" | "desc";

/** Filter fields for connection history. */
export interface ConnectionsFilter {
  text?: string;
  processId?: number;
  processName?: string;
  remoteIp?: string;
  domain?: string;
  network?: string;
  protocol?: string;
  startedFrom?: string;
  startedTo?: string;
  activeOnly?: boolean;
}

/** Page request used by the Connections table. */
export interface ConnectionsPageQuery {
  page: number;
  pageSize: number;
  sortBy: ConnectionSortBy;
  sortDir: SortDir;
  filter: ConnectionsFilter;
}

/** Paginated response from the connection-history command. */
export interface ConnectionsPage {
  rows: ConnectionRow[];
  page: number;
  pageSize: number;
  total: number;
}

/** Display-oriented row for the Connections table. */
export interface ConnectionTableRow {
  id: number;
  processId: number;
  time: string;
  process: string;
  pid: number;
  protocol: string;
  direction: string;
  localAddress: string;
  remoteAddress: string;
  domain: string;
  network: string;
  organization: string;
  duration: string;
  status: string;
  /** Raw cell values keyed by column id for copy/export. */
  cells: Record<string, string>;
}

export const CONNECTION_COLUMNS = [
  { id: "time", label: "Time", sortBy: "startedAt" as ConnectionSortBy },
  { id: "process", label: "Process", sortBy: "process" as ConnectionSortBy },
  { id: "pid", label: "PID", sortBy: "pid" as ConnectionSortBy },
  { id: "protocol", label: "Protocol", sortBy: "protocol" as ConnectionSortBy },
  { id: "direction", label: "Direction", sortBy: "direction" as ConnectionSortBy },
  { id: "local", label: "Local", sortBy: "local" as ConnectionSortBy },
  { id: "remote", label: "Remote", sortBy: "remote" as ConnectionSortBy },
  { id: "domain", label: "Domain", sortBy: "domain" as ConnectionSortBy },
  { id: "network", label: "Network", sortBy: "network" as ConnectionSortBy },
  { id: "organization", label: "Organization", sortBy: "organization" as ConnectionSortBy },
  { id: "duration", label: "Duration", sortBy: null },
  { id: "status", label: "Status", sortBy: null },
] as const;

function formatEndpoint(ip: string, port: number, protocol: string): string {
  if (protocol.toLowerCase() === "icmp" && port === 0) {
    return ip;
  }
  return `${ip}:${port}`;
}

function formatDuration(startedAt: string, endedAt: string | null, nowMs = Date.now()): string {
  const start = Date.parse(startedAt);
  if (Number.isNaN(start)) {
    return "—";
  }
  const end = endedAt ? Date.parse(endedAt) : nowMs;
  if (Number.isNaN(end) || end < start) {
    return "—";
  }
  const totalSeconds = Math.floor((end - start) / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds}s`;
  }
  return `${seconds}s`;
}

function display(value: string | null | undefined): string {
  const trimmed = value?.trim();
  return trimmed ? trimmed : "—";
}

/** Maps a backend connection row into table display fields. */
export function mapConnectionRow(
  row: ConnectionRow,
  nowMs = Date.now(),
  dateTimeFormat: DateTimeFormatPreference = "iso",
): ConnectionTableRow {
  const process = display(row.processName);
  const localAddress = formatEndpoint(row.localIp, row.localPort, row.protocol);
  const remoteAddress = formatEndpoint(row.remoteIp, row.remotePort, row.protocol);
  const domain = display(row.domain);
  const network = display(row.network);
  const organization = display(row.organization);
  const duration = formatDuration(row.startedAt, row.endedAt, nowMs);
  const protocol = row.protocol.toUpperCase();
  const time = formatDateTime(row.startedAt, dateTimeFormat);

  return {
    id: row.id,
    processId: row.processId,
    time,
    process,
    pid: row.pid,
    protocol,
    direction: row.direction,
    localAddress,
    remoteAddress,
    domain,
    network,
    organization,
    duration,
    status: row.status,
    cells: {
      time,
      process: row.processName?.trim() || "",
      pid: String(row.pid),
      protocol,
      direction: row.direction,
      local: localAddress,
      remote: remoteAddress,
      domain: row.domain?.trim() || "",
      network: row.network?.trim() || "",
      organization: row.organization?.trim() || "",
      duration,
      status: row.status,
    },
  };
}

function optionalString(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed ? trimmed : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value)) {
    return Math.floor(value);
  }
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return Math.floor(parsed);
    }
  }
  return undefined;
}

const SORT_BY_VALUES: ConnectionSortBy[] = [
  "startedAt",
  "endedAt",
  "process",
  "pid",
  "protocol",
  "direction",
  "local",
  "remote",
  "domain",
  "network",
  "organization",
  "id",
];

/** Builds a page query with sane defaults for the Connections table. */
export function normalizeConnectionsQuery(
  query: Partial<ConnectionsPageQuery> & { filter?: Partial<ConnectionsFilter> } = {},
): ConnectionsPageQuery {
  const page = Number.isFinite(query.page) ? Math.max(1, Math.floor(query.page as number)) : 1;
  const pageSize = Number.isFinite(query.pageSize)
    ? Math.min(500, Math.max(1, Math.floor(query.pageSize as number)))
    : 50;
  const sortBy = SORT_BY_VALUES.includes(query.sortBy as ConnectionSortBy)
    ? (query.sortBy as ConnectionSortBy)
    : "startedAt";
  const sortDir: SortDir = query.sortDir === "asc" ? "asc" : "desc";
  const raw = query.filter ?? {};

  const filter: ConnectionsFilter = {
    text: optionalString(raw.text),
    processId: optionalNumber(raw.processId),
    processName: optionalString(raw.processName),
    remoteIp: optionalString(raw.remoteIp),
    domain: optionalString(raw.domain),
    network: optionalString(raw.network),
    protocol: optionalString(raw.protocol),
    startedFrom: optionalString(raw.startedFrom),
    startedTo: optionalString(raw.startedTo),
    activeOnly: raw.activeOnly === true ? true : undefined,
  };

  return { page, pageSize, sortBy, sortDir, filter };
}

/** Payload shape sent to the Tauri `query_connection_history` command. */
export function toHistoryInvokeArgs(query: ConnectionsPageQuery): Record<string, unknown> {
  const filter: Record<string, unknown> = {};
  if (query.filter.text) filter.text = query.filter.text;
  if (query.filter.processId != null) filter.processId = query.filter.processId;
  if (query.filter.processName) filter.processName = query.filter.processName;
  if (query.filter.remoteIp) filter.remoteIp = query.filter.remoteIp;
  if (query.filter.domain) filter.domain = query.filter.domain;
  if (query.filter.network) filter.network = query.filter.network;
  if (query.filter.protocol) filter.protocol = query.filter.protocol;
  if (query.filter.startedFrom) filter.startedFrom = query.filter.startedFrom;
  if (query.filter.startedTo) filter.startedTo = query.filter.startedTo;
  if (query.filter.activeOnly) filter.activeOnly = true;

  return {
    query: {
      filter,
      sortBy: query.sortBy,
      sortDir: query.sortDir,
      page: query.page,
      pageSize: query.pageSize,
    },
  };
}

/** Networks page group filter: all groups, ungrouped only, or a specific group id. */
export type NetworkGroupFilter = "all" | "ungrouped" | number;

/** Builds a default entity list query (processes/IPs/domains/networks). */
export interface EntityListQuery {
  page: number;
  pageSize: number;
  text?: string;
  groupFilter?: NetworkGroupFilter;
}

function normalizeGroupFilter(
  value: NetworkGroupFilter | string | undefined,
): NetworkGroupFilter | undefined {
  if (value === undefined || value === "all" || value === "") return undefined;
  if (value === "ungrouped" || value === "none") return "ungrouped";
  if (typeof value === "number" && Number.isFinite(value) && value > 0) {
    return Math.floor(value);
  }
  if (typeof value === "string") {
    const id = Number(value);
    if (Number.isFinite(id) && id > 0) return Math.floor(id);
  }
  return undefined;
}

export function normalizeEntityListQuery(
  query: Partial<EntityListQuery> = {},
): EntityListQuery {
  const page = Number.isFinite(query.page) ? Math.max(1, Math.floor(query.page as number)) : 1;
  const pageSize = Number.isFinite(query.pageSize)
    ? Math.min(500, Math.max(1, Math.floor(query.pageSize as number)))
    : 50;
  return {
    page,
    pageSize,
    text: optionalString(query.text),
    groupFilter: normalizeGroupFilter(query.groupFilter),
  };
}

export function toEntityInvokeArgs(query: EntityListQuery): Record<string, unknown> {
  const payload: Record<string, unknown> = {
    page: query.page,
    pageSize: query.pageSize,
  };
  if (query.text) {
    payload.text = query.text;
  }
  if (query.groupFilter === "ungrouped") {
    payload.groupFilter = "ungrouped";
  } else if (typeof query.groupFilter === "number") {
    payload.groupFilter = String(query.groupFilter);
  }
  return { query: payload };
}

/** Statistics grouping keys accepted by `query_statistics`. */
export type StatsGrouping =
  | "process"
  | "ip"
  | "domain"
  | "network"
  | "organization"
  | "day"
  | "week"
  | "month";

export interface StatsQuery {
  groupBy: StatsGrouping;
  from: string;
  to: string;
}

export interface StatsTable {
  groupBy: StatsGrouping;
  columns: [string, string, string, string];
  rows: Array<{
    label: string;
    connections: number;
    metricA: number;
    metricB: number;
  }>;
}

const STATS_GROUPINGS: StatsGrouping[] = [
  "process",
  "ip",
  "domain",
  "network",
  "organization",
  "day",
  "week",
  "month",
];

/** Normalizes statistics query parameters (grouping + inclusive date range). */
export function normalizeStatsQuery(query: Partial<StatsQuery> = {}): StatsQuery {
  const groupBy = STATS_GROUPINGS.includes(query.groupBy as StatsGrouping)
    ? (query.groupBy as StatsGrouping)
    : "process";

  const today = new Date();
  const defaultTo = today.toISOString().slice(0, 10);
  const monthAgo = new Date(today.getTime() - 30 * 24 * 60 * 60 * 1000)
    .toISOString()
    .slice(0, 10);

  return {
    groupBy,
    from: optionalString(query.from) ?? monthAgo,
    to: optionalString(query.to) ?? defaultTo,
  };
}

export function toStatsInvokeArgs(query: StatsQuery): Record<string, unknown> {
  return {
    query: {
      groupBy: query.groupBy,
      from: query.from,
      to: query.to,
    },
  };
}
