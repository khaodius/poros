import { describe, expect, it } from "vitest";
import { rangeStart } from "./selection";
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
