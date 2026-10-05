import { describe, expect, it } from "vitest";
import {
  formatDuration,
  formatPermissions,
  formatSize,
  formatSpeed,
  formatVersion,
  pluralize,
} from "./format";

describe("formatSize", () => {
  it.each([
    [0, "0 B"],
    [512, "512 B"],
    [1023, "1023 B"],
    [1024, "1.0 KB"],
    [1536, "1.5 KB"],
    [5 * 1024 ** 3, "5.0 GB"],
    [1024 ** 6, "1024.0 PB"],
  ])("formats %d bytes as %s", (bytes, expected) => {
    expect(formatSize(bytes)).toBe(expected);
  });

  it("returns nothing for impossible sizes", () => {
    expect(formatSize(-1)).toBe("");
    expect(formatSize(Number.NaN)).toBe("");
  });
});

describe("formatPermissions", () => {
  it.each([
    ["dir", 0o755, "drwxr-xr-x"],
    ["file", 0o644, "-rw-r--r--"],
    ["symlink", 0o777, "lrwxrwxrwx"],
    ["file", 0o4755, "-rwsr-xr-x"],
    ["file", 0o2644, "-rw-r-Sr--"],
    ["dir", 0o1777, "drwxrwxrwt"],
    ["dir", 0o1776, "drwxrwxrwT"],
  ] as const)("renders %s mode %o as %s", (kind, permissions, expected) => {
    expect(formatPermissions({ kind, permissions })).toBe(expected);
  });

  it("is blank when the mode is unknown", () => {
    expect(formatPermissions({ kind: "file", permissions: null })).toBe("");
  });
});

describe("formatVersion", () => {
  it("drops a zero patch component", () => {
    expect(formatVersion("0.1.0")).toBe("v0.1");
    expect(formatVersion("1.2.3")).toBe("v1.2.3");
    expect(formatVersion("0.2.0-beta.1")).toBe("v0.2");
  });
});

describe("pluralize", () => {
  it("chooses the form by count", () => {
    expect(pluralize(1, "item")).toBe("1 item");
    expect(pluralize(0, "item")).toBe("0 items");
    expect(pluralize(3, "entry", "entries")).toBe("3 entries");
  });
});

describe("formatSpeed and formatDuration", () => {
  it("formats rates and times left", () => {
    expect(formatSpeed(1536)).toBe("1.5 KB/s");
    expect(formatSpeed(-5)).toBe("0 B/s");
    expect(formatDuration(45)).toBe("45s");
    expect(formatDuration(125)).toBe("2m 5s");
    expect(formatDuration(3720)).toBe("1h 2m");
  });
});
