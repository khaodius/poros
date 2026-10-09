// The dock layout of a window: groups of tabs, arranged in nested rows and columns. Every
// operation returns a new tree, normalized so no empty group or single-child split remains.

import type { DocumentInfo, FileStamp, TextLineEnding, TextEncoding } from "./types";

/** Unsaved text an editor tab takes along when it moves to another window. */
export interface EditorDraft {
  text: string;
  encoding: TextEncoding;
  lineEnding: TextLineEnding;
  /** The file as last read or saved; null when it never was. */
  stamp: FileStamp | null;
}

export type PaneTab =
  | { id: string; kind: "local"; path?: string }
  | { id: string; kind: "remote"; sessionId: string; path?: string }
  | { id: string; kind: "welcome" }
  | {
      id: string;
      kind: "editor";
      documentId: string;
      name: string;
      path: string;
      /** "Local", or the server's label. */
      origin: string;
      /** The session the file was opened from, for server files. */
      sessionId?: string;
      draft?: EditorDraft;
    }
  | {
      id: string;
      kind: "terminal";
      sessionId: string;
      label: string;
      /** Absent until the shell has started. */
      terminalId?: string;
    };

export type EditorTab = Extract<PaneTab, { kind: "editor" }>;
export type TerminalTab = Extract<PaneTab, { kind: "terminal" }>;

export interface GroupNode {
  type: "group";
  id: string;
  tabs: PaneTab[];
  activeTabId: string | null;
}

export interface SplitNode {
  type: "split";
  id: string;
  direction: "row" | "column";
  children: LayoutNode[];
  /** Shares of the split, summing to 1. */
  sizes: number[];
}

export type LayoutNode = GroupNode | SplitNode;
export type DropSide = "left" | "right" | "top" | "bottom" | "center";

export const MIN_SHARE = 0.08;

export function newId(prefix: string): string {
  return `${prefix}-${Math.random().toString(36).slice(2, 10)}`;
}

export function group(tabs: PaneTab[], activeTabId = tabs[0]?.id ?? null): GroupNode {
  return { type: "group", id: newId("group"), tabs, activeTabId };
}

export function localTab(path?: string): PaneTab {
  return { id: newId("tab"), kind: "local", path };
}

export function welcomeTab(): PaneTab {
  return { id: newId("tab"), kind: "welcome" };
}

export function editorTab(document: DocumentInfo, sessionId?: string): EditorTab {
  const { id: documentId, name, path, origin } = document;
  return { id: newId("tab"), kind: "editor", documentId, name, path, origin, sessionId };
}

export function terminalTab(sessionId: string, label: string): TerminalTab {
  return { id: newId("tab"), kind: "terminal", sessionId, label };
}

export function defaultLayout(): LayoutNode {
  return {
    type: "split",
    id: newId("split"),
    direction: "row",
    children: [group([localTab()]), group([welcomeTab()])],
    sizes: [0.5, 0.5],
  };
}

export function allGroups(node: LayoutNode): GroupNode[] {
  return node.type === "group" ? [node] : node.children.flatMap(allGroups);
}

export function allTabs(node: LayoutNode): PaneTab[] {
  return allGroups(node).flatMap((entry) => entry.tabs);
}

export function findGroup(node: LayoutNode, groupId: string): GroupNode | null {
  return allGroups(node).find((entry) => entry.id === groupId) ?? null;
}

export function groupOfTab(node: LayoutNode, tabId: string): GroupNode | null {
  return allGroups(node).find((entry) => entry.tabs.some((tab) => tab.id === tabId)) ?? null;
}

export function findTab(node: LayoutNode, tabId: string): PaneTab | null {
  return allTabs(node).find((tab) => tab.id === tabId) ?? null;
}

function mapGroups(node: LayoutNode, change: (entry: GroupNode) => LayoutNode): LayoutNode {
  if (node.type === "group") return change(node);
  return { ...node, children: node.children.map((child) => mapGroups(child, change)) };
}

function withTabs(entry: GroupNode, tabs: PaneTab[], activeTabId?: string | null): GroupNode {
  const present = (id: string | null | undefined) =>
    id != null && tabs.some((tab) => tab.id === id);
  if (present(activeTabId)) return { ...entry, tabs, activeTabId: activeTabId! };
  if (present(entry.activeTabId)) return { ...entry, tabs };
  // The active tab left: its neighbor takes over.
  const previousIndex = Math.max(0, indexOf(entry, entry.activeTabId));
  const neighbor = tabs[Math.min(tabs.length - 1, previousIndex)];
  return { ...entry, tabs, activeTabId: neighbor?.id ?? null };
}

