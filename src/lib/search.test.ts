import { describe, expect, it } from "vitest";
import {
  MAX_RECHECKED_PATHS,
  RECHECK_ALL_FOLDERS,
  RECENT_SEARCHES,
  SEARCH_LIMIT,
  applyRecheck,
  buildSearchRequest,
  dateRange,
  defaultSearchForm,
  describeDate,
  describeSearch,
  describeSize,
  folderOf,
  parseSizeText,
  recheckTargets,
  relativeFolder,
  rememberSearch,
  sharedFolder,
  sizeRange,
  type SearchForm,
} from "./search";
import type { FileEntry } from "./types";

function entry(path: string, overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name: path.split(/[/\\]/).pop() ?? path,
    path,
    kind: "file",
    size: 1,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
    ...overrides,
  };
}

const seconds = (moment: Date) => Math.floor(moment.getTime() / 1000);
const MIB = 1024 * 1024;

describe("parseSizeText", () => {
  it("reads bytes and units in powers of 1024", () => {
    expect(parseSizeText("")).toBeNull();
    expect(parseSizeText("  ")).toBeNull();
    expect(parseSizeText("0")).toBe(0);
    expect(parseSizeText("1536")).toBe(1536);
    expect(parseSizeText("500K")).toBe(500 * 1024);
    expect(parseSizeText("500 KB")).toBe(500 * 1024);
    expect(parseSizeText("1.5 MB")).toBe(1.5 * MIB);
    expect(parseSizeText("1,5mb")).toBe(1.5 * MIB);
    expect(parseSizeText("2G")).toBe(2 * 1024 * MIB);
    expect(parseSizeText("1 TiB")).toBe(1024 * 1024 * MIB);
  });

  it("refuses anything else", () => {
    for (const text of ["big", "-1", "10 XB", "1e3", "1.2.3M", "M", "99999999999 TB"]) {
      expect(parseSizeText(text), text).toBeUndefined();
    }
  });
});

describe("sizeRange", () => {
  it("gives presets their bounds", () => {
    expect(sizeRange({ preset: "any", atLeast: "", atMost: "" })).toEqual({
      start: null,
      end: null,
    });
    expect(sizeRange({ preset: "empty", atLeast: "", atMost: "" })).toEqual({
      start: null,
      end: 0,
    });
    expect(sizeRange({ preset: "large", atLeast: "9", atMost: "" })).toEqual({
      start: 100 * MIB + 1,
      end: null,
    });
  });

  it("reads custom ends and checks they make sense", () => {
    expect(sizeRange({ preset: "custom", atLeast: "1 KB", atMost: "" })).toEqual({
      start: 1024,
      end: null,
    });
    expect(sizeRange({ preset: "custom", atLeast: "", atMost: "2M" })).toEqual({
      start: null,
      end: 2 * MIB,
    });
    expect(sizeRange({ preset: "custom", atLeast: "huge", atMost: "" })).toEqual({
      error: "huge is not a size",
    });
    expect(sizeRange({ preset: "custom", atLeast: "2M", atMost: "1M" })).toHaveProperty("error");
  });
});

describe("dateRange", () => {
  // Wednesday 2026-11-18, 15:30 local time; weeks start on Monday.
  const now = new Date(2026, 10, 18, 15, 30);
  const monday = 1;
  const range = (preset: Parameters<typeof dateRange>[0]["preset"], from = "", to = "") =>
    dateRange({ preset, from, to }, now, monday);

  it("starts presets at local midnight", () => {
    expect(range("any")).toEqual({ start: null, end: null });
    expect(range("today")).toEqual({ start: seconds(new Date(2026, 10, 18)), end: null });
    expect(range("yesterday")).toEqual({
      start: seconds(new Date(2026, 10, 17)),
      end: seconds(new Date(2026, 10, 18)) - 1,
    });
    expect(range("week")).toEqual({ start: seconds(new Date(2026, 10, 16)), end: null });
    expect(range("month")).toEqual({ start: seconds(new Date(2026, 10, 1)), end: null });
    expect(range("year")).toEqual({ start: seconds(new Date(2026, 0, 1)), end: null });
    expect(range("older")).toEqual({ start: null, end: seconds(new Date(2026, 0, 1)) - 1 });
  });

  it("follows the first day of the week", () => {
    const sunday = 0;
    expect(dateRange({ preset: "week", from: "", to: "" }, now, sunday)).toEqual({
      start: seconds(new Date(2026, 10, 15)),
      end: null,
    });
  });

  it("takes whole days for a custom range", () => {
    expect(range("custom", "2026-11-01", "2026-11-01")).toEqual({
      start: seconds(new Date(2026, 10, 1)),
      end: seconds(new Date(2026, 10, 2)) - 1,
    });
    expect(range("custom", "", "2025-12-31")).toEqual({
      start: null,
      end: seconds(new Date(2026, 0, 1)) - 1,
    });
    expect(range("custom", "2026-02-30", "")).toEqual({ error: "2026-02-30 is not a date" });
    expect(range("custom", "yesterday", "")).toHaveProperty("error");
    expect(range("custom", "2026-11-02", "2026-11-01")).toEqual({
      error: "The first day is after the last",
    });
  });
});

