import { invoke } from "@tauri-apps/api/core";
import {
  normalizeConnectionsQuery,
  normalizeEntityListQuery,
  normalizeStatsQuery,
  toEntityInvokeArgs,
  toHistoryInvokeArgs,
  toStatsInvokeArgs,
  type ConnectionsPage,
  type ConnectionsPageQuery,
  type EntityListQuery,
  type StatsQuery,
  type StatsTable,
} from "../types/connections";

/** Invokes the real connection-history query (filters/sort/pagination). */
export async function fetchConnections(
  query: Partial<ConnectionsPageQuery> & { filter?: ConnectionsPageQuery["filter"] } = {},
): Promise<ConnectionsPage> {
  const normalized = normalizeConnectionsQuery(query);
  return invoke<ConnectionsPage>(
    "query_connection_history",
    toHistoryInvokeArgs(normalized),
  );
}

/**
 * Seeds scripted mock rows when the DB is empty (dev convenience), then returns
 * a real history page.
 */
export async function fetchConnectionsWithOptionalSeed(
  query: Partial<ConnectionsPageQuery> & { filter?: ConnectionsPageQuery["filter"] } = {},
): Promise<ConnectionsPage> {
  const page = await fetchConnections(query);
  if (page.total > 0) {
    return page;
  }
  // Optional seed path: only runs when history is empty.
  await invoke("list_mock_connections", { page: 1, pageSize: 1 });
  return fetchConnections(query);
}

export interface ProcessRow {
  id: number;
  processName: string | null;
  pid: number;
  executablePath: string | null;
  firstSeenAt: string;
  lastSeenAt: string;
  totalConnections: number;
  uniqueRemoteIps: number;
  uniqueDomains: number;
  bytesSent: number;
  bytesReceived: number;
}

export interface ProcessPage {
  rows: ProcessRow[];
  page: number;
  pageSize: number;
  total: number;
}

export async function fetchProcesses(
  query: Partial<EntityListQuery> = {},
): Promise<ProcessPage> {
  return invoke<ProcessPage>(
    "list_processes",
    toEntityInvokeArgs(normalizeEntityListQuery(query)),
  );
}

export interface IpRow {
  ip: string;
  firstSeenAt: string;
  lastSeenAt: string;
  connectionCount: number;
  processCount: number;
  domainCount: number;
  cidr: string | null;
  netName: string | null;
  asn: number | null;
  organization: string | null;
  country: string | null;
}

export interface IpPage {
  rows: IpRow[];
  page: number;
  pageSize: number;
  total: number;
}

export async function fetchIpAddresses(
  query: Partial<EntityListQuery> = {},
): Promise<IpPage> {
  return invoke<IpPage>(
    "list_ip_addresses",
    toEntityInvokeArgs(normalizeEntityListQuery(query)),
  );
}

export interface DomainRow {
  id: number;
  domain: string;
  firstSeenAt: string;
  lastSeenAt: string;
  ipCount: number;
  processCount: number;
  connectionCount: number;
  sources: string | null;
}

export interface DomainPage {
  rows: DomainRow[];
  page: number;
  pageSize: number;
  total: number;
}

export async function fetchDomains(
  query: Partial<EntityListQuery> = {},
): Promise<DomainPage> {
  return invoke<DomainPage>(
    "list_domains",
    toEntityInvokeArgs(normalizeEntityListQuery(query)),
  );
}

export interface NetworkRow {
  id: number;
  cidr: string;
  netRange: string | null;
  netName: string | null;
  asn: number | null;
  organization: string | null;
  country: string | null;
  ipCount: number;
  connectionCount: number;
  firstSeenAt: string | null;
  lastSeenAt: string | null;
}

export interface NetworkPage {
  rows: NetworkRow[];
  page: number;
  pageSize: number;
  total: number;
}

export async function fetchNetworks(
  query: Partial<EntityListQuery> = {},
): Promise<NetworkPage> {
  return invoke<NetworkPage>(
    "list_networks",
    toEntityInvokeArgs(normalizeEntityListQuery(query)),
  );
}

export async function fetchStatistics(
  query: Partial<StatsQuery> = {},
): Promise<StatsTable> {
  return invoke<StatsTable>(
    "query_statistics",
    toStatsInvokeArgs(normalizeStatsQuery(query)),
  );
}
