import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";

vi.mock("../services/probe", () => ({
  fetchProbeApps: vi.fn(async () => [
    {
      appKey: "/Applications/Safari.app",
      displayName: "Safari",
      processCount: 2,
      lastSeenAt: "2024-06-01T10:00:00Z",
    },
  ]),
  fetchProbeDiff: vi.fn(async (request: { measureStart?: string | null }) => {
    const measuring = Boolean(request.measureStart);
    const baseline = {
      remoteIp: "203.0.113.10",
      domains: "old.example.com",
      ports: "443",
      processNames: "Safari",
      firstSeen: "2024-06-01T10:00:00Z",
      origin: "baseline" as const,
    };
    const fresh = {
      remoteIp: "192.0.2.50",
      domains: "new.example.com",
      ports: "443",
      processNames: "Safari",
      firstSeen: "2024-06-01T11:01:00Z",
      origin: "new" as const,
    };
    return {
      prepareIpCount: 1,
      measureIpCount: measuring ? 2 : 0,
      rows: measuring ? [baseline, fresh] : [baseline],
    };
  }),
}));

vi.mock("./monitoring", () => ({
  useMonitoringStore: () => ({
    status: { state: "running" },
    refreshStatus: vi.fn(async () => undefined),
    start: vi.fn(async () => undefined),
    resume: vi.fn(async () => undefined),
  }),
}));

import { fetchProbeApps, fetchProbeDiff, type ProbeDiffResult } from "../services/probe";
import { useProbeStore } from "./probe";

describe("probe store phases", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("starts prepare without selecting apps, then filters when selected", async () => {
    const store = useProbeStore();

    expect(store.canPrepare).toBe(true);
    expect(store.apps).toHaveLength(0);

    await store.startPrepare();
    expect(store.phase).toBe("preparing");
    expect(store.prepareIpCount).toBe(1);
    expect(store.measureIpCount).toBe(0);
    expect(store.rows.map((row) => row.origin)).toEqual(["baseline"]);
    expect(store.newIpCount).toBe(0);
    expect(fetchProbeDiff).toHaveBeenCalledWith(
      expect.objectContaining({ appKeys: [] }),
    );
    expect(fetchProbeApps).toHaveBeenCalled();
    expect(store.apps).toHaveLength(1);

    store.toggleApp("/Applications/Safari.app");
    expect(store.selectedCount).toBe(1);
    expect(fetchProbeDiff).toHaveBeenLastCalledWith(
      expect.objectContaining({
        appKeys: ["/Applications/Safari.app"],
      }),
    );

    await store.startMeasure();
    expect(store.phase).toBe("measuring");
    expect(store.canStop).toBe(true);
    expect(store.rows.map((row) => row.origin)).toEqual(["baseline", "new"]);
    expect(store.newIpCount).toBe(1);
    expect(store.prepareIpCount).toBe(1);

    await store.stopMeasure();
    expect(store.phase).toBe("done");
    expect(store.rows).toHaveLength(2);

    store.reset();
    expect(store.phase).toBe("idle");
    expect(store.apps).toHaveLength(0);
    expect(store.rows).toHaveLength(0);
    expect(store.prepareIpCount).toBe(0);
    expect(store.measureIpCount).toBe(0);
  });

  it("clears counts when prepare starts and drops a refresh that finishes after reset", async () => {
    const store = useProbeStore();
    await store.startPrepare();
    expect(store.prepareIpCount).toBe(1);

    let release: (value: ProbeDiffResult | PromiseLike<ProbeDiffResult>) => void =
      () => {};
    vi.mocked(fetchProbeDiff).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    const pending = store.refreshDiff();
    await vi.waitFor(() => expect(fetchProbeDiff).toHaveBeenCalledTimes(2));

    store.reset();
    expect(store.prepareIpCount).toBe(0);
    expect(store.measureIpCount).toBe(0);
    expect(store.rows).toHaveLength(0);

    release({
      prepareIpCount: 9,
      measureIpCount: 8,
      rows: [
        {
          remoteIp: "198.51.100.9",
          domains: null,
          ports: null,
          processNames: null,
          firstSeen: "2024-06-01T10:05:00Z",
          origin: "baseline",
        },
      ],
    });
    await pending;

    expect(store.phase).toBe("idle");
    expect(store.prepareIpCount).toBe(0);
    expect(store.measureIpCount).toBe(0);
    expect(store.rows).toHaveLength(0);
    expect(store.apps).toHaveLength(0);
  });
});
