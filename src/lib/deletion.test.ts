import { describe, expect, it } from "vitest";
import { deleteFraction, deleteProgressText, entriesStillListed } from "./deletion";
import type { FileEntry } from "./types";

describe("delete progress", () => {
  it("counts first, then shows how much of the total is gone", () => {
    expect(deleteProgressText({ files: 0 })).toBe("Counting what to delete...");
    expect(deleteProgressText({ files: 12, total: 4000, counting: true })).toBe(
      "12 deleted, 4,000 found so far",
    );
    expect(deleteProgressText({ files: 1204, total: 5000 })).toBe("1,204 of 5,000 deleted");
    expect(deleteProgressText({ files: 3 })).toBe("3 deleted");
  });

  it("fills the bar once the total is known, failures included", () => {
    expect(deleteFraction({ files: 10, total: 100, counting: true })).toBeNull();
    expect(deleteFraction({ files: 0 })).toBeNull();
    expect(deleteFraction({ files: 40, total: 100, failed: 10 })).toBe(0.5);
    expect(deleteFraction({ files: 120, total: 100 })).toBe(1);
  });
});

describe("entriesStillListed", () => {
  const entry = (path: string, change: Partial<FileEntry> = {}): FileEntry => ({
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind: "file",
    size: 1,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
    ...change,
  });

  it("leaves out what the folder no longer lists", () => {
    const chosen = [entry("/srv/a"), entry("/srv/b"), entry("/srv/c")];
    const listed = [entry("/srv/c"), entry("/srv/a"), entry("/srv/z")];
    expect(entriesStillListed(chosen, listed).map((kept) => kept.path)).toEqual([
      "/srv/a",
      "/srv/c",
    ]);
    expect(entriesStillListed(chosen, [])).toEqual([]);
  });

  it("tells each entry as the folder lists it now", () => {
    const chosen = [entry("/srv/a")];
    const listed = [entry("/srv/a", { kind: "dir" })];
    expect(entriesStillListed(chosen, listed)[0]).toBe(listed[0]);
  });
});
