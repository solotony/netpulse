import { invoke } from "@tauri-apps/api/core";
import {
  normalizeConnectionsQuery,
  type ConnectionsPage,
  type ConnectionsPageQuery,
} from "../types/connections";

/** Invokes the Tauri mock-connections command with a normalized page query. */
export async function fetchMockConnections(
  query: Partial<ConnectionsPageQuery> = {},
): Promise<ConnectionsPage> {
  const { page, pageSize } = normalizeConnectionsQuery(query);
  return invoke<ConnectionsPage>("list_mock_connections", {
    page,
    pageSize,
  });
}