describe("buildSearchRequest", () => {
  const place = {
    searchId: "search-1",
    location: { kind: "remote" as const, sessionId: "session" },
    folder: "/srv",
    now: new Date(2026, 10, 18, 15, 30),
    weekStartsOn: 1,
  };

  it("carries every choice of the form", () => {
    const form: SearchForm = {
      ...defaultSearchForm(true),
      pattern: "*.log",
      patternKind: "regex",
      caseSensitive: true,
      kind: "files",
      size: { preset: "medium", atLeast: "", atMost: "" },
      date: { preset: "today", from: "", to: "" },
      depth: 3,
      followLinks: true,
    };
    expect(buildSearchRequest(form, place)).toEqual({
      searchId: "search-1",
      location: place.location,
      folder: "/srv",
      pattern: "*.log",
      patternKind: "regex",
      caseSensitive: true,
      kind: "files",
      minSize: MIB,
      maxSize: 100 * MIB,
      modifiedAfter: seconds(new Date(2026, 10, 18)),
      modifiedBefore: null,
      maxDepth: 3,
      hidden: true,
      followLinks: true,
      limit: SEARCH_LIMIT,
    });
  });

  it("says why a form cannot be searched", () => {
    const form = {
      ...defaultSearchForm(),
      size: { preset: "custom" as const, atLeast: "lots", atMost: "" },
    };
    expect(buildSearchRequest(form, place)).toEqual({ error: "lots is not a size" });
  });
});

describe("descriptions", () => {
  it("name filters in short", () => {
    expect(describeSize({ preset: "huge", atLeast: "", atMost: "" })).toBe("Larger than 1 GB");
    expect(describeSize({ preset: "custom", atLeast: "5 MB", atMost: "" })).toBe("At least 5 MB");
    expect(describeSize({ preset: "custom", atLeast: "1K", atMost: "2K" })).toBe("1K to 2K");
    expect(describeSize({ preset: "custom", atLeast: " ", atMost: "" })).toBe("Any size");
    expect(describeDate({ preset: "custom", from: "2026-11-01", to: "2026-11-01" })).toBe(
      "On 2026-11-01",
    );
    expect(describeDate({ preset: "custom", from: "", to: "2026-01-01" })).toBe("Until 2026-01-01");
  });

  it("sum up a search with only what narrows it", () => {
    expect(describeSearch(defaultSearchForm())).toBe("Everything");
    expect(
      describeSearch({
        ...defaultSearchForm(true),
        pattern: "report",
        kind: "folders",
        date: { preset: "week", from: "", to: "" },
        depth: 1,
      }),
    ).toBe("report, folders only, this week, this folder only, hidden folders");
  });
});

describe("rememberSearch", () => {
  const search = (pattern: string) => ({ ...defaultSearchForm(), pattern });

  it("puts the newest first, each once", () => {
    const recent = rememberSearch([search("b"), search("a")], search("a"));
    expect(recent.map((form) => form.pattern)).toEqual(["a", "b"]);
  });

  it("tells searches with other filters apart", () => {
    const larger = { ...search("a"), size: { preset: "large" as const, atLeast: "", atMost: "" } };
    expect(rememberSearch([search("a")], larger)).toHaveLength(2);
  });

  it("keeps a few", () => {
    let recent: SearchForm[] = [];
    for (let index = 0; index < RECENT_SEARCHES + 5; index++) {
      recent = rememberSearch(recent, search(String(index)));
    }
    expect(recent).toHaveLength(RECENT_SEARCHES);
    expect(recent[0].pattern).toBe(String(RECENT_SEARCHES + 4));
  });
});

