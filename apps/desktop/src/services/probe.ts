import { invoke } from "@tauri-apps/api/core";

export interface ProbeApp {
  appKey: string;
  displayName: string | null;
  processCount: number;
  lastSeenAt: string;
}

export interface ProbeDiffRow {
  remoteIp: string;
  domains: string | null;
  ports: string | null;
  processNames: string | null;
  firstSeenInMeasure: string;
}

export interface ProbeDiffResult {
  prepareIpCount: number;
  measureIpCount: number;
  rows: ProbeDiffRow[];
}

export interface ProbeDiffRequest {
  appKeys: string[];
  prepareStart: string;
  prepareEnd: string;
  measureStart?: string | null;
  measureEnd?: string | null;
}

export async function fetchProbeApps(
  windowStart: string,
  windowEnd: string,
): Promise<ProbeApp[]> {
  return invoke<ProbeApp[]>("list_probe_applications", {
    request: { windowStart, windowEnd },
  });
}

export async function fetchProbeDiff(
  request: ProbeDiffRequest,
): Promise<ProbeDiffResult> {
  return invoke<ProbeDiffResult>("diff_probe", { request });
}
