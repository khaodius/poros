// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FileEntry } from "../lib/types";
import { RenameField } from "./FileList";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const report = (): FileEntry => ({
  name: "report.txt",
  path: "/srv/report.txt",
  kind: "file",
  size: 1,
  modified: null,
  permissions: null,
  owner: null,
  group: null,
  hidden: false,
});

const events = {
  onPointerDown: () => {},
  onClick: () => {},
  onOpen: () => {},
  onContextMenu: () => {},
  onRenameSubmit: async () => true,
  onRenameCancel: () => {},
};

let root: Root | null = null;
let container: HTMLElement;

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  container.remove();
});

async function show(entry: FileEntry) {
  if (!root) {
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  }
  await act(async () => root!.render(createElement(RenameField, { entry, events })));
}

const field = () => container.querySelector("input")!;

/** Types `text` in place of what is selected, as a key press does. */
async function type(text: string) {
  const input = field();
  const start = input.selectionStart ?? 0;
  const value = input.value.slice(0, start) + text + input.value.slice(input.selectionEnd ?? 0);
  const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => {
    setValue.call(input, value);
    input.setSelectionRange(start + text.length, start + text.length);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("RenameField", () => {
  it("selects the name before the extension to start with", async () => {
    await show(report());
    expect([field().selectionStart, field().selectionEnd]).toEqual([0, "report".length]);
  });

  it("keeps what was typed when a reload hands it a new entry for the same file", async () => {
    await show(report());
    await type("summary");
    expect(field().value).toBe("summary.txt");
    await show(report());
    await type("-2026");
    expect(field().value).toBe("summary-2026.txt");
  });
});
