// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PaneController } from "../hooks/usePane";
import type { FileSource } from "../lib/fileSource";
import type { FileEntry, SessionInfo } from "../lib/types";
import { useSessionStore } from "../state/sessionStore";
import { PanePreview } from "./PanePreview";
import type { PreviewPane } from "./PreviewPane";

type Shown = Parameters<typeof PreviewPane>[0];

const shown = vi.hoisted(() => [] as Shown[]);

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
vi.mock("./PreviewPane", () => ({
  PreviewPane: (props: Shown) => {
    shown.push(props);
    return null;
  },
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SESSION = "session-1";

function file(path: string, size = 2048): FileEntry {
  return {
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind: "file",
    size,
    modified: 1_700_000_000,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

/** A pane as `usePane` hands it over, with only what the preview looks at. */
function paneShowing(
  entries: FileEntry[],
  cursorPath: string | null,
  change: Partial<PaneController> = {},
): PaneController {
  return {
    visibleEntries: entries,
    cursorPath,
    selectedEntries: () => [],
    stale: null,
    showingResults: false,
    refresh: vi.fn(async () => true),
    ...change,
  } as unknown as PaneController;
}

const source = { kind: "remote", key: `remote:${SESSION}` } as unknown as FileSource;

let root: Root;

async function render(pane: PaneController) {
  await act(async () => {
    root.render(
      createElement(PanePreview, {
        pane,
        source,
        sessionId: SESSION,
        dateFormat: "minutes",
        onOpenInEditor: () => undefined,
        onSwitchPane: () => undefined,
        onLeave: () => undefined,
      }),
    );
  });
  return shown[shown.length - 1];
}

beforeEach(() => {
  shown.length = 0;
  root = createRoot(document.createElement("div"));
  const info = { id: SESSION, protocol: "ftp" } as unknown as SessionInfo;
  useSessionStore.setState({ sessions: { [SESSION]: { info, profile: null } as never } });
});

afterEach(() => {
  act(() => root.unmount());
  useSessionStore.setState({ sessions: {} });
});

describe("PanePreview", () => {
  it("previews the search result under the cursor by its own path", async () => {
    const results = [
      file("/srv/www/index.html"),
      file("/srv/www/assets/img/logo.png"),
      file("/srv/backup/2024/logo.png"),
    ];
    const pane = paneShowing(results, "/srv/www/assets/img/logo.png", { showingResults: true });
    const props = await render(pane);
    expect(props.subject).toEqual({ kind: "file", entry: results[1] });
    // Sizes as the search found them on the server, not a remembered listing's.
    expect(props.place).toMatchObject({ sessionId: SESSION, sizes: "listed" });

    // Refresh is the pane's own, which on results searches again.
    props.onRefresh();
    expect(pane.refresh).toHaveBeenCalledTimes(1);
  });

  it("previews results selected together from several folders", async () => {
    const results = [file("/srv/www/a.png"), file("/srv/backup/b.png")];
    const pane = paneShowing(results, results[0].path, {
      showingResults: true,
      selectedEntries: () => results,
    });
    const props = await render(pane);
    expect(props.subject).toEqual({ kind: "selection", entries: results });
  });

  it("previews only rows on show, as when rows identical in both panes are hidden", async () => {
    const shownRow = file("/srv/www/changed.css");
    const hidden = file("/srv/www/same.css");
    // The pane leaves hidden rows out of what it shows and of what is selected.
    const pane = paneShowing([shownRow], hidden.path);
    expect((await render(pane)).subject).toEqual({ kind: "nothing" });
    const selected = paneShowing([shownRow], hidden.path, { selectedEntries: () => [shownRow] });
    expect((await render(selected)).subject).toEqual({ kind: "file", entry: shownRow });
  });

  it("trusts nothing a remembered listing says, but results are never remembered", async () => {
    const entries = [file("/srv/www/logo.png")];
    const remembered = await render(paneShowing(entries, entries[0].path, { stale: "outdated" }));
    expect(remembered.place.sizes).toBe("remembered");
    const results = await render(
      paneShowing(entries, entries[0].path, {
        stale: "outdated",
        showingResults: true,
      }),
    );
    expect(results.place.sizes).toBe("listed");
  });
});
