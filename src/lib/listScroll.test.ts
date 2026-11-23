import { describe, expect, it } from "vitest";
import { cursorScroll, type ListChange } from "./listScroll";

const nothing: ListChange = {
  opened: false,
  reloaded: false,
  cursorMoved: false,
  groupToggled: false,
};

describe("cursorScroll", () => {
  it("puts the cursor's row in the middle of a folder just opened", () => {
    expect(cursorScroll({ ...nothing, opened: true })).toBe("center");
    expect(cursorScroll({ ...nothing, opened: true, reloaded: true, groupToggled: true })).toBe(
      "center",
    );
  });

  it("follows the cursor as it moves", () => {
    expect(cursorScroll({ ...nothing, cursorMoved: true })).toBe("auto");
  });

  it("keeps the cursor's row in view as the rows are sorted or filtered", () => {
    expect(cursorScroll(nothing)).toBe("auto");
  });

  it("stays where it was scrolled when the same folder is listed again", () => {
    expect(cursorScroll({ ...nothing, reloaded: true })).toBeNull();
  });

  it("follows a reload that moves the cursor, as onto a folder just made", () => {
    expect(cursorScroll({ ...nothing, reloaded: true, cursorMoved: true })).toBe("auto");
  });

  it("stays where it was scrolled when a group is collapsed or expanded", () => {
    expect(cursorScroll({ ...nothing, groupToggled: true })).toBeNull();
    expect(cursorScroll({ ...nothing, groupToggled: true, cursorMoved: true })).toBeNull();
  });
});
