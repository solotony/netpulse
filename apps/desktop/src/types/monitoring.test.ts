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
      dnsResolution: true,
      reverseDns: true,
      ipEnrichment: true,
      asnInfo: true,
      countryInfo: false,
      cacheTtlSeconds: 86400,
      historyRetentionEnabled: false,
      historyRetentionDays: 30,
      language: "en",
      theme: "system",
      dateTimeFormat: "iso",
    };
    expect(settings.backgroundMode).toBe(true);
    expect(settings.theme).toBe("system");
  });
});
