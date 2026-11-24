import { describe, expect, it } from "vitest";
import {
  addTab,
  allGroups,
  allTabs,
  dockTab,
  editorTab,
  group,
  groupOfTab,
  layoutPlacements,
  localTab,
  moveTab,
  normalize,
  openTab,
  parseLayout,
  isRestoring,
  persistableLayout,
  removeTab,
  resizeShares,
  terminalTab,
  updateTab,
  type CurrentState,
  type LayoutNode,
  type PaneTab,
  type SavedServer,
} from "./layout";
import type { VaultInfo } from "./types";

const remote = (id: string): PaneTab => ({ id, kind: "remote", sessionId: `session-${id}` });

const server = (label: string): SavedServer => ({
  label,
  protocol: "sftp",
  savedConnectionId: `saved-${label}`,
  profile: {
    protocol: "sftp",
    host: `${label}.example.com`,
    port: 22,
    username: "deploy",
    auth: { type: "password", password: "" },
  },
});

/**
 * A window whose panes show `folders`, and whose sessions are connected to `servers` or have
 * unlocked `vaults`.
 */
function current(
  folders: Record<string, string> = {},
  servers: Record<string, SavedServer> = {},
  vaults: Record<string, VaultInfo> = {},
): CurrentState {
  return {
    folderOf: (tabId) => folders[tabId] ?? null,
    serverOf: (sessionId) => servers[sessionId] ?? null,
    vaultOf: (sessionId) => vaults[sessionId] ?? null,
  };
}

const throughStorage = (root: LayoutNode, state: CurrentState) =>
  parseLayout(JSON.parse(JSON.stringify(persistableLayout(root, state))));

function twoGroups(): { root: LayoutNode; left: string; right: string } {
  const leftGroup = group([{ id: "a", kind: "local" }]);
  const rightGroup = group([remote("b"), remote("c")]);
  return {
    root: {
      type: "split",
      id: "root",
      direction: "row",
      children: [leftGroup, rightGroup],
      sizes: [0.5, 0.5],
    },
    left: leftGroup.id,
    right: rightGroup.id,
  };
}

