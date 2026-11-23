import { describe, expect, it } from "vitest";
import { onlySelected, rangeStart } from "./selection";
import type { FileEntry } from "./types";

function entry(path: string): FileEntry {
  return {
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind: "file",
    size: 0,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

describe("rangeStart", () => {
  const shown = ["/a", "/b", "/c", "/d", "/e"].map(entry);

  it("is the anchor, wherever the cursor has gone since", () => {
    expect(rangeStart(shown, "/b", "/d", "/e")).toBe("/b");
  });

  it("is the cursor when there is no anchor, as after Esc", () => {
    expect(rangeStart(shown, null, "/a", "/e")).toBe("/a");
  });

  it("is the row the range runs to when there is neither", () => {
    expect(rangeStart(shown, null, null, "/c")).toBe("/c");
  });

  it("passes over an anchor that is not on show, as one the filter hides", () => {
    expect(rangeStart(shown, "/hidden", "/b", "/e")).toBe("/b");
    expect(rangeStart(shown, "/hidden", "/hidden", "/e")).toBe("/e");
    expect(rangeStart(shown, null, "/hidden", "/e")).toBe("/e");
  });
});

describe("onlySelected", () => {
  const shown = ["/a.txt", "/b.txt", "/c.txt"].map(entry);

  it("is the one row selected", () => {
    expect(onlySelected(shown, new Set(["/b.txt"]))?.path).toBe("/b.txt");
  });

  it("is none with several rows selected, or none", () => {
    expect(onlySelected(shown, new Set(["/a.txt", "/c.txt"]))).toBeUndefined();
    expect(onlySelected(shown, new Set())).toBeUndefined();
  });

  it("counts only the rows on show, as with a hidden file also selected", () => {
    expect(onlySelected(shown, new Set(["/.env", "/a.txt"]))?.path).toBe("/a.txt");
    expect(onlySelected(shown, new Set(["/.env"]))).toBeUndefined();
    expect(onlySelected(shown, new Set(["/.env", "/.cache", "/a.txt", "/b.txt"]))).toBeUndefined();
  });
});
