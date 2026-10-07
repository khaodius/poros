import { describe, expect, it } from "vitest";
import {
  chosenItems,
  chosenTotals,
  describeCounts,
  describePlanned,
  describeReason,
  describeRun,
  excludePatterns,
  initialChoices,
  isWithin,
  matchesFilter,
  possibleActions,
} from "./sync";
import type { SyncItem } from "./types";

function item(id: number, overrides: Partial<SyncItem> = {}): SyncItem {
  return {
    id,
    path: `folder/file-${id}`,
    isDir: false,
    action: "upload",
    reason: "new",
    files: 1,
    bytes: 100,
    ...overrides,
  };
}

const conflict = item(3, {
  action: "conflict",
  reason: "bothChanged",
  bytes: 0,
  local: { isDir: false, size: 40, modified: 10 },
  remote: { isDir: false, size: 70, modified: 10 },
});
const mismatch = item(4, { action: "conflict", reason: "typeDiffers", bytes: 0 });
const items = [
  item(0),
  item(1, { action: "download", reason: "changed", bytes: 250 }),
  item(2, { action: "deleteRemote", reason: "extraneous", isDir: true, files: 3, bytes: 900 }),
  conflict,
  mismatch,
];

describe("sync choices", () => {
  it("takes every planned item except conflicts", () => {
    const choices = initialChoices(items);
    expect(chosenItems(items, choices)).toEqual([
      { id: 0, action: "upload" },
      { id: 1, action: "download" },
      { id: 2, action: "deleteRemote" },
    ]);
  });

  it("lets two changed files be settled either way, but not a file against a folder", () => {
    expect(possibleActions(conflict)).toEqual(["upload", "download"]);
    expect(possibleActions(mismatch)).toEqual([]);
    expect(possibleActions(items[2])).toEqual(["deleteRemote"]);
  });

  it("totals what was chosen, sizing a settled conflict by the side copied", () => {
    const choices = new Map(initialChoices(items)).set(3, "download").set(0, null);
    const totals = chosenTotals(items, choices);
    expect(totals.upload).toEqual({ items: 0, files: 0, bytes: 0 });
    expect(totals.download).toEqual({ items: 2, files: 2, bytes: 320 });
    expect(totals.delete).toEqual({ items: 1, files: 3, bytes: 900 });
    expect(totals.undecided).toBe(0);
    expect(describePlanned(totals)).toBe("Download 2 files, 320 B. Delete 1 item.");
  });

  it("counts undecided conflicts and empty folders", () => {
    const folder = item(5, { isDir: true, files: 0, bytes: 0 });
    const totals = chosenTotals([folder, conflict, mismatch], initialChoices([folder, conflict]));
    expect(totals.undecided).toBe(1);
    expect(describePlanned(totals)).toBe("Upload 1 folder. 1 conflict left as it is.");
    expect(describePlanned(chosenTotals([], new Map()))).toBe("Nothing selected.");
  });

  it("filters by action", () => {
    expect(
      items.filter((entry) => matchesFilter(entry, "delete")).map((entry) => entry.id),
    ).toEqual([2]);
    expect(items.filter((entry) => matchesFilter(entry, "conflict"))).toHaveLength(2);
    expect(items.filter((entry) => matchesFilter(entry, "all"))).toHaveLength(5);
  });
});

describe("sync descriptions", () => {
  it("explains each item from the side it concerns", () => {
    expect(describeReason(item(0))).toBe("Only in the local folder");
    expect(describeReason(item(0, { action: "download" }))).toBe("Only on the server");
    expect(describeReason(item(0, { action: "deleteLocal", reason: "extraneous" }))).toBe(
      "Not on the server",
    );
    expect(describeReason(item(0, { reason: "newer" }))).toBe("Newer in the local folder");
    expect(
      describeReason(
        item(0, {
          action: "conflict",
          reason: "typeDiffers",
          local: { isDir: true, size: 0, modified: null },
        }),
      ),
    ).toBe("Folder here, file on the server");
  });

  it("lists what was left alone", () => {
    const counts = { unchanged: 12, kept: 0, extraOnTarget: 2, excluded: 3, passedOver: 0 };
    expect(describeCounts(counts, "upload")).toBe(
      "12 unchanged, 2 only on the server, not deleted, 3 excluded",
    );
    expect(describeCounts({ ...counts, unchanged: 0, excluded: 0 }, "download")).toBe(
      "2 only in the local folder, not deleted",
    );
  });

  it("summarizes a run", () => {
    expect(
      describeRun({
        queuedFiles: 1,
        queuedBytes: 2048,
        deleted: 0,
        createdFolders: 0,
        failures: [],
      }),
    ).toBe("Queued 1 file (2.0 KB).");
    expect(
      describeRun({
        queuedFiles: 0,
        queuedBytes: 0,
        deleted: 3,
        createdFolders: 1,
        failures: ["a: denied", "b: denied"],
      }),
    ).toBe(
      "Queued 0 files (0 B), deleted 3 items, created 1 folder. 2 changes failed; the log has the details.",
    );
  });
});

describe("excludePatterns", () => {
  it("keeps one pattern per non-blank line", () => {
    expect(excludePatterns("*.tmp\r\n\n  \nnode_modules/\n# note")).toEqual([
      "*.tmp",
      "node_modules/",
      "# note",
    ]);
    expect(excludePatterns("")).toEqual([]);
  });
});

describe("isWithin", () => {
  it("matches a folder and what is inside it, not its namesakes", () => {
    expect(isWithin("/srv/site", "/srv/site", "posix")).toBe(true);
    expect(isWithin("/srv/site/css", "/srv/site", "posix")).toBe(true);
    expect(isWithin("/srv/site-old", "/srv/site", "posix")).toBe(false);
    expect(isWithin("/anything", "/", "posix")).toBe(true);
  });

  it("ignores case and slash direction on Windows", () => {
    expect(isWithin("c:\\Users\\Me\\Site\\css", "C:\\Users\\me\\site", "windows")).toBe(true);
    expect(isWithin("C:/Users/me/site", "C:\\Users\\me\\site\\", "windows")).toBe(true);
    expect(isWithin("C:\\Users\\me\\sites", "C:\\Users\\me\\site", "windows")).toBe(false);
  });
});
