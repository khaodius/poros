// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileSource } from "../lib/fileSource";
import type { DeletedEntry, DirListing, FileEntry, SessionInfo } from "../lib/types";
import { deletionsIn } from "../lib/versions";
import { listingShare, paneListings } from "../state/listings";
import { useSessionStore } from "../state/sessionStore";
import { useVersioningStore } from "../state/versionActions";
import { usePane, type PaneController } from "./usePane";
import { usePaneVersions, type PaneVersions } from "./usePaneVersions";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SCOPE = '["server","s3","s3.example.com",443,"key",null]';
const FOLDER = "/bucket/reports";
const cache = listingShare.cache;

function entry(name: string, kind: FileEntry["kind"] = "file"): FileEntry {
  return {
    name,
    path: `${FOLDER}/${name}`,
    kind,
    size: kind === "dir" ? 0 : 10,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

/** What the bucket holds, and what was deleted from it, as the test changes them. */
const bucket: { live: FileEntry[]; deleted: DeletedEntry[] } = { live: [], deleted: [] };
const listed = (): DirListing => ({ path: FOLDER, parent: "/bucket", entries: [...bucket.live] });

const source: FileSource = {
  key: "remote:s1",
  kind: "remote",
  label: "s3.example.com",
  pathStyle: "posix",
  listingScope: SCOPE,
  initialPath: async () => FOLDER,
  home: async () => "/",
  list: async () => listed(),
  mkdir: async () => "",
  makeFile: async () => "",
  rename: async () => "",
  remove: async () => undefined,
};

let pane: PaneController | null = null;
let shown: PaneVersions | null = null;
let unmount: (() => void) | null = null;

async function mount(): Promise<void> {
  function Probe() {
    shown = usePaneVersions(source, "s1");
    pane = usePane(source, { handover: shown.deletedView, listings: paneListings });
    return null;
  }
  const root = createRoot(document.createElement("div"));
  await act(async () => root.render(createElement(Probe)));
  unmount = () => act(() => root.unmount());
}

async function showDeleted(shownNow: boolean): Promise<void> {
  await act(async () => {
    shown!.setShowDeleted(shownNow);
    await pane!.refresh();
  });
}

const names = () => pane!.listing?.entries.map((item) => item.name).sort();
const marked = () => [...deletionsIn(pane!.listing).keys()];

beforeEach(() => {
  cache.clear();
  bucket.live = [entry("a.txt"), entry("docs", "dir")];
  bucket.deleted = [
    { name: "old", path: `${FOLDER}/old`, isDir: true, deleted: null, size: 0 },
    { name: "b.txt", path: `${FOLDER}/b.txt`, isDir: false, deleted: 1_700_000_000, size: 4 },
  ];
  const info = { id: "s1", label: "s3", protocol: "s3" } as SessionInfo;
  useSessionStore.setState({ sessions: { s1: { info, profile: null, status: "connected" } } });
  useVersioningStore.setState({ known: { "s1\nbucket": "enabled" } });
  invoke.mockImplementation(async (command: string) => {
    if (command === "versions_list_deleted") return { listing: listed(), deleted: bucket.deleted };
    throw { kind: "unsupported", message: `${command} is not part of this test` };
  });
});

afterEach(() => {
  unmount?.();
  unmount = null;
  invoke.mockReset();
});

describe("deleted files in a pane", () => {
  it("are listed only while shown, and never remembered as the folder's listing", async () => {
    await mount();
    expect(names()).toEqual(["a.txt", "docs"]);
    expect(invoke).not.toHaveBeenCalled();

    await showDeleted(true);
    expect(names()).toEqual(["a.txt", "b.txt", "docs", "old"]);
    expect(marked().sort()).toEqual([`${FOLDER}/b.txt`, `${FOLDER}/old`]);
    expect(cache.get(SCOPE, FOLDER)?.entries.map((item) => item.name)).toEqual(["a.txt", "docs"]);

    await showDeleted(false);
    expect(names()).toEqual(["a.txt", "docs"]);
    expect(marked()).toEqual([]);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("stop being marked once brought back, though the row looks the same", async () => {
    await mount();
    await showDeleted(true);
    // The folder comes back as the folder it was: nothing in its row tells the two apart.
    bucket.live = [...bucket.live, entry("old", "dir")];
    bucket.deleted = bucket.deleted.filter((item) => item.name !== "old");
    await act(async () => void (await pane!.refresh()));
    expect(names()).toEqual(["a.txt", "b.txt", "docs", "old"]);
    expect(marked()).toEqual([`${FOLDER}/b.txt`]);
  });
});
