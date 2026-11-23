import { describe, expect, it } from "vitest";
import { anchoredScrollTop, type AnchorQuery } from "./listAnchor";

const rows = (...names: string[]) => names;

function query(change: Partial<AnchorQuery>): AnchorQuery {
  return {
    before: [],
    after: [],
    scrollTop: 0,
    viewportHeight: 100,
    rowHeight: 20,
    held: [],
    cursor: null,
    ...change,
  };
}

describe("anchoredScrollTop", () => {
  const before = rows("a", "b", "c", "d", "e", "f", "g", "h", "i", "j");

  it("leaves a list alone when nothing moved the rows in view", () => {
    const after = [...before, "k"];
    expect(anchoredScrollTop(query({ before, after, scrollTop: 40 }))).toBeNull();
  });

  it("keeps the first row in view where it was when rows appear above it", () => {
    const after = rows("0", "1", ...before);
    // Row c was at the top; two new rows above push it down by two rows.
    expect(anchoredScrollTop(query({ before, after, scrollTop: 40 }))).toBe(80);
  });

  it("keeps it there when rows above are gone", () => {
    const after = before.slice(1);
    expect(anchoredScrollTop(query({ before, after, scrollTop: 40 }))).toBe(20);
  });

  it("stays at the top, so that new first rows show", () => {
    const after = rows("0", ...before);
    expect(anchoredScrollTop(query({ before, after, scrollTop: 0 }))).toBeNull();
  });

  it("holds on to the cursor's row rather than the first one, while it is in view", () => {
    const after = rows("a", "b", "c", "x", "y", "d", "e", "f", "g", "h", "i", "j");
    // Rows c to g are in view and the cursor is on e, below the rows that are new.
    expect(anchoredScrollTop(query({ before, after, scrollTop: 40, cursor: "e" }))).toBe(80);
    // A cursor scrolled out of view holds nothing: the first row in view stays.
    expect(anchoredScrollTop(query({ before, after, scrollTop: 40, cursor: "j" }))).toBeNull();
  });

  it("holds on to a row being renamed before anything else, also at the top", () => {
    const after = rows("0", "1", ...before);
    const renaming = query({ before, after, scrollTop: 0, held: ["d"], cursor: "a" });
    expect(anchoredScrollTop(renaming)).toBe(40);
  });

  it("moves on to the next row when the first one in view is gone", () => {
    const after = rows("0", "a", "b", "d", "e", "f", "g", "h", "i", "j");
    // Row c is gone and a row is new above: d was one row below the top and still is.
    expect(anchoredScrollTop(query({ before, after, scrollTop: 40 }))).toBeNull();
    const shifted = rows("0", "1", "a", "b", "d", "e", "f", "g", "h", "i", "j");
    expect(anchoredScrollTop(query({ before, after: shifted, scrollTop: 40 }))).toBe(60);
  });

  it("leaves the list alone when no row in view is left, or rows have no height", () => {
    expect(anchoredScrollTop(query({ before, after: rows("x", "y"), scrollTop: 40 }))).toBeNull();
    const after = rows("0", ...before);
    expect(anchoredScrollTop(query({ before, after, scrollTop: 40, rowHeight: 0 }))).toBeNull();
  });
});
