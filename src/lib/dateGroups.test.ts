import { describe, expect, it } from "vitest";
import { dateGroupFinder, firstDayOfWeek, groupEntriesByDate } from "./dateGroups";
import type { FileEntry } from "./types";

const MONDAY = 1;
const SUNDAY = 0;

/** Seconds since the epoch at a local time, as listings give modification times. */
function at(year: number, month: number, day: number, hour = 12, minute = 0): number {
  return new Date(year, month - 1, day, hour, minute).getTime() / 1000;
}

function entry(name: string, modified: number | null, kind: FileEntry["kind"] = "file"): FileEntry {
  return {
    name,
    path: `/${name}`,
    kind,
    size: 0,
    modified,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

describe("dateGroupFinder", () => {
  // A Wednesday.
  const today = new Date(2026, 9, 28, 15, 30);

  it("follows the calendar rather than hours ago", () => {
    const groupOf = dateGroupFinder(today, MONDAY);
    expect(groupOf(at(2026, 10, 28, 0, 0))).toBe("today");
    expect(groupOf(at(2026, 10, 28, 23, 59))).toBe("today");
    expect(groupOf(at(2026, 10, 27, 23, 59))).toBe("yesterday");
    expect(groupOf(at(2026, 10, 27, 0, 0))).toBe("yesterday");
    expect(groupOf(at(2026, 10, 29, 0, 0))).toBe("future");
  });

  it("puts each earlier date in the group Explorer would", () => {
    const groupOf = dateGroupFinder(today, MONDAY);
    expect(groupOf(at(2026, 10, 26))).toBe("earlierThisWeek");
    expect(groupOf(at(2026, 10, 25))).toBe("lastWeek");
    expect(groupOf(at(2026, 10, 19, 0, 0))).toBe("lastWeek");
    expect(groupOf(at(2026, 10, 18))).toBe("earlierThisMonth");
    expect(groupOf(at(2026, 10, 1, 0, 0))).toBe("earlierThisMonth");
    expect(groupOf(at(2026, 9, 30))).toBe("lastMonth");
    expect(groupOf(at(2026, 9, 1, 0, 0))).toBe("lastMonth");
    expect(groupOf(at(2026, 8, 31))).toBe("earlierThisYear");
    expect(groupOf(at(2026, 1, 1, 0, 0))).toBe("earlierThisYear");
    expect(groupOf(at(2025, 12, 31, 23, 59))).toBe("longAgo");
    expect(groupOf(null)).toBe("unknown");
  });

  it("starts the week on the locale's first day", () => {
    expect(dateGroupFinder(today, SUNDAY)(at(2026, 10, 25))).toBe("earlierThisWeek");
    expect(dateGroupFinder(today, MONDAY)(at(2026, 10, 25))).toBe("lastWeek");
  });

  it("keeps Yesterday and Last week ahead of an earlier month", () => {
    // A Monday, the first of its week, early in the month.
    const groupOf = dateGroupFinder(new Date(2026, 10, 2, 9, 0), MONDAY);
    expect(groupOf(at(2026, 11, 1))).toBe("yesterday");
    expect(groupOf(at(2026, 10, 30))).toBe("lastWeek");
    expect(groupOf(at(2026, 10, 26, 0, 0))).toBe("lastWeek");
    expect(groupOf(at(2026, 10, 25))).toBe("lastMonth");
  });

  it("reaches back into last year for Last month in January", () => {
    // A Friday.
    const groupOf = dateGroupFinder(new Date(2026, 0, 2, 9, 0), MONDAY);
    expect(groupOf(at(2025, 12, 30))).toBe("earlierThisWeek");
    expect(groupOf(at(2025, 12, 24))).toBe("lastWeek");
    expect(groupOf(at(2025, 12, 3))).toBe("lastMonth");
    expect(groupOf(at(2025, 11, 30))).toBe("longAgo");
  });
});

describe("groupEntriesByDate", () => {
  const today = new Date(2026, 9, 28, 15, 30);
  const entries = [
    entry("docs", at(2026, 10, 20), "dir"),
    entry("a.txt", at(2026, 10, 28)),
    entry("b.txt", at(2024, 3, 1)),
    entry("c.txt", null),
    entry("d.txt", at(2026, 10, 28, 8)),
    entry("e.txt", at(2026, 10, 27)),
  ];
  const summary = (groups: ReturnType<typeof groupEntriesByDate>) =>
    groups.map((group) => [group.id, group.entries.map((item) => item.name)]);

  it("keeps the order within each group and leaves out empty groups", () => {
    const groups = groupEntriesByDate(entries, { today, weekStartsOn: MONDAY, newestFirst: true });
    expect(summary(groups)).toEqual([
      ["today", ["a.txt", "d.txt"]],
      ["yesterday", ["e.txt"]],
      ["lastWeek", ["docs"]],
      ["longAgo", ["b.txt"]],
      ["unknown", ["c.txt"]],
    ]);
  });

  it("runs oldest first when asked, with undated entries still last", () => {
    const groups = groupEntriesByDate(entries, { today, weekStartsOn: MONDAY, newestFirst: false });
    expect(groups.map((group) => group.id)).toEqual([
      "longAgo",
      "lastWeek",
      "yesterday",
      "today",
      "unknown",
    ]);
  });
});

describe("firstDayOfWeek", () => {
  it("reads the locale's week", () => {
    expect(firstDayOfWeek("de-DE")).toBe(MONDAY);
    expect(firstDayOfWeek("en-US")).toBe(SUNDAY);
  });

  it("falls back to Monday for a locale it cannot read", () => {
    expect(firstDayOfWeek("not a locale")).toBe(MONDAY);
  });
});