describe("layout", () => {
  it("moves a tab into another group and drops the emptied group", () => {
    const { root, right } = twoGroups();
    const moved = moveTab(root, "a", right, 1);
    expect(moved.type).toBe("group");
    expect(allTabs(moved).map((tab) => tab.id)).toEqual(["b", "a", "c"]);
    expect(groupOfTab(moved, "a")?.activeTabId).toBe("a");
  });

  it("reorders within a group", () => {
    const { root, right } = twoGroups();
    const moved = moveTab(root, "b", right, 2);
    expect(allTabs(moved).map((tab) => tab.id)).toEqual(["a", "c", "b"]);
  });

  it("docks beside a group, merging into a split of the same direction", () => {
    const { root, right } = twoGroups();
    const docked = dockTab(root, "c", right, "right");
    expect(docked.type).toBe("split");
    if (docked.type !== "split") return;
    expect(docked.children).toHaveLength(3);
    expect(docked.sizes).toEqual([0.5, 0.25, 0.25]);
    expect(allGroups(docked).map((entry) => entry.tabs.map((tab) => tab.id))).toEqual([
      ["a"],
      ["b"],
      ["c"],
    ]);

    const below = dockTab(root, "c", right, "bottom");
    if (below.type !== "split") throw new Error("expected a split");
    const column = below.children[1];
    expect(column.type === "split" && column.direction).toBe("column");
  });

  it("does not dock a group's only tab beside itself", () => {
    const { root, left } = twoGroups();
    expect(dockTab(root, "a", left, "left")).toBe(root);
  });

  it("activates a neighbor when the active tab closes", () => {
    let root: LayoutNode = group([localTab(), remote("x"), remote("y")]);
    root = addTab(root, allGroups(root)[0].id, remote("z"), 2);
    expect(allGroups(root)[0].activeTabId).toBe("z");
    root = removeTab(root, "z")!;
    expect(allGroups(root)[0].activeTabId).toBe("y");
    expect(removeTab(group([remote("only")]), "only")).toBeNull();
  });

  it("resizes within limits", () => {
    expect(resizeShares([0.5, 0.5], 0, 0.2)).toEqual([0.7, 0.30000000000000004]);
    expect(resizeShares([0.5, 0.5], 0, 0.9)[0]).toBeCloseTo(0.92);
  });

  it("normalizes nested splits", () => {
    const inner: LayoutNode = {
      type: "split",
      id: "inner",
      direction: "row",
      children: [group([remote("b")]), group([])],
      sizes: [0.5, 0.5],
    };
    const outer: LayoutNode = {
      type: "split",
      id: "outer",
      direction: "row",
      children: [group([remote("a")]), inner],
      sizes: [0.4, 0.6],
    };
    const normalized = normalize(outer);
    expect(normalized?.type === "split" && normalized.sizes).toEqual([0.4, 0.6]);
  });

  it("places a docked tab where the layout will put it", () => {
    const { root, right } = twoGroups();
    const swapped = dockTab(root, "a", right, "right");
    expect(layoutPlacements(swapped, 8).get(groupOfTab(swapped, "a")!.id)).toEqual({
      left: { share: 0.5, pixels: 4 },
      top: { share: 0, pixels: 0 },
      width: { share: 0.5, pixels: -4 },
      height: { share: 1, pixels: 0 },
    });

    const below = dockTab(root, "c", right, "bottom");
    const placements = layoutPlacements(below, 8);
    expect(placements.get(groupOfTab(below, "c")!.id)).toEqual({
      left: { share: 0.5, pixels: 4 },
      top: { share: 0.5, pixels: 4 },
      width: { share: 0.5, pixels: -4 },
      height: { share: 0.5, pixels: -4 },
    });
    expect(placements.get(below.id)).toEqual({
      left: { share: 0, pixels: 0 },
      top: { share: 0, pixels: 0 },
      width: { share: 1, pixels: 0 },
      height: { share: 1, pixels: 0 },
    });
    expect(placements.get("missing")).toBeUndefined();
  });

  it("keeps the panes, the folders they show and the servers they use", () => {
    const { root } = twoGroups();
    const restored = throughStorage(
      root,
      current(
        { a: "/home/me/projects", b: "/var/www" },
        { "session-b": server("web"), "session-c": server("db") },
      ),
    );
    expect(restored && allGroups(restored).map((entry) => entry.tabs)).toEqual([
      [{ id: "a", kind: "local", path: "/home/me/projects" }],
      [
        {
          id: "b",
          kind: "remote",
          sessionId: "session-b",
          path: "/var/www",
          restore: { server: server("web") },
        },
        { id: "c", kind: "remote", sessionId: "session-c", restore: { server: server("db") } },
      ],
    ]);
    expect(restored?.type === "split" && restored.sizes).toEqual([0.5, 0.5]);
    expect(parseLayout({ type: "group", id: "g", tabs: [{ id: 1 }] })).toBeNull();
    expect(parseLayout(JSON.parse(JSON.stringify(root)))).toEqual(root);
  });

  it("leaves out server tabs whose session has ended, and the panes they empty", () => {
    const { root } = twoGroups();
    const restored = throughStorage(root, current({}, { "session-c": server("db") }));
    expect(restored && allTabs(restored).map((tab) => tab.id)).toEqual(["a", "c"]);
    const alone = throughStorage(root, current());
    expect(alone?.type).toBe("group");
    expect(alone && allTabs(alone).map((tab) => tab.id)).toEqual(["a"]);
  });

  it("reopens editors from their files and terminals with a new shell", () => {
    const editor: PaneTab = {
      ...editorTab({
        id: "doc-1",
        name: "nginx.conf",
        path: "/etc/nginx/nginx.conf",
        origin: "srv",
      }),
      draft: { text: "user www;\n", encoding: "utf8", lineEnding: "lf", stamp: null },
    };
    const remoteEditor: PaneTab = {
      ...editorTab({ id: "doc-2", name: "app.env", path: "/srv/app.env", origin: "srv" }, "s"),
      id: "e2",
    };
    const shell: PaneTab = { ...terminalTab("s", "root@srv"), id: "t", terminalId: "shell-1" };
    const root = group([{ id: "a", kind: "local" }, editor, remoteEditor, shell]);
    expect(parseLayout(JSON.parse(JSON.stringify(root)))).toEqual(root);

    const restored = throughStorage(root, current({}, { s: server("srv") }));
    const tabs = restored ? allTabs(restored) : [];
    expect(tabs.map((tab) => tab.kind)).toEqual(["local", "editor", "editor", "terminal"]);
    expect(tabs.slice(1).every(isRestoring)).toBe(true);
    expect(tabs[1]).toEqual({
      id: editor.id,
      kind: "editor",
      documentId: "doc-1",
      name: "nginx.conf",
      path: "/etc/nginx/nginx.conf",
      origin: "srv",
      restore: {},
    });
    expect(tabs[2]).toMatchObject({ sessionId: "s", restore: { server: server("srv") } });
    expect(tabs[3]).toEqual({
      id: "t",
      kind: "terminal",
      sessionId: "s",
      label: "root@srv",
      restore: { server: server("srv") },
    });

    // Without the server, only the local tabs come back.
    const offline = throughStorage(root, current());
    expect(offline && allTabs(offline).map((tab) => tab.id)).toEqual(["a", editor.id]);

    const broken = { ...editor, draft: { text: 4 } };
    expect(parseLayout({ type: "group", id: "g", tabs: [broken] })).toBeNull();
    const nameless = { id: "t", kind: "terminal", sessionId: "s" };
    expect(parseLayout({ type: "group", id: "g", tabs: [nameless] })).toBeNull();
    const serverless = { id: "r", kind: "remote", sessionId: "s", restore: {} };
    expect(parseLayout({ type: "group", id: "g", tabs: [serverless] })).toBeNull();
    const badServer = { ...serverless, restore: { server: { label: "srv" } } };
    expect(parseLayout({ type: "group", id: "g", tabs: [badServer] })).toBeNull();
  });

  it("does not reopen an older version of a file", () => {
    const older: PaneTab = {
      ...editorTab({ id: "doc-3", name: "a.txt (2024)", path: "/bucket/a.txt", origin: "s3" }, "s"),
      id: "v",
      version: "3HL4kqtJlcpXroDTDmJ",
    };
    const root = group([{ id: "a", kind: "local" }, older]);
    expect(parseLayout(JSON.parse(JSON.stringify(root)))).toEqual(root);
    const restored = throughStorage(root, current({}, { s: server("s3") }));
    expect(restored && allTabs(restored).map((tab) => tab.id)).toEqual(["a"]);
    expect(parseLayout({ type: "group", id: "g", tabs: [{ ...older, version: 7 }] })).toBeNull();
  });

  it("reopens a tab showing a vault on the vault's folder on its server", () => {
    const vault: VaultInfo = { parentSessionId: "s", root: "/srv/private", name: "private" };
    const inVault: PaneTab = { id: "v", kind: "remote", sessionId: "vault-1" };
    const decrypted: PaneTab = {
      ...editorTab(
        { id: "doc-3", name: "plan.md", path: "/srv/private/plan.md", origin: "private" },
        "vault-1",
      ),
      id: "e3",
    };
    const root = group([inVault, decrypted]);
    const state = current(
      { v: "/srv/private/letters" },
      { s: server("srv") },
      { "vault-1": vault },
    );

    // The vault is locked by then, so a file from inside it has nothing to reopen from.
    expect(throughStorage(root, state)).toMatchObject({
      tabs: [
        {
          id: "v",
          kind: "remote",
          sessionId: "s",
          path: "/srv/private",
          restore: { server: server("srv") },
        },
      ],
    });
    // Without its server the vault's tab cannot come back either.
    expect(throughStorage(root, current({}, {}, { "vault-1": vault }))).toBeNull();
  });

  it("keeps tabs still waiting for their server for the next start", () => {
    const waiting: PaneTab = {
      id: "b",
      kind: "remote",
      sessionId: "old-session",
      path: "/var/www",
      restore: { server: server("web") },
    };
    const root = group([waiting]);
    expect(throughStorage(root, current({ b: "/elsewhere" }))).toEqual(root);
  });

  it("opens a tab in a pane of its own after the pane it came from", () => {
    const { root, left, right } = twoGroups();
    const shell = terminalTab("session-b", "root@srv");
    const opened = openTab(root, shell, left);
    expect(opened.type === "split" && opened.direction).toBe("row");
    const groups = allGroups(opened);
    expect(groups.map((entry) => entry.id)).toEqual([left, groups[1].id, right]);
    expect(groups[1].tabs).toEqual([shell]);
    const sizes = opened.type === "split" ? opened.sizes : [];
    expect(sizes.map((size) => size.toFixed(3))).toEqual(["0.333", "0.333", "0.333"]);
  });

  it("keeps the sizes the panes had, relative to each other", () => {
    const { root, right } = twoGroups();
    const resized = { ...root, sizes: [0.7, 0.3] } as LayoutNode;
    const opened = openTab(resized, terminalTab("session-b", "root@srv"), right);
    const sizes = opened.type === "split" ? opened.sizes : [];
    expect(sizes.map((size) => size.toFixed(3))).toEqual(["0.467", "0.200", "0.333"]);
  });

  it("opens beside a single pane or a stack of panes", () => {
    const single = group([{ id: "a", kind: "local" }]);
    const opened = openTab(single, terminalTab("session-b", "root@srv"), single.id);
    expect(opened.type === "split" && [opened.direction, opened.sizes]).toEqual([
      "row",
      [0.5, 0.5],
    ]);

    const top = group([{ id: "a", kind: "local" }]);
    const stack: LayoutNode = {
      type: "split",
      id: "stack",
      direction: "column",
      children: [top, group([remote("b")])],
      sizes: [0.5, 0.5],
    };
    const beside = openTab(stack, terminalTab("session-b", "root@srv"), top.id);
    expect(beside.type === "split" && [beside.direction, beside.children[0]]).toEqual([
      "row",
      stack,
    ]);
  });

  it("joins the pane already showing a tab of its kind", () => {
    const { root, left } = twoGroups();
    const first = terminalTab("session-b", "root@srv");
    const second = terminalTab("session-c", "root@backup");
    const once = openTab(root, first, left);
    const twice = openTab(once, second, left);
    expect(allGroups(twice)).toHaveLength(3);
    expect(groupOfTab(twice, second.id)?.tabs).toEqual([first, second]);
    expect(groupOfTab(twice, second.id)?.activeTabId).toBe(second.id);
  });

  it("opens as a tab once the panes would get too narrow", () => {
    const columns = [0, 1, 2, 3].map(() => group([localTab()]));
    const root: LayoutNode = {
      type: "split",
      id: "row",
      direction: "row",
      children: columns,
      sizes: [0.25, 0.25, 0.25, 0.25],
    };
    const shell = terminalTab("session-b", "root@srv");
    const fifth = openTab(root, shell, columns[3].id);
    expect(allGroups(fifth)).toHaveLength(5);
    const file = editorTab({ id: "doc-1", name: "a.txt", path: "/a.txt", origin: "local" });
    const crowded = openTab(fifth, file, columns[1].id);
    expect(allGroups(crowded)).toHaveLength(5);
    expect(groupOfTab(crowded, file.id)?.id).toBe(columns[1].id);
  });
});

