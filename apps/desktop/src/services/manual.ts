import { invoke } from "@tauri-apps/api/core";

export type ManualEntryKind = "domain" | "ip" | "cidr";

export interface AddManualEntryRequest {
  kind: ManualEntryKind;
  value: string;
  groupId?: number | null;
  groupName?: string | null;
}

export interface ManualEntryResult {
  kind: string;
  value: string;
  addresses: string[];
  cidrs: string[];
  groupId: number | null;
  groupName: string | null;
  notes: string[];
}

export interface GroupRow {
  id: number;
  name: string;
  createdAt: string;
  networkCount: number;
}

export interface GroupNetworkRow {
  id: number;
  cidr: string;
  netName: string | null;
  organization: string | null;
}

export async function addManualEntry(
  request: AddManualEntryRequest,
): Promise<ManualEntryResult> {
  return invoke<ManualEntryResult>("add_manual_entry", { request });
}

export async function fetchGroups(): Promise<GroupRow[]> {
  return invoke<GroupRow[]>("list_groups");
}

export async function fetchGroupNetworks(groupId: number): Promise<GroupNetworkRow[]> {
  return invoke<GroupNetworkRow[]>("list_group_networks", { groupId });
}

export async function createGroup(name: string): Promise<GroupRow> {
  return invoke<GroupRow>("create_group_cmd", { request: { name } });
}

export async function renameGroup(id: number, name: string): Promise<void> {
  return invoke("rename_group_cmd", { request: { id, name } });
}

export async function deleteGroup(id: number): Promise<void> {
  return invoke("delete_group_cmd", { id });
}

export async function setNetworkGroup(
  networkId: number,
  groupId: number | null,
): Promise<void> {
  return invoke("set_network_group_cmd", {
    request: { networkId, groupId },
  });
}
