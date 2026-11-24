// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { EMPTY_DRAFT } from "./lib/connectDraft";
import { editorTab, findTab } from "./lib/layout";
import type { SessionInfo } from "./lib/types";
import { askToSave, closeEditorTab } from "./state/editorActions";
import { useLayoutStore } from "./state/layoutStore";
import { useUiStore } from "./state/uiStore";
import { AppDialog } from "./App";

// Modules that work across windows ask, as they load, which window this page is.
vi.hoisted(() => {
  Object.assign(globalThis, {
    __TAURI_INTERNALS__: { metadata: { currentWindow: { label: "main" } } },
  });
});
const editor = vi.hoisted(() => ({ saveAndClose: vi.fn(), closeWindowNow: vi.fn() }));
vi.mock("./state/editorActions", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./state/editorActions")>()),
  ...editor,
}));
const connecting = vi.hoisted(() => ({ connectInTab: vi.fn() }));
vi.mock("./state/connectActions", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./state/connectActions")>()),
  ...connecting,
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve(null)),
  Channel: class {},
}));

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
/** Answers the work the dialog on show started, which waits until then. */
let answerSave: (saved: boolean) => void;
let answerConnect: (result: { session: SessionInfo }) => void;
/** Editor tabs opened for a test, to close after it. */
let opened: string[] = [];

beforeEach(() => {
  vi.clearAllMocks();
  // As the real one, closing the tabs that saved.
  editor.saveAndClose.mockImplementation(
    (tabIds: string[]) =>
      new Promise<boolean>((resolve) => {
        answerSave = (saved) => {
          if (saved) tabIds.forEach(closeEditorTab);
          resolve(saved);
        };
      }),
  );
  connecting.connectInTab.mockImplementation(
    () =>
      new Promise((resolve) => {
        answerConnect = resolve;
      }),
  );
  useUiStore.setState({ dialog: null });
});

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  container.remove();
  act(() => opened.forEach(closeEditorTab));
  opened = [];
});

/** Opens a file with unsaved changes in an editor tab; its tab's id. */
function editing(name: string): string {
  const tab = editorTab({ id: `document-${name}`, name, path: `/srv/${name}`, origin: "Local" });
  act(() => useLayoutStore.getState().openTab(tab));
  opened.push(tab.id);
  return tab.id;
}

const isOpen = (tabId: string) => Boolean(findTab(useLayoutStore.getState().root, tabId));

async function host() {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root!.render(createElement(AppDialog)));
}

async function press(label: string | RegExp) {
  const button = [...container.querySelectorAll("button")].find((candidate) =>
    typeof label === "string"
      ? candidate.textContent === label || candidate.getAttribute("aria-label") === label
      : label.test(candidate.textContent ?? ""),
  );
  if (!button) throw new Error(`No button ${label}`);
  await act(async () => button.click());
}

const shown = () => useUiStore.getState().dialog;
const session = { id: "session-1" } as SessionInfo;
const server = {
  ...EMPTY_DRAFT,
  host: "web.example.com",
  username: "deploy",
  saveConnection: false,
};

describe("the unsaved changes question", () => {
  it("closes, and closes the window, when the window asked to close while the files saved", async () => {
    await host();
    const notes = editing("notes.txt");
    await act(async () => askToSave([notes]));
    await press(/^Save/);
    await act(async () => askToSave([notes], true));
    await act(async () => answerSave(true));
    expect(isOpen(notes)).toBe(false);
    expect(shown()).toBeNull();
    expect(editor.closeWindowNow).toHaveBeenCalledTimes(1);
  });

  it("stays for a tab that joined while the files saved", async () => {
    await host();
    const notes = editing("notes.txt");
    const todo = editing("todo.txt");
    await act(async () => askToSave([notes]));
    await press(/^Save/);
    await act(async () => askToSave([notes, todo], true));
    await act(async () => answerSave(true));
    expect(shown()).toEqual({ kind: "unsavedChanges", tabIds: [notes, todo], closeWindow: true });
    expect(container.querySelector("dialog[open] h2")?.textContent).toBe(
      "Save changes to todo.txt?",
    );
    expect(editor.closeWindowNow).not.toHaveBeenCalled();
  });

  it("leaves alone a dialog opened after it was dismissed", async () => {
    await host();
    const notes = editing("notes.txt");
    await act(async () => askToSave([notes], true));
    await press(/^Save/);
    await press("Close");
    await act(async () => useUiStore.getState().open({ kind: "saveConnection", sessionId: "x" }));
    await act(async () => answerSave(true));
    expect(shown()).toEqual({ kind: "saveConnection", sessionId: "x" });
    expect(editor.closeWindowNow).not.toHaveBeenCalled();
  });

  it("closes the window asked for once a save from a question dismissed since closes its tab", async () => {
    await host();
    const notes = editing("notes.txt");
    await act(async () => askToSave([notes]));
    await press(/^Save/);
    // Escape while the file saves, then the window's close button.
    await press("Close");
    await act(async () => askToSave([notes], true));
    expect(shown()).toEqual({ kind: "unsavedChanges", tabIds: [notes], closeWindow: true });
    await act(async () => answerSave(true));
    expect(shown()).toBeNull();
    expect(editor.closeWindowNow).toHaveBeenCalledTimes(1);
  });

  it("closes, and leaves the window open, once that save closes the tab it was asked about", async () => {
    await host();
    const notes = editing("notes.txt");
    await act(async () => askToSave([notes]));
    await press(/^Save/);
    await press("Close");
    // The tab is closed again while it saves.
    await act(async () => askToSave([notes]));
    await act(async () => answerSave(true));
    expect(shown()).toBeNull();
    expect(editor.closeWindowNow).not.toHaveBeenCalled();
  });
});

describe("Connect", () => {
  it("closes once connected, though Connect was asked for again meanwhile", async () => {
    await host();
    await act(async () => useUiStore.getState().open({ kind: "connect", draft: server }));
    await press("Connect");
    expect(connecting.connectInTab).toHaveBeenCalledTimes(1);
    // A saved connection opening in the background needs a password.
    const again = { kind: "connect" as const, draft: server, error: "Authentication failed" };
    await act(async () => useUiStore.getState().open(again));
    await act(async () => answerConnect({ session }));
    expect(shown()).toBeNull();
  });

  it("leaves alone a dialog opened after it was dismissed", async () => {
    await host();
    await act(async () => useUiStore.getState().open({ kind: "connect", draft: server }));
    await press("Connect");
    await press("Close");
    await act(async () => useUiStore.getState().open({ kind: "saveConnection", sessionId: "x" }));
    await act(async () => answerConnect({ session }));
    expect(shown()).toEqual({ kind: "saveConnection", sessionId: "x" });
  });
});
