import { describe, expect, it } from "vitest";
import { formatBytes, formatDateTime } from "./format";

describe("formatBytes", () => {
  it("formats null as a dash", () => {
    expect(formatBytes(null)).toBe("—");
  });

  it("formats small and large sizes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
  });
});

describe("formatDateTime", () => {
  it("returns a dash for empty values", () => {
    expect(formatDateTime(null)).toBe("—");
  });

  it("keeps the original string for iso preference", () => {
    expect(formatDateTime("2026-11-09T12:30:00.000Z", "iso")).toBe(
      "2026-11-09T12:30:00.000Z",
    );
  });
});