function indexOf(entry: GroupNode, tabId: string | null): number {
  return entry.tabs.findIndex((tab) => tab.id === tabId);
}

/** Drops empty groups, folds single-child splits into their parent and merges nested splits
 * that run the same way. Returns null when no tab is left. */
export function normalize(node: LayoutNode): LayoutNode | null {
  if (node.type === "group") return node.tabs.length > 0 ? node : null;
  const children: LayoutNode[] = [];
  const sizes: number[] = [];
  node.children.forEach((child, index) => {
    const normalized = normalize(child);
    if (!normalized) return;
    const share = node.sizes[index] ?? 1 / node.children.length;
    if (normalized.type === "split" && normalized.direction === node.direction) {
      children.push(...normalized.children);
      sizes.push(...normalized.sizes.map((size) => size * share));
    } else {
      children.push(normalized);
      sizes.push(share);
    }
  });
  if (children.length === 0) return null;
  if (children.length === 1) return children[0];
  const total = sizes.reduce((sum, size) => sum + size, 0);
  return { ...node, children, sizes: sizes.map((size) => size / total) };
}

export function addTab(
  root: LayoutNode,
  groupId: string,
  tab: PaneTab,
  index?: number,
): LayoutNode {
  return mapGroups(root, (entry) => {
    if (entry.id !== groupId) return entry;
    const tabs = [...entry.tabs];
    tabs.splice(index ?? tabs.length, 0, tab);
    return withTabs(entry, tabs, tab.id);
  });
}

export function activateTab(root: LayoutNode, tabId: string): LayoutNode {
  return mapGroups(root, (entry) =>
    entry.tabs.some((tab) => tab.id === tabId) ? { ...entry, activeTabId: tabId } : entry,
  );
}

export function updateTab(root: LayoutNode, tabId: string, replacement: PaneTab): LayoutNode {
  return mapGroups(root, (entry) =>
    entry.tabs.some((tab) => tab.id === tabId)
      ? withTabs(
          entry,
          entry.tabs.map((tab) => (tab.id === tabId ? replacement : tab)),
          entry.activeTabId === tabId ? replacement.id : undefined,
        )
      : entry,
  );
}

/** Without normalizing, so a caller can still place the tab next to its old group. */
function detachTab(root: LayoutNode, tabId: string): LayoutNode {
  return mapGroups(root, (entry) =>
    entry.tabs.some((tab) => tab.id === tabId)
      ? withTabs(
          entry,
          entry.tabs.filter((tab) => tab.id !== tabId),
        )
      : entry,
  );
}

export function removeTab(root: LayoutNode, tabId: string): LayoutNode | null {
  return normalize(detachTab(root, tabId));
}

export function moveTab(
  root: LayoutNode,
  tabId: string,
  targetGroupId: string,
  index?: number,
): LayoutNode {
  const tab = findTab(root, tabId);
  const source = groupOfTab(root, tabId);
  if (!tab || !source || !findGroup(root, targetGroupId)) return root;
  let insertAt = index;
  if (source.id === targetGroupId && insertAt !== undefined && indexOf(source, tabId) < insertAt) {
    insertAt -= 1;
  }
  return normalize(addTab(detachTab(root, tabId), targetGroupId, tab, insertAt)) ?? root;
}

/** Docks a tab beside a group, splitting the space the group had. */
export function dockTab(
  root: LayoutNode,
  tabId: string,
  targetGroupId: string,
  side: DropSide,
): LayoutNode {
  if (side === "center") return moveTab(root, tabId, targetGroupId);
  const tab = findTab(root, tabId);
  const source = groupOfTab(root, tabId);
  if (!tab || !source) return root;
  if (source.id === targetGroupId && source.tabs.length === 1) return root;

  const direction = side === "left" || side === "right" ? "row" : "column";
  const before = side === "left" || side === "top";
  const placed = group([tab]);
  const replace = (node: LayoutNode): LayoutNode => {
    if (node.type === "group") {
      if (node.id !== targetGroupId) return node;
      return {
        type: "split",
        id: newId("split"),
        direction,
        children: before ? [placed, node] : [node, placed],
        sizes: [0.5, 0.5],
      };
    }
    return { ...node, children: node.children.map(replace) };
  };
  return normalize(replace(detachTab(root, tabId))) ?? root;
}

export function setSizes(root: LayoutNode, splitId: string, sizes: number[]): LayoutNode {
  if (root.type === "group") return root;
  if (root.id === splitId) return { ...root, sizes };
  return { ...root, children: root.children.map((child) => setSizes(child, splitId, sizes)) };
}

/** Moves the boundary after `index` by `delta` (a share of the split), keeping both sides
 * at least `MIN_SHARE`. */
