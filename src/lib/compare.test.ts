import { describe, expect, it } from "vitest";
import {
  adjacentChange,
  changeStarts,
  displayItems,
  lineEndingNote,
  overviewMarks,
  segments,
  swapComparison,
  verdict,
} from "./compare";
import type { CompareRow, CompareRowKind, Comparison } from "./types";

function rowsOf(kinds: string): CompareRow[] {
  const names: Record<string, CompareRowKind> = {
    "=": "equal",
    "~": "changed",
    "+": "added",
    "-": "removed",
  };
  return [...kinds].map((symbol) => ({ kind: names[symbol] }));
}

describe("the rows on show", () => {
  it("shows every row unless only changes are asked for", () => {
    expect(displayItems(rowsOf("=~="), false)).toEqual([
      { kind: "row", index: 0 },
      { kind: "row", index: 1 },
      { kind: "row", index: 2 },
    ]);
  });

  it("folds unchanged rows away from the changes", () => {
    const rows = rowsOf("=======~=======+=");
    expect(displayItems(rows, true, new Set(), 2)).toEqual([
      { kind: "fold", start: 0, end: 5 },
      ...[5, 6, 7, 8, 9].map((index) => ({ kind: "row", index })),
      { kind: "fold", start: 10, end: 13 },
      ...[13, 14, 15, 16].map((index) => ({ kind: "row", index })),
    ]);
  });

  it("keeps a single hidden row rather than folding it", () => {
    expect(displayItems(rowsOf("=~"), true, new Set(), 0)).toEqual([
      { kind: "row", index: 0 },
      { kind: "row", index: 1 },
    ]);
  });

  it("opens an expanded fold", () => {
    const items = displayItems(rowsOf("===~"), true, new Set([0]), 0);
    expect(items.map((item) => item.kind)).toEqual(["row", "row", "row", "row"]);
  });

  it("folds everything when nothing changed", () => {
    expect(displayItems(rowsOf("===="), true)).toEqual([{ kind: "fold", start: 0, end: 4 }]);
  });
});

describe("moving between changes", () => {
  const rows = rowsOf("=~~==+=-");
  const items = displayItems(rows, false);

  it("finds where each run of changes starts", () => {
    expect(changeStarts(items, rows)).toEqual([1, 5, 7]);
  });

  it("steps forward and back, stopping at either end", () => {
    const starts = changeStarts(items, rows);
    expect(adjacentChange(starts, -1, 1)).toBe(1);
    expect(adjacentChange(starts, 1, 1)).toBe(5);
    expect(adjacentChange(starts, 7, 1)).toBeNull();
    expect(adjacentChange(starts, 5, -1)).toBe(1);
    expect(adjacentChange(starts, 1, -1)).toBeNull();
  });

  it("marks each run on the overview strip", () => {
    expect(overviewMarks(items, rows)).toEqual([
      { top: 1 / 8, height: 2 / 8, kind: "changed" },
      { top: 5 / 8, height: 1 / 8, kind: "added" },
      { top: 7 / 8, height: 1 / 8, kind: "removed" },
    ]);
  });
});

describe("highlighting a line", () => {
  it("splits a changed line around its changed ranges", () => {
    expect(
      segments("listen 443 ssl;", "changed", [
        [7, 10],
        [10, 14],
      ]),
    ).toEqual([
      { text: "listen ", changed: false },
      { text: "443", changed: true },
      { text: " ssl", changed: true },
      { text: ";", changed: false },
    ]);
  });

  it("marks a changed line without ranges as changed throughout", () => {
    expect(segments("abc", "changed", undefined)).toEqual([{ text: "abc", changed: true }]);
  });

  it("leaves added and removed lines to the row's own color", () => {
    expect(segments("new", "added", [[0, 3]])).toEqual([{ text: "new", changed: false }]);
  });

  it("keeps ranges inside the line", () => {
    expect(segments("ab", "changed", [[1, 9]])).toEqual([
      { text: "a", changed: false },
      { text: "b", changed: true },
    ]);
  });
});

describe("describing the result", () => {
  const text = (counts: Partial<Record<"added" | "removed" | "changed", number>>): Comparison => ({
    leftSize: 10,
    rightSize: 12,
    identical: false,
    content: {
      kind: "text",
      leftLines: [],
      rightLines: [],
      rows: [],
      added: 0,
      removed: 0,
      changed: 0,
      leftLineEnding: "lf",
      rightLineEnding: "lf",
      ...counts,
    },
  });

  it("counts the lines that differ", () => {
    expect(verdict(text({ changed: 2, added: 1 }), false)).toBe("2 changed lines, 1 added line.");
  });

  it("explains a difference no line shows", () => {
    expect(verdict(text({}), true)).toBe("The files differ only in what is being ignored.");
    const endings = text({});
    if (endings.content.kind === "text") endings.content.rightLineEnding = "crlf";
    expect(verdict(endings, false)).toBe("Only the line endings differ.");
  });

  it("says when files are the same or not text", () => {
    expect(verdict({ ...text({}), identical: true }, false)).toBe("The files are identical.");
    expect(verdict({ ...text({}), content: { kind: "binary", tooLarge: false } }, false)).toContain(
      "not text",
    );
  });

  it("notes line endings only when both files have them and they differ", () => {
    expect(lineEndingNote("lf", "crlf")).toBe(
      "Line endings differ: LF (Unix) on the left, CRLF (Windows) on the right.",
    );
    expect(lineEndingNote("lf", "lf")).toBeNull();
    expect(lineEndingNote("none", "crlf")).toBeNull();
  });
});

describe("swapping sides", () => {
  it("turns additions into removals and back", () => {
    const comparison: Comparison = {
      leftSize: 3,
      rightSize: 7,
      identical: false,
      content: {
        kind: "text",
        leftLines: ["a"],
        rightLines: ["a", "b"],
        rows: [
          { kind: "changed", left: 0, right: 0, leftChanges: [[0, 1]] },
          { kind: "added", right: 1 },
        ],
        added: 1,
        removed: 0,
        changed: 1,
        leftLineEnding: "lf",
        rightLineEnding: "crlf",
      },
    };
    const swapped = swapComparison(comparison);
    expect(swapped.leftSize).toBe(7);
    expect(swapped.content).toMatchObject({
      leftLines: ["a", "b"],
      rightLines: ["a"],
      rows: [
        { kind: "changed", left: 0, right: 0, rightChanges: [[0, 1]] },
        { kind: "removed", left: 1 },
      ],
      added: 0,
      removed: 1,
      leftLineEnding: "crlf",
    });
    expect(swapComparison(swapped)).toEqual(expect.objectContaining({ leftSize: 3, rightSize: 7 }));
  });
});
