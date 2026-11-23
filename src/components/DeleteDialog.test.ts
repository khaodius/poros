// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { DirListing, FileEntry } from "../lib/types";
import { DeleteDialog } from "./DeleteDialog";

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

function file(path: string): FileEntry {
  return {
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind: "file",
    size: 1,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

function listing(names: string[]): DirListing {
  return { path: "/srv", parent: "/", entries: names.map((name) => file(`/srv/${name}`)) };
}

let root: Root | null = null;
let container: HTMLElement;

afterEach(() => {
  act(() => root?.unmount());
  root = null;
});

const chosen = [file("/srv/a.txt"), file("/srv/b.txt")];
const confirmed: FileEntry[][] = [];

async function show(shown: DirListing | null, checking: boolean) {
  if (!root) {
    container = document.createElement("div");
    root = createRoot(container);
  }
  await act(async () =>
    root!.render(
      createElement(DeleteDialog, {
        chosen,
        listing: shown,
        checking,
        summary: (entries) => entries.map((entry) => entry.name).join(", "),
        onConfirm: async (entries) => {
          confirmed.push(entries);
        },
        onClose: () => undefined,
      }),
    ),
  );
}

const title = () => container.querySelector("h2")?.textContent;
const confirmButton = () => container.querySelector<HTMLButtonElement>("[data-autofocus]")!;
const body = () => container.querySelector(".confirm-body")?.textContent;

describe("DeleteDialog", () => {
  it("waits for the server while the listing is being checked, then deletes what is left", async () => {
    await show(listing(["a.txt", "b.txt"]), true);
    expect(confirmButton().disabled).toBe(true);
    expect(confirmButton().textContent).toBe("Checking the folder...");

    // b.txt went away on the server before the refresh landed.
    await show(listing(["a.txt", "c.txt"]), false);
    expect(title()).toBe("Delete item");
    expect(body()).toBe("a.txt");
    expect(confirmButton().disabled).toBe(false);
    expect(confirmButton().textContent).toBe("Delete");
    await act(async () => confirmButton().click());
    expect(confirmed[confirmed.length - 1].map((entry) => entry.path)).toEqual(["/srv/a.txt"]);
  });

  it("deletes nothing when none of the items is listed any more", async () => {
    await show(listing(["c.txt"]), false);
    expect(title()).toBe("Nothing to delete");
    expect(body()).toBe("These items are no longer in the folder.");
    expect(confirmButton().disabled).toBe(true);
  });
});
