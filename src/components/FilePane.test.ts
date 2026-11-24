// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { FileSource } from "../lib/fileSource";
import type { FileEntry } from "../lib/types";
import { FilePane } from "./FilePane";

// Modules that work across windows ask, as they load, which window this page is.
vi.hoisted(() => {
  Object.assign(globalThis, {
    __TAURI_INTERNALS__: { metadata: { currentWindow: { label: "main" } } },
  });
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => Promise.resolve(null)),
  Channel: class {},
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const FOLDER = "/srv/site";

beforeAll(() => {
  // The page's dialogs open as modals, which jsdom does not draw, and nothing is laid out.
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
  HTMLElement.prototype.scrollTo ??= function scrollTo() {};
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});

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
    hidden: false,
  };
}

/** Answers the folder the pane made, which waits until then. */
let answerMkdir: (path: string) => void;

const source: FileSource = {
  key: "local",
  kind: "local",
  label: "Local",
  pathStyle: "posix",
  initialPath: async () => FOLDER,
  home: async () => FOLDER,
  list: async (path) => ({ path, parent: "/srv", entries: [file("index.html")] }),
  mkdir: () =>
    new Promise((resolve) => {
      answerMkdir = resolve;
    }),
  makeFile: async () => "",
  rename: async () => "",
  remove: async () => {},
};

let root: Root | null = null;
let container: HTMLElement;

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  container.remove();
});

async function showPane() {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () =>
    root!.render(
      createElement(FilePane, {
        tabId: "tab-1",
        title: "Local",
        icon: null,
        source,
        visible: true,
        active: true,
      }),
    ),
  );
}

async function press(label: string) {
  const button = [...container.querySelectorAll("button")].find(
    (candidate) =>
      candidate.getAttribute("aria-label") === label || candidate.textContent === label,
  );
  if (!button) throw new Error(`No button ${label}`);
  await act(async () => button.click());
}

const openDialogs = () =>
  [...container.querySelectorAll("dialog[open] h2")].map((title) => title.textContent);

describe("FilePane", () => {
  it("leaves alone a dialog opened after the one that finished its work was dismissed", async () => {
    await showPane();
    await press("New folder (F7)");
    expect(openDialogs()).toEqual(["New folder"]);
    const name = container.querySelector<HTMLInputElement>("dialog[open] input")!;
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    await act(async () => {
      setValue.call(name, "assets");
      name.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await press("Create");
    // Dismissed while the folder is being made, and another question asked.
    await press("Close");
    await press("New file (Shift+F4)");
    expect(openDialogs()).toEqual(["New file"]);
    await act(async () => answerMkdir(`${FOLDER}/assets`));
    expect(openDialogs()).toEqual(["New file"]);
  });
});
