import { describe, expect, it } from "vitest";
import { isFocusPreviewKey, nextPane, type PaneStop } from "./paneFocus";

const left: PaneStop<string> = { pane: "left", left: 0, top: 40 };
const right: PaneStop<string> = { pane: "right", left: 600, top: 40 };
const below: PaneStop<string> = { pane: "below", left: 0, top: 500 };

describe("nextPane", () => {
  it("goes from list to list, left to right, then round to the first", () => {
    expect(nextPane([right, left], "left")).toBe("right");
    expect(nextPane([right, left], "right")).toBe("left");
    expect(nextPane([left, below, right], "left")).toBe("below");
    expect(nextPane([left, below, right], "below")).toBe("right");
  });

  it("goes from a preview to the next pane's list, not back to its own", () => {
    // Tab in the left pane's preview, with the right pane beside it.
    expect(nextPane([left, right], "left", true)).toBe("right");
  });

  it("stays in the only list, as before previews, but leaves a preview for it", () => {
    expect(nextPane([left], "left")).toBeNull();
    expect(nextPane([left], "left", true)).toBe("left");
    expect(nextPane([], null, true)).toBeNull();
  });

  it("goes to the first list from somewhere that is no pane's", () => {
    expect(nextPane([right, left], null)).toBe("left");
  });
});

describe("isFocusPreviewKey", () => {
  const press = (
    key: string,
    held: Partial<Record<"ctrl" | "meta" | "shift" | "alt", boolean>>,
  ) => ({
    key,
    ctrlKey: held.ctrl ?? false,
    metaKey: held.meta ?? false,
    shiftKey: held.shift ?? false,
    altKey: held.alt ?? false,
  });

  it("is Ctrl+Shift+P, or Cmd+Shift+P", () => {
    expect(isFocusPreviewKey(press("P", { ctrl: true, shift: true }))).toBe(true);
    expect(isFocusPreviewKey(press("p", { meta: true, shift: true }))).toBe(true);
  });

  it("is not Tab, nor P with other keys held", () => {
    expect(isFocusPreviewKey(press("Tab", {}))).toBe(false);
    expect(isFocusPreviewKey(press("Tab", { shift: true }))).toBe(false);
    expect(isFocusPreviewKey(press("p", { ctrl: true }))).toBe(false);
    expect(isFocusPreviewKey(press("P", { shift: true }))).toBe(false);
    expect(isFocusPreviewKey(press("P", { ctrl: true, shift: true, alt: true }))).toBe(false);
  });
});
