// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { FileEntry, ShareCapabilities, ShareLink, ShareOptions } from "../lib/types";
import { ShareDialog } from "./ShareDialog";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
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

/** S3: signed links for files, which always end. */
const signed: ShareCapabilities = {
  kinds: ["view", "download"],
  audiences: ["anyone"],
  expiry: "required",
  maxLifetimeSecs: 604_800,
  password: false,
  revocable: false,
  folders: false,
  singleLink: false,
};

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

/** Answers the share commands the way the backend does for S3. */
function answer(command: string, args: Record<string, unknown>): unknown {
  switch (command) {
    case "share_links":
      return [];
    case "share_create": {
      const options = args.options as ShareOptions;
      const link: ShareLink = {
        id: "",
        url: `https://s3.example${String(args.path)}?X-Amz-Signature=1`,
        kind: options.kind,
        audience: "anyone",
        expiresAt: Math.floor(Date.now() / 1000) + (options.lifetimeSecs ?? 3600),
        hasPassword: false,
        revocable: false,
        notes: [],
      };
      return link;
    }
    default:
      throw new Error(`unexpected command ${command}`);
  }
}

let root: Root | null = null;
let container: HTMLElement;

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  container?.remove();
  invoke.mockReset();
});

async function show(entries: FileEntry[], folderOf?: (entry: FileEntry) => string) {
  invoke.mockImplementation(async (command: string, args: Record<string, unknown>) =>
    answer(command, args),
  );
  // Attached, since a form submits only from the document.
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  await act(async () =>
    root!.render(
      createElement(ShareDialog, {
        sessionId: "session-1",
        folder: "/poros-media",
        folderOf,
        entries,
        sharing: signed,
        service: "S3",
        onClose: () => undefined,
      }),
    ),
  );
}

const createButton = () => container.querySelector<HTMLButtonElement>("[data-autofocus]")!;
const createdPaths = () =>
  invoke.mock.calls
    .filter(([command]) => command === "share_create")
    .map(([, args]) => (args as { path: string }).path);

describe("ShareDialog", () => {
  it("makes each search result's link for its own path, and says which folder it is in", async () => {
    // Search results from /poros-media: the same name there and in two folders below it.
    const results = [
      file("/poros-media/reports/2025/q3.pdf"),
      file("/poros-media/archive/q3.pdf"),
      file("/poros-media/q3.pdf"),
    ];
    // As the search names the folders: relative to the one searched, `.` for that one.
    await show(results, (entry) => {
      const folder = entry.path.slice(0, entry.path.lastIndexOf("/"));
      return folder === "/poros-media" ? "." : folder.slice("/poros-media/".length);
    });
    const places = [...container.querySelectorAll(".share-batch-place")].map(
      (place) => place.textContent,
    );
    expect(places).toEqual(["reports/2025", "archive"]);
    const titles = [...container.querySelectorAll(".share-batch-name")].map((name) =>
      name.getAttribute("title"),
    );
    expect(titles).toEqual(results.map((result) => result.path));

    expect(createButton().textContent).toBe("Create 3 links");
    await act(async () => createButton().click());

    expect(createdPaths().sort()).toEqual([
      "/poros-media/archive/q3.pdf",
      "/poros-media/q3.pdf",
      "/poros-media/reports/2025/q3.pdf",
    ]);
    const addresses = [...container.querySelectorAll<HTMLInputElement>(".share-address")].map(
      (address) => address.value,
    );
    expect(addresses).toEqual(
      results.map((result) => `https://s3.example${result.path}?X-Amz-Signature=1`),
    );
    // Made as asked, so the statement above describes them and no row says otherwise.
    expect(container.querySelectorAll(".share-batch-label")).toHaveLength(0);
  });

  it("names no folder for items listed in one folder", async () => {
    await show([file("/poros-media/reports/a.pdf"), file("/poros-media/reports/b.pdf")]);
    expect(container.querySelectorAll(".share-batch-place")).toHaveLength(0);
    await act(async () => createButton().click());
    expect(createdPaths().sort()).toEqual([
      "/poros-media/reports/a.pdf",
      "/poros-media/reports/b.pdf",
    ]);
  });
});
