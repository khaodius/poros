// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FileSource } from "../lib/fileSource";
import type { FileEntry } from "../lib/types";
import { usePane, type PaneController } from "./usePane";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const FOLDER = "/srv/incoming";

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

/** A folder that lists `names` at once, as they are when it is asked. */
function folderOf(names: string[]): FileSource {
  const unused = async () => {
    throw new Error("Not used by the pane");
  };
  return {
    key: "local",
    kind: "local",
    label: "Local",
    pathStyle: "posix",
    initialPath: async () => FOLDER,
    home: async () => FOLDER,
    list: async (path) => ({ path, parent: "/srv", entries: names.map(file) }),
    mkdir: unused,
    makeFile: unused,
    rename: unused,
    remove: unused,
  };
}

let root: Root | null = null;
let latest: PaneController | null = null;

afterEach(() => {
  act(() => root?.unmount());
  root = null;
  latest = null;
});

async function paneOn(source: FileSource) {
  function Probe() {
    latest = usePane(source, {});
    return null;
  }
  root = createRoot(document.createElement("div"));
  await act(async () => root!.render(createElement(Probe)));
  return {
    get pane() {
      return latest!;
    },
    /** Runs `step` on the pane as the user would, and lets it render. */
    act: (step: (pane: PaneController) => unknown) => act(async () => void step(latest!)),
  };
}

const selected = (pane: PaneController) =>
  pane.visibleEntries.filter((entry) => pane.selection.has(entry.path)).map((entry) => entry.name);

describe("Shift+click", () => {
  it("selects from the row clicked before after the folder reloads", async () => {
    const names = ["a.txt", "b.txt", "c.txt", "d.txt"];
    const view = await paneOn(folderOf(names));
    await view.act((pane) => pane.selectOnly(`${FOLDER}/b.txt`));
    names.push("arrived.txt");
    await view.act((pane) => pane.refresh());
    await view.act((pane) => pane.selectRangeTo(`${FOLDER}/d.txt`));
    expect(selected(view.pane)).toEqual(["b.txt", "c.txt", "d.txt"]);
  });

  it("selects from the cursor after Esc cleared the selection", async () => {
    const view = await paneOn(folderOf(["a.txt", "b.txt", "c.txt", "d.txt"]));
    await view.act((pane) => pane.selectOnly(`${FOLDER}/b.txt`));
    await view.act((pane) => pane.clearSelection());
    await view.act((pane) => pane.selectRangeTo(`${FOLDER}/d.txt`));
    expect(selected(view.pane)).toEqual(["b.txt", "c.txt", "d.txt"]);
  });

  it("selects from the cursor when the filter hides the row clicked before", async () => {
    const view = await paneOn(
      folderOf(["a-notes.txt", "b-draft.txt", "c-notes.txt", "d-notes.txt"]),
    );
    await view.act((pane) => pane.selectOnly(`${FOLDER}/b-draft.txt`));
    await view.act((pane) => pane.stepCursor(1));
    await view.act((pane) => pane.setFilter("notes"));
    await view.act((pane) => pane.selectRangeTo(`${FOLDER}/d-notes.txt`));
    expect(selected(view.pane)).toEqual(["c-notes.txt", "d-notes.txt"]);
  });

  it("with Ctrl adds from the cursor when the filter hides the row clicked before", async () => {
    const view = await paneOn(
      folderOf(["a-notes.txt", "b-draft.txt", "c-notes.txt", "d-notes.txt"]),
    );
    await view.act((pane) => pane.selectOnly(`${FOLDER}/b-draft.txt`));
    await view.act((pane) => pane.stepCursor(1));
    await view.act((pane) => pane.setFilter("notes"));
    await view.act((pane) => pane.addRangeTo(`${FOLDER}/d-notes.txt`));
    expect(selected(view.pane)).toEqual(["c-notes.txt", "d-notes.txt"]);
  });
});

describe("Shift+arrow", () => {
  it("extends from the cursor when the filter hides the row clicked before", async () => {
    const view = await paneOn(
      folderOf(["a-notes.txt", "b-draft.txt", "c-notes.txt", "d-notes.txt"]),
    );
    await view.act((pane) => pane.selectOnly(`${FOLDER}/b-draft.txt`));
    await view.act((pane) => pane.stepCursor(1));
    await view.act((pane) => pane.setFilter("notes"));
    await view.act((pane) => pane.moveCursor(1, true));
    expect(selected(view.pane)).toEqual(["c-notes.txt", "d-notes.txt"]);
  });
});
