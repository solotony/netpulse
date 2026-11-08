import { describe, expect, it } from "vitest";
import type { AppSettings, MonitoringStatus } from "./monitoring";

describe("monitoring types", () => {
  it("accepts a failed status shape from the backend", () => {
    const status: MonitoringStatus = {
      state: "failed",
      lastError: "collector failed: boom",
      backend: "native",
    };
    expect(status.state).toBe("failed");
    expect(status.lastError).toContain("boom");
  });

  it("accepts settings toggles", () => {
    const settings: AppSettings = {
      autostart: false,
      startMonitoringAutomatically: true,
      backgroundMode: true,
    };
    expect(settings.backgroundMode).toBe(true);
  });
});
