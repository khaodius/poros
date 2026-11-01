import { describe, expect, it } from "vitest";
import { defaultSelection, folderPrefix, plannedImports } from "./connectionImport";
import type { ImportEntry } from "./types";

function entry(name: string, extra: Partial<ImportEntry> = {}): ImportEntry {
  return {
    connection: {
      id: "",
      protocol: "sftp",
      name,
      host: `${name}.example.com`,
      port: 22,
      username: "alice",
      authType: "password",
      saveSecret: false,
    },
    folder: [],
    hasPassword: false,
    notes: [],
    ...extra,
  };
}

describe("connection import", () => {
  it("leaves out sites saved already", () => {
    const entries = [entry("a"), entry("b", { existingId: "x" }), entry("c")];
    expect([...defaultSelection(entries)]).toEqual([0, 2]);
  });

  it("brings along the jump hosts a site needs, unless they are saved", () => {
    const entries = [
      entry("app", { jump: 1 }),
      entry("bastion", { jump: 2 }),
      entry("gate"),
      entry("inner", { jump: 4 }),
      entry("saved", { existingId: "s" }),
      entry("loop", { jump: 5 }),
    ];
    const planned = plannedImports(entries, new Set([0, 3, 5]));
    expect([...planned.entries()].sort()).toEqual([
      [0, null],
      [1, 0],
      [2, 0],
      [3, null],
      [5, null],
    ]);
    // A saved jump host the user ticks is imported again.
    expect(plannedImports(entries, new Set([3, 4])).get(4)).toBeNull();
  });

  it("writes folder paths", () => {
    expect(folderPrefix(entry("a", { folder: ["Work", "Acme"] }))).toBe("Work / Acme / ");
    expect(folderPrefix(entry("a"))).toBe("");
  });
});
