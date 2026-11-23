// @vitest-environment jsdom
import { act, createElement, type ReactElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { usePane, type PaneController } from "../hooks/usePane";
import type { FileSource } from "../lib/fileSource";
import type { DirListing, FileEntry } from "../lib/types";
import { FileList, RenameField, type FileListActions } from "./FileList";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const FOLDER = "/srv/incoming";
const ROW_HEIGHT = 24;
const VIEW_HEIGHT = 240;

function file(name: string): FileEntry {
  return {
    name,
    path: `${FOLDER}/${name}`,
    kind: "file",
    size: 1,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: name.startsWith("."),
  };
}

/** Scroll positions the list was sent to, as a row brought into view sends it. */
let scrolledTo: number[] = [];

beforeAll(() => {
  // jsdom lays nothing out: the list is given a height, and scrolling is recorded.
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get() {
      return (this as HTMLElement).classList.contains("file-list-body") ? VIEW_HEIGHT : 0;
    },
  });
  HTMLElement.prototype.scrollTo = function scrollTo(
    this: HTMLElement,
    options?: ScrollToOptions | number,
  ) {
    const top = typeof options === "object" ? options.top : undefined;
    if (top === undefined) return;
    scrolledTo.push(top);
    this.scrollTop = top;
  } as HTMLElement["scrollTo"];
});

let root: Root | null = null;
let container: HTMLElement;

beforeEach(() => {
  scrolledTo = [];
});

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  container.remove();
  vi.useRealTimers();
});

const field = () => container.querySelector<HTMLInputElement>(".rename-field")!;

/** Types `text` in place of what is selected in the field, as a key press does. */
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

async function render(element: ReactElement) {
  if (!root) {
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  }
  await act(async () => root!.render(element));
}

describe("RenameField", () => {
  const report = file("report.txt");
  const events = {
    onPointerDown: () => {},
    onClick: () => {},
    onOpen: () => {},
    onContextMenu: () => {},
    onRenameSubmit: async () => true,
    onRenameCancel: () => {},
  };
  const show = (entry: FileEntry) => render(createElement(RenameField, { entry, events }));

  it("selects the name before the extension to start with", async () => {
    await show(report);
    expect([field().selectionStart, field().selectionEnd]).toEqual([0, "report".length]);
  });

  it("keeps what was typed when a reload hands it a new entry for the same file", async () => {
    await show(report);
    await type("summary");
    expect(field().value).toBe("summary.txt");
    await show({ ...report });
    await type("-2026");
    expect(field().value).toBe("summary-2026.txt");
  });
});

