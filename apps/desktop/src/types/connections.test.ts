import { describe, expect, it } from "vitest";
import {
  mapConnectionRow,
  normalizeConnectionsQuery,
  normalizeEntityListQuery,
  normalizeStatsQuery,
  toEntityInvokeArgs,
  toHistoryInvokeArgs,
  toStatsInvokeArgs,
  type ConnectionRow,
} from "./connections";

const sampleRow: ConnectionRow = {
  id: 7,
  processId: 3,
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
    expect(normalizeConnectionsQuery()).toEqual({
      page: 1,
      pageSize: 50,
      sortBy: "startedAt",
      sortDir: "desc",
      filter: {
        text: undefined,
        processId: undefined,
        processName: undefined,
        remoteIp: undefined,
        domain: undefined,
        network: undefined,
        protocol: undefined,
        startedFrom: undefined,
        startedTo: undefined,
        activeOnly: undefined,
      },
    });
  });

  it("clamps invalid page and pageSize values", () => {
    expect(normalizeConnectionsQuery({ page: 0, pageSize: 0 })).toMatchObject({
      page: 1,
      pageSize: 1,
    });
    expect(normalizeConnectionsQuery({ page: 2.9, pageSize: 999 })).toMatchObject({
      page: 2,
      pageSize: 500,
    });
  });

  it("keeps filters, sort, and active-only when provided", () => {
    const query = normalizeConnectionsQuery({
      page: 3,
      pageSize: 25,
      sortBy: "process",
      sortDir: "asc",
      filter: {
        text: " safari ",
        processName: "Safari",
        remoteIp: "1.1.1.1",
        domain: "example.com",
        network: "203.0.113.0/24",
        protocol: "tcp",
        startedFrom: "2026-11-01T00:00",
        startedTo: "2026-11-09T23:59",
        activeOnly: true,
        processId: 42,
      },
    });

    expect(query).toEqual({
      page: 3,
      pageSize: 25,
      sortBy: "process",
      sortDir: "asc",
      filter: {
        text: "safari",
        processId: 42,
        processName: "Safari",
        remoteIp: "1.1.1.1",
        domain: "example.com",
        network: "203.0.113.0/24",
        protocol: "tcp",
        startedFrom: "2026-11-01T00:00",
        startedTo: "2026-11-09T23:59",
        activeOnly: true,
      },
    });
  });

  it("drops blank filter strings and invalid sort keys", () => {
    const query = normalizeConnectionsQuery({
      sortBy: "not-a-column" as never,
      filter: {
        text: "   ",
        protocol: "",
        activeOnly: false,
      },
    });
    expect(query.sortBy).toBe("startedAt");
    expect(query.filter.text).toBeUndefined();
    expect(query.filter.protocol).toBeUndefined();
    expect(query.filter.activeOnly).toBeUndefined();
  });
});

describe("toHistoryInvokeArgs", () => {
  it("omits empty filter fields from the invoke payload", () => {
    const args = toHistoryInvokeArgs(
      normalizeConnectionsQuery({
        page: 2,
        pageSize: 10,
        sortBy: "remote",
        sortDir: "asc",
        filter: {
          text: "curl",
          activeOnly: true,
          processName: "",
        },
      }),
    );

    expect(args).toEqual({
      query: {
        filter: {
          text: "curl",
          activeOnly: true,
        },
        sortBy: "remote",
        sortDir: "asc",
        page: 2,
        pageSize: 10,
      },
    });
  });
});

describe("normalizeEntityListQuery", () => {
  it("defaults and clamps entity list pagination", () => {
    expect(normalizeEntityListQuery()).toEqual({
      page: 1,
      pageSize: 50,
      text: undefined,
    });
    expect(normalizeEntityListQuery({ page: -1, pageSize: 900, text: " dns " })).toEqual({
      page: 1,
      pageSize: 500,
      text: "dns",
    });
  });

  it("builds entity invoke args without blank text", () => {
    expect(toEntityInvokeArgs(normalizeEntityListQuery({ page: 2, text: "a" }))).toEqual({
      query: { page: 2, pageSize: 50, text: "a" },
    });
    expect(toEntityInvokeArgs(normalizeEntityListQuery({ page: 1 }))).toEqual({
      query: { page: 1, pageSize: 50 },
    });
  });

  it("passes networks group filter through invoke args", () => {
    expect(
      toEntityInvokeArgs(normalizeEntityListQuery({ groupFilter: "ungrouped" })),
    ).toEqual({
      query: { page: 1, pageSize: 50, groupFilter: "ungrouped" },
    });
    expect(toEntityInvokeArgs(normalizeEntityListQuery({ groupFilter: 7 }))).toEqual({
      query: { page: 1, pageSize: 50, groupFilter: "7" },
    });
    expect(toEntityInvokeArgs(normalizeEntityListQuery({ groupFilter: "all" }))).toEqual({
      query: { page: 1, pageSize: 50 },
    });
  });
});

describe("normalizeStatsQuery", () => {
  it("defaults grouping and fills a 30-day date range", () => {
    const query = normalizeStatsQuery();
    expect(query.groupBy).toBe("process");
    expect(query.from).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(query.to).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  it("keeps valid grouping and date bounds", () => {
    expect(
      normalizeStatsQuery({
        groupBy: "week",
        from: "2026-01-01",
        to: "2026-01-31",
      }),
    ).toEqual({
      groupBy: "week",
      from: "2026-01-01",
      to: "2026-01-31",
    });
  });

  it("falls back when grouping is unknown", () => {
    expect(normalizeStatsQuery({ groupBy: "charts" as never }).groupBy).toBe("process");
  });

  it("builds stats invoke args", () => {
    expect(
      toStatsInvokeArgs({
        groupBy: "domain",
        from: "2026-01-01",
        to: "2026-01-31",
      }),
    ).toEqual({
      query: {
        groupBy: "domain",
        from: "2026-01-01",
        to: "2026-01-31",
      },
    });
  });
});

describe("mapConnectionRow", () => {
  it("maps backend fields into table display columns", () => {
    const mapped = mapConnectionRow(sampleRow, Date.parse("2026-11-09T00:02:00.000Z"));
    expect(mapped).toMatchObject({
      id: 7,
      processId: 3,
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
    expect(mapped.cells.local).toBe("192.168.1.10:54321");
  });

  it("omits port 0 on ICMP endpoints", () => {
    const mapped = mapConnectionRow({
      ...sampleRow,
      protocol: "icmp",
      localPort: 0,
      remotePort: 0,
      remoteIp: "172.64.155.209",
    });
    expect(mapped.protocol).toBe("ICMP");
    expect(mapped.localAddress).toBe("192.168.1.10");
    expect(mapped.remoteAddress).toBe("172.64.155.209");
  });

  it("uses a placeholder process name when missing", () => {
    const mapped = mapConnectionRow({ ...sampleRow, processName: "  " });
    expect(mapped.process).toBe("—");
  });
});
