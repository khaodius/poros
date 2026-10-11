import { describe, expect, it } from "vitest";
import { entryAfterRemoval, entryLeadingTo } from "./cursor";
import type { FileEntry } from "./types";

function entry(path: string): FileEntry {
  return {
    name: path.split(/[/\\]/).pop() ?? path,
    path,
    kind: "dir",
    size: 0,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

describe("entryLeadingTo", () => {
  const entries = [entry("/srv/app"), entry("/srv/application"), entry("/srv/logs")];

  it("finds the folder a deeper path is inside", () => {
    expect(entryLeadingTo(entries, "/srv/app/releases/current")).toBe("/srv/app");
  });

  it("finds the entry the path names", () => {
    expect(entryLeadingTo(entries, "/srv/application")).toBe("/srv/application");
  });

  it("does not mistake a longer name for a parent", () => {
    expect(entryLeadingTo(entries, "/srv/applications/x")).toBeUndefined();
  });

  it("reads local Windows paths", () => {
    const local = [entry("C:\\Users\\poros\\Documents"), entry("C:\\Users\\poros\\Downloads")];
    expect(entryLeadingTo(local, "C:\\Users\\poros\\Downloads\\iso")).toBe(
      "C:\\Users\\poros\\Downloads",
    );
  });

  it("finds nothing for a path outside the listing", () => {
    expect(entryLeadingTo(entries, "/home/poros")).toBeUndefined();
  });
});

describe("entryAfterRemoval", () => {
  const entries = ["/a", "/b", "/c", "/d", "/e"].map(entry);

  it("keeps a cursor that was not removed", () => {
    expect(entryAfterRemoval(entries, new Set(["/b"]), "/d")).toBe("/d");
  });

  it("moves to the next row that stays", () => {
    expect(entryAfterRemoval(entries, new Set(["/b", "/c"]), "/b")).toBe("/d");
  });

  it("moves back when nothing after the cursor stays", () => {
    expect(entryAfterRemoval(entries, new Set(["/d", "/e"]), "/e")).toBe("/c");
  });

  it("starts from the first removed row without a cursor", () => {
    expect(entryAfterRemoval(entries, new Set(["/c", "/a"]), null)).toBe("/b");
  });

  it("finds nothing when every row goes", () => {
    expect(
      entryAfterRemoval(entries, new Set(entries.map((item) => item.path)), "/a"),
    ).toBeUndefined();
  });
});
