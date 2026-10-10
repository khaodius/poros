import { describe, expect, it } from "vitest";
import { focusReturnCandidates, isTextEntryTarget, type FocusReturn } from "./focus";

interface FakeElement {
  label: string;
  isConnected: boolean;
}

const body: FakeElement = { label: "body", isConnected: true };
const fileList: FakeElement = { label: "file list", isConnected: true };
const otherFileList: FakeElement = { label: "other file list", isConnected: true };
const menuItem: FakeElement = { label: "menu item", isConnected: false };
const filterInput: FakeElement = { label: "filter", isConnected: true };

function closing(change: Partial<FocusReturn<FakeElement>>): FakeElement[] {
  return focusReturnCandidates<FakeElement>({
    active: body,
    previous: fileList,
    body,
    inside: (element) => element === menuItem,
    moved: false,
    fallback: otherFileList,
    ...change,
  });
}

describe("focusReturnCandidates", () => {
  it("gives focus back to what had it when it fell to the page body", () => {
    expect(closing({})).toEqual([fileList, otherFileList]);
    expect(closing({ active: null })).toEqual([fileList, otherFileList]);
  });

  it("gives focus back while the closing menu still holds it", () => {
    expect(closing({ active: menuItem })).toEqual([fileList, otherFileList]);
  });

  it("leaves focus alone when something else took it", () => {
    expect(closing({ active: filterInput })).toEqual([]);
  });

  it("falls back when what had focus is gone or was nothing", () => {
    expect(closing({ previous: { label: "closed tab", isConnected: false } })).toEqual([
      otherFileList,
    ]);
    expect(closing({ previous: body })).toEqual([otherFileList]);
    expect(closing({ previous: null })).toEqual([otherFileList]);
    expect(closing({ previous: null, fallback: null })).toEqual([]);
  });

  it("does not pull the user back to a tab group they left", () => {
    expect(closing({ moved: true })).toEqual([otherFileList]);
    expect(closing({ moved: true, fallback: null })).toEqual([]);
  });

  it("offers the file list once when it is also the fallback", () => {
    expect(closing({ fallback: fileList })).toEqual([fileList]);
  });
});

function fakeTarget(fields: { tagName: string; type?: string; isContentEditable?: boolean }) {
  return fields as unknown as EventTarget;
}

describe("isTextEntryTarget", () => {
  it("finds the fields people type in", () => {
    expect(isTextEntryTarget(fakeTarget({ tagName: "INPUT", type: "text" }))).toBe(true);
    expect(isTextEntryTarget(fakeTarget({ tagName: "INPUT", type: "search" }))).toBe(true);
    expect(isTextEntryTarget(fakeTarget({ tagName: "INPUT", type: "number" }))).toBe(true);
    expect(isTextEntryTarget(fakeTarget({ tagName: "INPUT", type: "password" }))).toBe(true);
    expect(isTextEntryTarget(fakeTarget({ tagName: "TEXTAREA" }))).toBe(true);
    expect(isTextEntryTarget(fakeTarget({ tagName: "DIV", isContentEditable: true }))).toBe(true);
  });

  it("leaves out controls without text, and nothing at all", () => {
    expect(isTextEntryTarget(fakeTarget({ tagName: "INPUT", type: "checkbox" }))).toBe(false);
    expect(isTextEntryTarget(fakeTarget({ tagName: "INPUT", type: "range" }))).toBe(false);
    expect(isTextEntryTarget(fakeTarget({ tagName: "BUTTON", type: "button" }))).toBe(false);
    expect(isTextEntryTarget(fakeTarget({ tagName: "DIV", isContentEditable: false }))).toBe(false);
    expect(isTextEntryTarget(null)).toBe(false);
  });
});
