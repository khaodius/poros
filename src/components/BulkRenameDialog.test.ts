// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { Staleness } from "../hooks/usePane";
import { DEFAULT_RULES, formatDateToken, type RenameRules } from "../lib/bulkRename";
import type { FileOrigin } from "../lib/fileOrigin";
import { DEFAULT_SETTINGS } from "../lib/settings";
import type { DirListing, FileEntry, RenameRequest } from "../lib/types";
import { useBulkRenameStore } from "../state/bulkRename";
import { useSettingsStore } from "../state/settingsStore";
import { BulkRenameDialog } from "./BulkRenameDialog";

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

const ORIGIN: FileOrigin = {
  kind: "remote",
  sessionId: "s1",
  server: { protocol: "sftp", host: "web.example.com", port: 22, username: "deploy" },
};
const FOLDER = "/srv/photos";
/** When the items were modified as the remembered listing has it, and as the server says now. */
const BEFORE = Date.UTC(2025, 4, 5, 12) / 1000;
const NOW = Date.UTC(2026, 2, 4, 12) / 1000;

function file(name: string, modified = BEFORE): FileEntry {
  return {
    name,
    path: `${FOLDER}/${name}`,
    kind: "file",
    size: 1,
    modified,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

function listing(...entries: FileEntry[]): DirListing {
  return { path: FOLDER, parent: "/srv", entries };
}

const remembered = listing(file("a.jpg"), file("b.jpg"), file("c.jpg"));
const chosen = [file("a.jpg"), file("b.jpg")];

let root: Root | null = null;
let container: HTMLElement;
let retries = 0;
let renameRequests: RenameRequest[] = [];

beforeEach(() => {
  retries = 0;
  renameRequests = [];
  invoke.mockReset();
  invoke.mockImplementation((command: string, args: Record<string, unknown> = {}) => {
    if (command === "files_rename") {
      renameRequests.push(args.request as RenameRequest);
      return Promise.resolve({ renamed: [], cancelled: false });
    }
    if (command === "files_conflicts") return Promise.resolve([]);
    if (command === "settings_set") return Promise.resolve(args.value);
    return Promise.resolve(null);
  });
  useSettingsStore.setState({ settings: DEFAULT_SETTINGS });
});

afterEach(() => {
  act(() => root?.unmount());
  root = null;
});

function useRules(change: Partial<RenameRules>) {
  useBulkRenameStore.setState({ rules: { ...DEFAULT_RULES, ...change }, report: null });
}

/** Shows the dialog over `shown`, and lets the folder answer what names it takes. */
async function show(shown: DirListing | null, stale: Staleness) {
  if (!root) {
    container = document.createElement("div");
    root = createRoot(container);
  }
  await act(async () =>
    root!.render(
      createElement(BulkRenameDialog, {
        origin: ORIGIN,
        folder: FOLDER,
        chosen,
        listing: shown,
        stale,
        onRetry: () => void (retries += 1),
        onClose: () => undefined,
      }),
    ),
  );
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
}

const title = () => container.querySelector("h2")?.textContent;
const renameButton = () => container.querySelector<HTMLButtonElement>("button[type=submit]")!;
const footer = () =>
  [...container.querySelectorAll(".rename-footer p")].map((line) => line.textContent);
const submit = () =>
  act(async () => {
    container.querySelector("form")!.requestSubmit();
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

describe("BulkRenameDialog", () => {
  it("waits for the server while the listing is a remembered one, then renames", async () => {
    useRules({ extension: { change: "replace", replacement: "jpeg" } });
    await show(remembered, "refreshing");
    expect(renameButton().disabled).toBe(true);
    expect(renameButton().textContent).toBe("Checking the folder...");
    await submit();
    expect(renameRequests).toEqual([]);

    await show(listing(file("a.jpg"), file("b.jpg"), file("c.jpg")), null);
    expect(renameButton().disabled).toBe(false);
    expect(renameButton().textContent).toBe("Rename 2 items");
    await submit();
    expect(renameRequests.map((request) => request.groups)).toEqual([
      [
        { renames: [{ from: "a.jpg", to: "a.jpeg" }] },
        { renames: [{ from: "b.jpg", to: "b.jpeg" }] },
      ],
    ]);
  });

  it("leaves out an item the fresh listing no longer has, and dates the rest as it does", async () => {
    useRules({
      template: { ...DEFAULT_RULES.template, enabled: true, pattern: "{name}-{date}" },
    });
    await show(remembered, "refreshing");
    expect(title()).toBe("Rename 2 items");

    // b.jpg went away on the server, and a.jpg was written to, before the refresh landed.
    await show(listing(file("a.jpg", NOW), file("c.jpg")), null);
    expect(title()).toBe("Rename 1 item");
    expect(footer()).toEqual([
      "1 of 1 item will be renamed. 1 selected item is no longer in the folder and is left out.",
    ]);
    await submit();
    const dated = `a-${formatDateToken(NOW, "YYYY-MM-DD")}.jpg`;
    expect(renameRequests.map((request) => request.groups)).toEqual([
      [{ renames: [{ from: "a.jpg", to: dated }] }],
    ]);
  });

  it("counts a name taken on the server since as taken", async () => {
    useRules({ extension: { change: "replace", replacement: "jpeg" } });
    await show(remembered, "refreshing");
    expect(footer()).toEqual(["2 of 2 items will be renamed."]);

    await show(listing(file("a.jpg"), file("b.jpg"), file("c.jpg"), file("b.jpeg")), null);
    expect(footer()).toEqual(["1 name is already taken."]);
    expect(renameButton().disabled).toBe(true);
    await submit();
    expect(renameRequests).toEqual([]);
  });

  it("renames nothing when none of the items is listed any more", async () => {
    useRules({ extension: { change: "replace", replacement: "jpeg" } });
    await show(listing(file("c.jpg")), null);
    expect(title()).toBe("Nothing to rename");
    expect(footer()).toEqual(["These items are no longer in the folder."]);
    expect(renameButton().disabled).toBe(true);
  });

  it("renames nothing over a listing the server could not be asked to refresh", async () => {
    useRules({ extension: { change: "replace", replacement: "jpeg" } });
    await show(remembered, "outdated");
    expect(footer()).toEqual([
      "Could not refresh this folder. This listing may be out of date.Try again",
      "2 of 2 items will be renamed.",
    ]);
    expect(renameButton().disabled).toBe(true);
    expect(renameButton().textContent).toBe("Rename");
    await submit();
    expect(renameRequests).toEqual([]);

    await act(async () => container.querySelector<HTMLButtonElement>(".link-button")!.click());
    expect(retries).toBe(1);
    await show(remembered, null);
    expect(footer()).toEqual(["2 of 2 items will be renamed."]);
    expect(renameButton().disabled).toBe(false);
  });
});
