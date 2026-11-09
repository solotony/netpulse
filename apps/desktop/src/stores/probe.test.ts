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
  fetchProbeDiff: vi.fn(async () => ({
    prepareIpCount: 3,
    measureIpCount: 4,
    rows: [
      {
        remoteIp: "192.0.2.50",
        domains: "new.example.com",
        ports: "443",
        processNames: "Safari",
        firstSeenInMeasure: "2024-06-01T11:01:00Z",
      },
    ],
  })),
}));

vi.mock("./monitoring", () => ({
  useMonitoringStore: () => ({
    status: { state: "running" },
    refreshStatus: vi.fn(async () => undefined),
    start: vi.fn(async () => undefined),
    resume: vi.fn(async () => undefined),
  }),
}));

import { fetchProbeApps, fetchProbeDiff } from "../services/probe";
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

    await store.stopMeasure();
    expect(store.phase).toBe("done");
    expect(store.rows).toHaveLength(1);

    store.reset();
    expect(store.phase).toBe("idle");
    expect(store.apps).toHaveLength(0);
    expect(store.rows).toHaveLength(0);
  });
});
