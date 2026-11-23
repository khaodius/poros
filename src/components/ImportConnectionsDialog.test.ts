// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { ImportScan, ImportSourceInfo } from "../lib/types";
import { ImportConnectionsDialog } from "./ImportConnectionsDialog";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

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

const fileZilla: ImportSourceInfo = {
  source: "fileZilla",
  found: ["/home/poros/.config/filezilla/sitemanager.xml"],
  places: [],
};

const scan: ImportScan = {
  scanId: "scan-1",
  source: "fileZilla",
  origins: ["sitemanager.xml"],
  entries: [],
  skipped: [{ name: "Old server", reason: "Unsupported protocol" }],
  warnings: [],
};

let root: Root | null = null;
let container: HTMLElement;
/** Answers the scan the dialog started, which waits until then. */
let answerScan: (scan: ImportScan) => void;

beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((command: string) => {
    if (command === "connections_import_sources") return Promise.resolve([fileZilla]);
    if (command === "connections_import_scan") {
      return new Promise((resolve) => {
        answerScan = resolve;
      });
    }
    return Promise.resolve(null);
  });
});

afterEach(() => {
  act(() => root?.unmount());
  root = null;
});

async function open() {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () =>
    root!.render(createElement(ImportConnectionsDialog, { onClose: () => {} })),
  );
}

async function readFileZilla() {
  const button = [...container.querySelectorAll("button")].find(
    (candidate) => candidate.textContent === "Import",
  );
  await act(async () => button!.click());
}

const discarded = () =>
  invoke.mock.calls
    .filter(([command]) => command === "connections_import_discard")
    .map(([, args]) => (args as { scanId: string }).scanId);

describe("ImportConnectionsDialog", () => {
  it("forgets a scan, with its passwords, that is read after the dialog closed", async () => {
    await open();
    await readFileZilla();
    act(() => root!.unmount());
    root = null;
    await act(async () => answerScan(scan));
    expect(discarded()).toEqual(["scan-1"]);
  });

  it("forgets the scan it holds as it closes", async () => {
    await open();
    await readFileZilla();
    await act(async () => answerScan(scan));
    expect(discarded()).toEqual([]);
    act(() => root!.unmount());
    root = null;
    expect(discarded()).toEqual(["scan-1"]);
  });
});
