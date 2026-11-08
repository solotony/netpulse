import { describe, expect, it } from "vitest";
import {
  mapConnectionRow,
  normalizeConnectionsQuery,
  type ConnectionRow,
} from "./connections";

const sampleRow: ConnectionRow = {
  id: 7,
  startedAt: "2026-11-09T00:01:00.000Z",
  processName: "curl",
  pid: 4242,
  protocol: "tcp",
  direction: "outbound",
  localIp: "192.168.1.10",
  localPort: 54321,
  remoteIp: "93.184.216.34",
  remotePort: 443,
  domain: null,
  network: null,
  organization: null,
  endedAt: "2026-11-09T00:01:30.000Z",
  status: "closed",
};

describe("normalizeConnectionsQuery", () => {
  it("applies defaults for an empty query", () => {
    expect(normalizeConnectionsQuery()).toEqual({ page: 1, pageSize: 50 });
  });

  it("clamps invalid page and pageSize values", () => {
    expect(normalizeConnectionsQuery({ page: 0, pageSize: 0 })).toEqual({
      page: 1,
      pageSize: 1,
    });
    expect(normalizeConnectionsQuery({ page: 2.9, pageSize: 999 })).toEqual({
      page: 2,
      pageSize: 500,
    });
  });
});

describe("mapConnectionRow", () => {
  it("maps backend fields into table display columns", () => {
    const mapped = mapConnectionRow(sampleRow, Date.parse("2026-11-09T00:02:00.000Z"));
    expect(mapped).toEqual({
      id: 7,
      time: "2026-11-09T00:01:00.000Z",
      process: "curl",
      pid: 4242,
      protocol: "TCP",
      direction: "outbound",
      localAddress: "192.168.1.10:54321",
      remoteAddress: "93.184.216.34:443",
      domain: "—",
      network: "—",
      organization: "—",
      duration: "30s",
      status: "closed",
    });
  });

  it("uses a placeholder process name when missing", () => {
    const mapped = mapConnectionRow({ ...sampleRow, processName: "  " });
    expect(mapped.process).toBe("—");
  });
});
