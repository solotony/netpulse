/** Backend connection row returned by `list_mock_connections`. */
export interface ConnectionRow {
  id: number;
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

/** Page request used by the Connections table. */
export interface ConnectionsPageQuery {
  page: number;
  pageSize: number;
}

/** Paginated response from the mock connections command. */
export interface ConnectionsPage {
  rows: ConnectionRow[];
  page: number;
  pageSize: number;
  total: number;
}

/** Display-oriented row for the Connections table. */
export interface ConnectionTableRow {
  id: number;
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
}

function formatEndpoint(ip: string, port: number): string {
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

/** Maps a backend connection row into table display fields. */
export function mapConnectionRow(
  row: ConnectionRow,
  nowMs = Date.now(),
): ConnectionTableRow {
  return {
    id: row.id,
    time: row.startedAt,
    process: row.processName?.trim() || "—",
    pid: row.pid,
    protocol: row.protocol.toUpperCase(),
    direction: row.direction,
    localAddress: formatEndpoint(row.localIp, row.localPort),
    remoteAddress: formatEndpoint(row.remoteIp, row.remotePort),
    domain: row.domain?.trim() || "—",
    network: row.network?.trim() || "—",
    organization: row.organization?.trim() || "—",
    duration: formatDuration(row.startedAt, row.endedAt, nowMs),
    status: row.status,
  };
}

/** Builds a page query with sane defaults for the Connections table. */
export function normalizeConnectionsQuery(
  query: Partial<ConnectionsPageQuery> = {},
): ConnectionsPageQuery {
  const page = Number.isFinite(query.page) ? Math.max(1, Math.floor(query.page as number)) : 1;
  const pageSize = Number.isFinite(query.pageSize)
    ? Math.min(500, Math.max(1, Math.floor(query.pageSize as number)))
    : 50;
  return { page, pageSize };
}
