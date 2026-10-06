import { describe, expect, it } from "vitest";
import {
  addTab,
  allGroups,
  allTabs,
  dockTab,
  group,
  groupOfTab,
  localTab,
  moveTab,
  normalize,
  parseLayout,
  persistableLayout,
  placeGroup,
  removeTab,
  resizeShares,
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
    expect(placeGroup(swapped, groupOfTab(swapped, "a")!.id, 8)).toEqual({
      left: { share: 0.5, pixels: 4 },
      top: { share: 0, pixels: 0 },
      width: { share: 0.5, pixels: -4 },
      height: { share: 1, pixels: 0 },
    });

    const below = dockTab(root, "c", right, "bottom");
    expect(placeGroup(below, groupOfTab(below, "c")!.id, 8)).toEqual({
      left: { share: 0.5, pixels: 4 },
      top: { share: 0.5, pixels: 4 },
      width: { share: 0.5, pixels: -4 },
      height: { share: 0.5, pixels: -4 },
    });
    expect(placeGroup(below, "missing", 8)).toBeNull();
  });

  it("round-trips through storage without remote tabs", () => {
    const { root } = twoGroups();
    const stored = JSON.parse(JSON.stringify(persistableLayout(root)));
    const restored = parseLayout(stored);
    expect(restored && allTabs(restored).map((tab) => tab.id)).toEqual(["a"]);
    expect(parseLayout({ type: "group", id: "g", tabs: [{ id: 1 }] })).toBeNull();
    expect(parseLayout(JSON.parse(JSON.stringify(root)))).toEqual(root);
  });
});
