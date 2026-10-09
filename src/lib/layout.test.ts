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
  persistableLayout,
  removeTab,
  resizeShares,
  terminalTab,
  type LayoutNode,
  type PaneTab,
} from "./layout";

const remote = (id: string): PaneTab => ({ id, kind: "remote", sessionId: `session-${id}` });

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

  it("round-trips through storage without remote tabs", () => {
    const { root } = twoGroups();
    const stored = JSON.parse(JSON.stringify(persistableLayout(root)));
    const restored = parseLayout(stored);
    expect(restored && allTabs(restored).map((tab) => tab.id)).toEqual(["a"]);
    expect(parseLayout({ type: "group", id: "g", tabs: [{ id: 1 }] })).toBeNull();
    expect(parseLayout(JSON.parse(JSON.stringify(root)))).toEqual(root);
  });

  it("hands editor and terminal tabs between windows but does not keep them", () => {
    const editor: PaneTab = {
      ...editorTab({
        id: "doc-1",
        name: "nginx.conf",
        path: "/etc/nginx/nginx.conf",
        origin: "srv",
      }),
      draft: { text: "user www;\n", encoding: "utf8", lineEnding: "lf", stamp: null },
    };
    const shell = terminalTab("session-1", "root@srv");
    const root = group([{ id: "a", kind: "local" }, editor, shell]);
    expect(parseLayout(JSON.parse(JSON.stringify(root)))).toEqual(root);
    const restored = parseLayout(JSON.parse(JSON.stringify(persistableLayout(root))));
    expect(restored && allTabs(restored).map((tab) => tab.kind)).toEqual(["local"]);

    const broken = { ...editor, draft: { text: 4 } };
    expect(parseLayout({ type: "group", id: "g", tabs: [broken] })).toBeNull();
    const nameless = { id: "t", kind: "terminal", sessionId: "s" };
    expect(parseLayout({ type: "group", id: "g", tabs: [nameless] })).toBeNull();
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
