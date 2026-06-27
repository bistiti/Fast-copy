import { describe, expect, it } from "vitest";
import { formatBytes, formatSpeed, formatDuration } from "./format";

describe("formatBytes", () => {
  it("formats across unit boundaries", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KiB");
    expect(formatBytes(1536)).toBe("1.5 KiB");
    expect(formatBytes(1048576)).toBe("1.00 MiB");
    expect(formatBytes(1073741824)).toBe("1.00 GiB");
  });
});

describe("formatSpeed", () => {
  it("formats throughput", () => {
    expect(formatSpeed(500)).toBe("500 B/s");
    expect(formatSpeed(1024 * 1024)).toBe("1.0 MiB/s");
  });
});

describe("formatDuration", () => {
  it("formats durations and handles unknown", () => {
    expect(formatDuration(0)).toBe("0s");
    expect(formatDuration(65)).toBe("1m 5s");
    expect(formatDuration(3661)).toBe("1h 1m 1s");
    expect(formatDuration(-1)).toBe("--");
  });
});
