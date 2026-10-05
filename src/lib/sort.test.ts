import { describe, expect, it } from "vitest";
import { filterEntries, sortEntries } from "./sort";
import type { FileEntry } from "./types";

function entry(name: string, overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name,
    path: `/${name}`,
    kind: "file",
    size: 0,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: name.startsWith("."),
    ...overrides,
  };
}

const names = (entries: FileEntry[]) => entries.map((item) => item.name);

describe("sortEntries", () => {
  const entries = [
    entry("file10.txt", { size: 30 }),
    entry("file2.txt", { size: 10 }),
    entry("Zeta", { kind: "dir" }),
    entry("alpha", { kind: "dir" }),
    entry("link-to-dir", { kind: "symlink", linkTarget: "dir" }),
    entry("File1.txt", { size: 20 }),
  ];

  it("keeps folders first and orders names naturally", () => {
    expect(names(sortEntries(entries, { key: "name", direction: 1 }))).toEqual([
      "alpha",
      "link-to-dir",
      "Zeta",
      "File1.txt",
      "file2.txt",
      "file10.txt",
    ]);
  });

  it("keeps folders first when descending", () => {
    expect(names(sortEntries(entries, { key: "name", direction: -1 }))).toEqual([
      "Zeta",
      "link-to-dir",
      "alpha",
      "file10.txt",
      "file2.txt",
      "File1.txt",
    ]);
  });

  it("sorts by size with the name as tiebreaker", () => {
    const sized = [entry("b", { size: 5 }), entry("a", { size: 5 }), entry("c", { size: 1 })];
    expect(names(sortEntries(sized, { key: "size", direction: 1 }))).toEqual(["c", "a", "b"]);
  });

  it("does not mutate its input", () => {
    const original = names(entries);
    sortEntries(entries, { key: "size", direction: -1 });
    expect(names(entries)).toEqual(original);
  });
});

describe("filterEntries", () => {
  const entries = [entry(".profile"), entry("Notes.md"), entry("notebook"), entry("photo.jpg")];

  it("hides dotfiles unless asked", () => {
    expect(names(filterEntries(entries, { showHidden: false, query: "" }))).toEqual([
      "Notes.md",
      "notebook",
      "photo.jpg",
    ]);
    expect(filterEntries(entries, { showHidden: true, query: "" })).toHaveLength(4);
  });

  it("matches the query anywhere in the name, ignoring case", () => {
    expect(names(filterEntries(entries, { showHidden: true, query: " NOTE " }))).toEqual([
      "Notes.md",
      "notebook",
    ]);
  });
});