describe("a pane's preview", () => {
  const tabOf = (root: LayoutNode, id: string) => allTabs(root).find((tab) => tab.id === id);

  it("stays open when the pane is given another session or folder", () => {
    const root = group([{ id: "a", kind: "local", preview: true }, remote("b")]);
    const connected = updateTab(root, "a", { id: "a", kind: "remote", sessionId: "s", path: "/" });
    expect(tabOf(connected, "a")).toEqual({
      id: "a",
      kind: "remote",
      sessionId: "s",
      path: "/",
      preview: true,
    });
    // A pane that had none gets none.
    const moved = updateTab(root, "b", { id: "b", kind: "remote", sessionId: "s", path: "/srv" });
    expect(tabOf(moved, "b")).toEqual({ id: "b", kind: "remote", sessionId: "s", path: "/srv" });
  });

  it("closes when the replacement says so, and is not passed on to other kinds of tab", () => {
    const root = group([{ id: "a", kind: "local", preview: true }]);
    const closed = updateTab(root, "a", { id: "a", kind: "local", preview: false });
    expect(tabOf(closed, "a")).toEqual({ id: "a", kind: "local", preview: false });
    const welcome = updateTab(root, "a", { id: "a", kind: "welcome" });
    expect(tabOf(welcome, "a")).toEqual({ id: "a", kind: "welcome" });
  });

  it("is open again after a restart, on this computer and on a server", () => {
    const root = group([
      { id: "a", kind: "local", preview: true },
      { id: "b", kind: "remote", sessionId: "session-b", preview: true },
      remote("c"),
    ]);
    const restored = throughStorage(
      root,
      current({}, { "session-b": server("web"), "session-c": server("db") }),
    );
    const tabs = restored ? allTabs(restored) : [];
    expect(tabs.map((tab) => "preview" in tab && tab.preview)).toEqual([true, true, false]);
  });

  it("is read as closed from a layout that stores anything but yes or no, keeping the tabs", () => {
    const stored = {
      type: "group",
      id: "g",
      tabs: [
        { id: "a", kind: "local", path: "/home/deploy", preview: "yes" },
        { id: "b", kind: "local", preview: 1 },
      ],
      activeTabId: "b",
    };
    const parsed = parseLayout(stored);
    expect(parsed).toMatchObject({
      type: "group",
      activeTabId: "b",
      tabs: [
        { id: "a", kind: "local", path: "/home/deploy" },
        { id: "b", kind: "local" },
      ],
    });
    const tabs = parsed?.type === "group" ? parsed.tabs : [];
    expect(tabs.some((tab) => "preview" in tab)).toBe(false);
    const open = { type: "group", id: "g", tabs: [{ id: "a", kind: "local", preview: true }] };
    expect(parseLayout(open)).toMatchObject({ tabs: [{ id: "a", preview: true }] });
  });
});