export function resizeShares(sizes: number[], index: number, delta: number): number[] {
  const pair = sizes[index] + sizes[index + 1];
  const first = Math.min(pair - MIN_SHARE, Math.max(MIN_SHARE, sizes[index] + delta));
  const next = [...sizes];
  next[index] = first;
  next[index + 1] = pair - first;
  return next;
}

/** A length as a share of the whole dock plus a pixel offset, so it holds at any size. */
export interface Extent {
  share: number;
  pixels: number;
}

export interface Placement {
  left: Extent;
  top: Extent;
  width: Extent;
  height: Extent;
}

const WHOLE: Placement = {
  left: { share: 0, pixels: 0 },
  top: { share: 0, pixels: 0 },
  width: { share: 1, pixels: 0 },
  height: { share: 1, pixels: 0 },
};

export function addExtents(first: Extent, second: Extent): Extent {
  return { share: first.share + second.share, pixels: first.pixels + second.pixels };
}

/**
 * Where every group and split sits in the area the layout fills, keyed by node id, with `gap`
 * pixels between split children.
 */
export function layoutPlacements(
  node: LayoutNode,
  gap: number,
  area: Placement = WHOLE,
  placements = new Map<string, Placement>(),
): Map<string, Placement> {
  placements.set(node.id, area);
  if (node.type === "group") return placements;
  const row = node.direction === "row";
  const along = row ? area.width : area.height;
  const free = along.pixels - gap * (node.children.length - 1);
  let offset = row ? area.left : area.top;
  for (const [index, child] of node.children.entries()) {
    const share = node.sizes[index];
    const size = { share: along.share * share, pixels: free * share };
    layoutPlacements(
      child,
      gap,
      row ? { ...area, left: offset, width: size } : { ...area, top: offset, height: size },
      placements,
    );
    offset = addExtents(offset, { share: size.share, pixels: size.pixels + gap });
  }
  return placements;
}

const isString = (value: unknown): value is string => typeof value === "string";
const isOptionalString = (value: unknown) => value === undefined || isString(value);

function isDraft(value: unknown): value is EditorDraft {
  if (!value || typeof value !== "object") return false;
  const draft = value as Record<string, unknown>;
  return isString(draft.text) && isString(draft.encoding) && isString(draft.lineEnding);
}

function isTab(value: unknown): value is PaneTab {
  if (!value || typeof value !== "object") return false;
  const tab = value as Record<string, unknown>;
  if (typeof tab.id !== "string") return false;
  if (tab.path !== undefined && typeof tab.path !== "string") return false;
  switch (tab.kind) {
    case "remote":
      return isString(tab.sessionId);
    case "editor":
      return (
        [tab.documentId, tab.name, tab.path, tab.origin].every(isString) &&
        isOptionalString(tab.sessionId) &&
        (tab.draft === undefined || isDraft(tab.draft))
      );
    case "terminal":
      return isString(tab.sessionId) && isString(tab.label) && isOptionalString(tab.terminalId);
    default:
      return tab.kind === "local" || tab.kind === "welcome";
  }
}

/** Checks a layout read from storage or handed over by another window. */
export function parseLayout(value: unknown): LayoutNode | null {
  if (!value || typeof value !== "object") return null;
  const node = value as Record<string, unknown>;
  if (typeof node.id !== "string") return null;
  if (node.type === "group") {
    if (!Array.isArray(node.tabs) || !node.tabs.every(isTab)) return null;
    const tabs = node.tabs as PaneTab[];
    const activeTabId = typeof node.activeTabId === "string" ? node.activeTabId : null;
    return normalize(withTabs({ type: "group", id: node.id, tabs, activeTabId }, tabs));
  }
  if (node.type !== "split" || (node.direction !== "row" && node.direction !== "column")) {
    return null;
  }
  if (!Array.isArray(node.children) || !Array.isArray(node.sizes)) return null;
  const children = node.children.map(parseLayout);
  if (children.some((child) => child === null)) return null;
  const sizes = node.sizes.map(Number);
  if (sizes.length !== children.length || sizes.some((size) => !(size > 0))) return null;
  return normalize({
    type: "split",
    id: node.id,
    direction: node.direction,
    children: children as LayoutNode[],
    sizes,
  });
}

/** The layout worth restoring after a restart: server, editor and terminal tabs need what
 * only this run had open, so they go. */
export function persistableLayout(root: LayoutNode): LayoutNode | null {
  return normalize(
    mapGroups(root, (entry) =>
      withTabs(
        entry,
        entry.tabs.filter((tab) => tab.kind === "local" || tab.kind === "welcome"),
      ),
    ),
  );
}