describe("folders of results", () => {
  it("are read from the path", () => {
    expect(folderOf(entry("/srv/a/b.txt"), "posix")).toBe("/srv/a");
    expect(folderOf(entry("/b.txt"), "posix")).toBe("/");
    expect(folderOf(entry("C:\\Users\\b.txt"), "windows")).toBe("C:\\Users");
    expect(folderOf(entry("C:\\b.txt"), "windows")).toBe("C:\\");
    expect(folderOf(entry("/srv/two\nlines/x"), "posix")).toBe("/srv/two\nlines");
  });

  it("show relative to where the search started", () => {
    expect(relativeFolder("/srv", entry("/srv/a.txt"), "posix")).toBe(".");
    expect(relativeFolder("/srv", entry("/srv/logs/old/a.txt"), "posix")).toBe("logs/old");
    expect(relativeFolder("/srv/", entry("/srv/logs/a.txt"), "posix")).toBe("logs");
    expect(relativeFolder("/", entry("/etc/motd"), "posix")).toBe("etc");
    expect(relativeFolder("/", entry("/motd"), "posix")).toBe(".");
    expect(relativeFolder("/srv", entry("/elsewhere/a"), "posix")).toBe("/elsewhere");
    expect(relativeFolder("C:\\Data", entry("C:\\data\\Logs\\a.txt"), "windows")).toBe("Logs");
    expect(relativeFolder("C:\\", entry("C:\\Logs\\a.txt"), "windows")).toBe("Logs");
  });

  it("are shared only when every result is in one", () => {
    expect(sharedFolder([entry("/srv/a/x"), entry("/srv/a/y")], "posix")).toBe("/srv/a");
    expect(sharedFolder([entry("/srv/a/x"), entry("/srv/b/y")], "posix")).toBeNull();
    expect(sharedFolder([], "posix")).toBeNull();
  });
});

describe("looking at results again", () => {
  it("asks about every result while they are in few folders", () => {
    const entries = [entry("/srv/a/x"), entry("/srv/b/y"), entry("/srv/z")];
    expect(recheckTargets(entries, "/srv", "posix", ["/srv/new"])).toEqual([
      "/srv/new",
      "/srv/a/x",
      "/srv/b/y",
      "/srv/z",
    ]);
  });

  it("asks about those in the start folder alone when they are spread wide", () => {
    const spread = Array.from({ length: RECHECK_ALL_FOLDERS + 1 }, (_, index) =>
      entry(`/srv/folder-${index}/file`),
    );
    const entries = [...spread, entry("/srv/top")];
    expect(recheckTargets(entries, "/srv", "posix", ["/srv/folder-3/file"])).toEqual([
      "/srv/folder-3/file",
      "/srv/top",
    ]);
  });

  it("never asks about too many at once", () => {
    const entries = Array.from({ length: MAX_RECHECKED_PATHS + 10 }, (_, index) =>
      entry(`/srv/file-${index}`),
    );
    expect(recheckTargets(entries, "/srv", "posix")).toHaveLength(MAX_RECHECKED_PATHS);
  });

  it("updates what changed, drops what went and adds what was renamed", () => {
    const entries = [entry("/srv/a"), entry("/srv/b"), entry("/srv/c")];
    const result = applyRecheck(
      entries,
      {
        entries: [entry("/srv/a", { size: 99 }), entry("/srv/renamed")],
        missing: ["/srv/b", "/srv/old-name"],
      },
      ["/srv/a", "/srv/b", "/srv/renamed", "/srv/old-name"],
    );
    expect(result.map((item) => [item.path, item.size])).toEqual([
      ["/srv/a", 99],
      ["/srv/c", 1],
      ["/srv/renamed", 1],
    ]);
  });
});