describe("FileList", () => {
  const names: string[] = [];
  const source: FileSource = {
    key: "local",
    kind: "local",
    label: "Local",
    pathStyle: "posix",
    initialPath: async () => FOLDER,
    home: async () => FOLDER,
    list: async (path) => ({ path, parent: "/srv", entries: names.map(file) }),
    mkdir: async () => "",
    makeFile: async () => "",
    rename: async () => "",
    remove: async () => {},
  };
  const onRename = vi.fn();
  const onRenameSeveral = vi.fn();
  const ignored = () => {};
  const actions: FileListActions = {
    onActivate: ignored,
    onContextMenu: ignored,
    onHeaderContextMenu: ignored,
    onDelete: ignored,
    onRename,
    onRenameSeveral,
    onRenameSubmit: async () => true,
    onRenameCancel: ignored,
    onEdit: ignored,
    onNewFolder: ignored,
    onNewFile: ignored,
    onEditPath: ignored,
    onFocusFilter: ignored,
    onSwitchPane: ignored,
    onRowPointerDown: ignored,
    onFileActivate: ignored,
    onCut: ignored,
    onCopy: ignored,
    onPaste: ignored,
    onProperties: ignored,
    onCompareFolders: ignored,
  };
  const noRows: ReadonlySet<string> = new Set();
  let pane: PaneController;

  function Listed({ renaming, results }: { renaming: string | null; results: DirListing | null }) {
    pane = usePane(source, { showHiddenByDefault: true, results });
    return createElement(FileList, {
      ...actions,
      pane,
      side: { kind: "local" },
      active: true,
      rowHeight: ROW_HEIGHT,
      hiddenColumns: [],
      columnWidths: {},
      onColumnWidthsChange: () => {},
      dateFormat: "minutes",
      striped: false,
      fontSize: 13,
      renaming,
      heldPaths: noRows,
    });
  }

  const list = (renaming: string | null = null, results: DirListing | null = null) =>
    render(createElement(Listed, { renaming, results }));
  /** Search results found so far in the folder, each a new entry as a search hands them over. */
  const found = (resultNames: string[]): DirListing => ({
    path: FOLDER,
    parent: null,
    entries: resultNames.map(file),
  });
  /** The pane showing `results`, a step in its history as a search makes them. */
  async function showResults(results: DirListing, renaming: string | null = null) {
    await list(renaming, results);
    if (!pane.showingResults) await onPane((pane) => pane.showResults(FOLDER));
    expect(pane.showingResults).toBe(true);
  }
  const body = () => container.querySelector<HTMLElement>(".file-list-body")!;
  const nameOf = (name: string) =>
    container.querySelector(`.file-row[data-path="${FOLDER}/${name}"] .file-name-text`)!;
  const onPane = (step: (pane: PaneController) => unknown) => act(async () => void step(pane));

  /** A list of `a.txt` and the hidden `.env`, both selected, with hidden files then hidden. */
  async function selectedWithAHiddenFile() {
    names.splice(0, Infinity, ".env", "a.txt", "b.txt");
    await list();
    await onPane((pane) => pane.selectOnly(`${FOLDER}/.env`));
    await onPane((pane) => pane.toggleSelected(`${FOLDER}/a.txt`));
    await onPane((pane) => pane.toggleHidden());
    expect(pane.selection.size).toBe(2);
    await act(async () => body().focus());
  }

  async function pressOnName(name: string) {
    const at = { bubbles: true, button: 0, clientX: 10, clientY: 10 };
    await act(async () => {
      nameOf(name).dispatchEvent(new PointerEvent("pointerdown", { ...at, pointerType: "mouse" }));
      nameOf(name).dispatchEvent(new MouseEvent("click", { ...at, detail: 1 }));
    });
  }

  beforeEach(() => {
    onRename.mockReset();
    onRenameSeveral.mockReset();
  });

  it("renames with F2 the one selected row on show, though a hidden one is selected too", async () => {
    await selectedWithAHiddenFile();
    await act(async () => {
      body().dispatchEvent(new KeyboardEvent("keydown", { key: "F2", bubbles: true }));
    });
    expect(onRename).toHaveBeenCalledWith(expect.objectContaining({ name: "a.txt" }));
    expect(onRenameSeveral).not.toHaveBeenCalled();
  });

  it("renames with F2 several selected rows on show together", async () => {
    await selectedWithAHiddenFile();
    await onPane((pane) => pane.toggleSelected(`${FOLDER}/b.txt`));
    await act(async () => {
      body().dispatchEvent(new KeyboardEvent("keydown", { key: "F2", bubbles: true }));
    });
    expect(onRenameSeveral).toHaveBeenCalledTimes(1);
    expect(onRename).not.toHaveBeenCalled();
  });

  it("renames on a slow click the one selected row on show, though a hidden one is selected too", async () => {
    await selectedWithAHiddenFile();
    vi.useFakeTimers();
    await pressOnName("a.txt");
    // A hidden row joins the selection before the click is known to be a slow one.
    await onPane((pane) => pane.selectPaths(new Set([`${FOLDER}/.env`, `${FOLDER}/a.txt`])));
    await act(async () => vi.runAllTimers());
    expect(onRename).toHaveBeenCalledWith(expect.objectContaining({ name: "a.txt" }));
  });

  it("brings the row being renamed into view as renaming starts, and not again on a reload", async () => {
    names.splice(
      0,
      Infinity,
      ...Array.from({ length: 60 }, (_, index) => `file-${index + 10}.txt`),
    );
    await list();
    scrolledTo = [];
    await list(`${FOLDER}/file-50.txt`);
    expect(scrolledTo.length).toBeGreaterThan(0);
    const scrolledAtStart = scrolledTo.length;
    // Files arrive above it, which moves its row down.
    names.unshift("file-00.txt", "file-01.txt");
    await onPane((pane) => pane.refresh());
    expect(pane.visibleEntries.findIndex((entry) => entry.name === "file-50.txt")).toBe(42);
    expect(scrolledTo.length).toBe(scrolledAtStart);
  });

  describe("showing search results", () => {
    const manyResults = Array.from({ length: 60 }, (_, index) => `found-${index + 10}.txt`);

    it("keeps what is typed in a result's rename field as more results arrive", async () => {
      names.splice(0, Infinity, "notes.txt");
      await list();
      await showResults(found(manyResults));
      await list(`${FOLDER}/found-30.txt`, found(manyResults));
      await type("summary");
      await list(`${FOLDER}/found-30.txt`, found(["found-00.txt", ...manyResults]));
      await type("-2026");
      expect(field().value).toBe("summary-2026.txt");
    });

    it("brings a result being renamed into view once, not again as results arrive above it", async () => {
      names.splice(0, Infinity, "notes.txt");
      await list();
      await showResults(found(manyResults));
      scrolledTo = [];
      await list(`${FOLDER}/found-50.txt`, found(manyResults));
      expect(scrolledTo.length).toBeGreaterThan(0);
      const scrolledAtStart = scrolledTo.length;
      await list(`${FOLDER}/found-50.txt`, found(["found-00.txt", "found-01.txt", ...manyResults]));
      expect(pane.visibleEntries.findIndex((entry) => entry.name === "found-50.txt")).toBe(42);
      expect(scrolledTo.length).toBe(scrolledAtStart);
    });

    it("renames with F2 the one selected result on show, though the filter hides another", async () => {
      names.splice(0, Infinity, "notes.txt");
      await list();
      await showResults(found(["a-notes.txt", "b-draft.txt", "c-notes.txt"]));
      await onPane((pane) => pane.selectOnly(`${FOLDER}/a-notes.txt`));
      await onPane((pane) => pane.toggleSelected(`${FOLDER}/b-draft.txt`));
      await onPane((pane) => pane.setFilter("notes"));
      await act(async () => body().focus());
      await act(async () => {
        body().dispatchEvent(new KeyboardEvent("keydown", { key: "F2", bubbles: true }));
      });
      expect(onRename).toHaveBeenCalledWith(expect.objectContaining({ name: "a-notes.txt" }));
    });
  });
});
