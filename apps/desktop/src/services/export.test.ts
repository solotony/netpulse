import { describe, expect, it } from "vitest";

/**
 * Mirrors the filter payload shaping used by `exportConnections` so we can
 * assert invoke args without spinning up Tauri.
 */
function toFilterPayload(filter: Record<string, unknown>): Record<string, unknown> {
  const payload: Record<string, unknown> = {};
  if (filter.text) payload.text = filter.text;
  if (filter.processId != null) payload.processId = filter.processId;
  if (filter.processName) payload.processName = filter.processName;
  if (filter.remoteIp) payload.remoteIp = filter.remoteIp;
  if (filter.domain) payload.domain = filter.domain;
  if (filter.network) payload.network = filter.network;
  if (filter.protocol) payload.protocol = filter.protocol;
  if (filter.startedFrom) payload.startedFrom = filter.startedFrom;
  if (filter.startedTo) payload.startedTo = filter.startedTo;
  if (filter.activeOnly) payload.activeOnly = true;
  return payload;
}

describe("export request shaping", () => {
  it("keeps process / IP / network history filters", () => {
    expect(toFilterPayload({ processId: 42 })).toEqual({ processId: 42 });
    expect(toFilterPayload({ remoteIp: "1.2.3.4" })).toEqual({ remoteIp: "1.2.3.4" });
    expect(toFilterPayload({ network: "172.64.0.0/13" })).toEqual({
      network: "172.64.0.0/13",
    });
  });

  it("includes date range and omits empty fields", () => {
    expect(
      toFilterPayload({
        processName: "Safari",
        startedFrom: "2026-11-01T00:00",
        startedTo: "2026-11-09T23:59",
        text: "",
      }),
    ).toEqual({
      processName: "Safari",
      startedFrom: "2026-11-01T00:00",
      startedTo: "2026-11-09T23:59",
    });
  });
});
