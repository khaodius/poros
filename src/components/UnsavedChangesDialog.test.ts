// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { UnsavedChangesDialog } from "./UnsavedChangesDialog";

const editor = vi.hoisted(() => ({
  saveAndClose: vi.fn(),
  closeWindowNow: vi.fn(),
  closeEditorTab: vi.fn(),
}));
vi.mock("../state/editorActions", () => editor);
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeAll(() => {
  // The page's dialogs open as modals, which jsdom does not draw.
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
});

let root: Root | null = null;
let container: HTMLElement;
/** Answers the save the dialog started, which waits until then. */
let answerSave: (saved: boolean) => void;

beforeEach(() => {
  vi.clearAllMocks();
  editor.saveAndClose.mockImplementation(
    () =>
      new Promise<boolean>((resolve) => {
        answerSave = resolve;
      }),
  );
});

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  container.remove();
});

async function askOnClosingWindow() {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () =>
    root!.render(
      createElement(UnsavedChangesDialog, {
        tabIds: ["one"],
        closeWindow: true,
        onClose: () => {},
      }),
    ),
  );
}

async function save() {
  const button = [...container.querySelectorAll("button")].find(
    (candidate) => candidate.textContent === "Save all" || candidate.textContent === "Save",
  );
  await act(async () => button!.click());
}

describe("UnsavedChangesDialog for a closing window", () => {
  it("closes the window once the files are saved", async () => {
    await askOnClosingWindow();
    await save();
    await act(async () => answerSave(true));
    expect(editor.closeWindowNow).toHaveBeenCalledTimes(1);
  });

  it("leaves the window open when it is dismissed while the files save", async () => {
    await askOnClosingWindow();
    await save();
    act(() => root!.unmount());
    root = null;
    await act(async () => answerSave(true));
    expect(editor.closeWindowNow).not.toHaveBeenCalled();
  });

  it("leaves the window open when a file could not be saved", async () => {
    await askOnClosingWindow();
    await save();
    await act(async () => answerSave(false));
    expect(editor.closeWindowNow).not.toHaveBeenCalled();
  });
});
