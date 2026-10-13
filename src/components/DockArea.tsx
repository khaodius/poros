import { useMemo, useRef, useState, type CSSProperties, type PointerEvent } from "react";
import { SquareArrowOutUpRight } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { DOCK_GAP, TAB_BAR_HEIGHT, dropPlacement, resolveExtent } from "../lib/docking";
import {
  addExtents,
  allGroups,
  allTabs,
  layoutPlacements,
  resizeShares,
  setSizes,
  type DropSide,
  type Extent,
  type GroupNode,
  type LayoutNode,
  type PaneTab,
  type Placement,
  type SplitNode,
} from "../lib/layout";
import { MAIN_WINDOW } from "../lib/ipc";
import {
  isLifted,
  joinsTabBar,
  markedDrop,
  useDragStore,
  type DropTarget,
} from "../state/dragStore";
import { useLayoutStore } from "../state/layoutStore";
import { liftedLayers, liftedPane } from "../state/liftedPane";
import { DropHint, DropMarker } from "./DropMarker";
import { TabBar } from "./TabBar";
import { TabContent } from "./TabContent";
import { TAB_ICONS } from "./tabIcons";
import { WindowSketch } from "./WindowSketch";

const cssLength = ({ share, pixels }: Extent) => `calc(${share * 100}% + ${pixels}px)`;

function placementStyle({ left, top, width, height }: Placement): CSSProperties {
  return {
    left: cssLength(left),
    top: cssLength(top),
    width: cssLength(width),
    height: cssLength(height),
  };
}

/** The part of a group below its tab bar, overlapping the bar's bottom border. */
function belowTabBar(placement: Placement): Placement {
  const offset = TAB_BAR_HEIGHT - 1;
  return {
    ...placement,
    top: addExtents(placement.top, { share: 0, pixels: offset }),
    height: addExtents(placement.height, { share: 0, pixels: -offset }),
  };
}

function allSplits(node: LayoutNode): SplitNode[] {
  return node.type === "group" ? [] : [node, ...node.children.flatMap(allSplits)];
}

/** The handle in the gap after the split's child at `index`. */
function resizerPlacement(
  split: SplitNode,
  index: number,
  placements: Map<string, Placement>,
): Placement {
  const area = placements.get(split.id)!;
  const before = placements.get(split.children[index].id)!;
  const gap = { share: 0, pixels: DOCK_GAP };
  return split.direction === "row"
    ? { ...area, left: addExtents(before.left, before.width), width: gap }
    : { ...area, top: addExtents(before.top, before.height), height: gap };
}

interface Resizing {
  splitId: string;
  sizes: number[];
}

/**
 * Groups, resize handles and tab panes are all placed absolutely in one flat list, so moving a
 * tab between groups never remounts its pane, and position changes animate. A dragged tab
 * lifts its pane under the pointer, and the groups stay put while a marker under the pane shows
 * where it would land.
 */
export function DockArea() {
  const root = useLayoutStore((state) => state.root);
  const activeGroupId = useLayoutStore((state) => state.activeGroupId);
  const commitSizes = useLayoutStore((state) => state.setSizes);
  const dockRef = useRef<HTMLDivElement>(null);
  const [resizing, setResizing] = useState<Resizing | null>(null);
  const drag = useDragStore(
    useShallow(({ payload, target, pointer }) =>
      payload?.kind === "tab" ? { payload, target, pointer } : null,
    ),
  );

  // Also set while a tab from another window is over this one.
  const target = useDragStore((state) => state.target);
  const desktopPreview = useDragStore((state) => state.desktopPreview);
  const drawnOnDesktop = useDragStore((state) => state.drawnOnDesktop);
  const liftedTabId = drag && isLifted(drag.payload, drag.target) ? drag.payload.tabId : null;
  const dropTarget = markedDrop(drag?.payload ?? null, target);

  const layout = useMemo(
    () => (resizing ? setSizes(root, resizing.splitId, resizing.sizes) : root),
    [root, resizing],
  );
  const placements = useMemo(() => layoutPlacements(layout, DOCK_GAP), [layout]);
  const groups = allGroups(layout);
  // Sorted by id rather than layout order: moving a DOM node restarts its transitions.
  const tabs = allTabs(layout).sort((first, second) => (first.id < second.id ? -1 : 1));
  const groupOf = new Map<string, GroupNode>(
    groups.flatMap((entry) => entry.tabs.map((tab) => [tab.id, entry] as const)),
  );

  const leaving = target?.kind === "outside" || target?.kind === "window";
  // Off the window the tab shows as a preview on the desktop, or in the window under the
  // pointer, so the pane here fades out, having turned into the preview on its way to the edge.
  // Where the system cannot place a preview, the pane stays at the edge of the dock saying
  // where the tab will go.
  const away = leaving && desktopPreview;
  const lifted =
    drag && liftedTabId
      ? liftedPane(drag.payload, drag.pointer, drag.target, desktopPreview)
      : null;
  const floating = lifted?.rect ?? null;
  const toward = lifted?.toward ?? 0;
  const liftedTab = tabs.find((tab) => tab.id === liftedTabId);
  const layers = liftedLayers(toward);
  const sketchOpacity = layers.sketch;
  const contentOpacity = toward > 0 ? layers.content : undefined;

  // Dragging a handle previews the sizes in this component only and commits on release.
  const startResize = (split: SplitNode, index: number, event: PointerEvent<HTMLDivElement>) => {
    const dock = dockRef.current;
    if (!dock || event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const row = split.direction === "row";
    const bounds = dock.getBoundingClientRect();
    const area = placements.get(split.id)!;
    const along = row
      ? resolveExtent(area.width, bounds.width)
      : resolveExtent(area.height, bounds.height);
    const free = Math.max(1, along - DOCK_GAP * (split.children.length - 1));
    const origin = row ? event.clientX : event.clientY;
    let sizes = split.sizes;
    let frame = 0;

    const move = (moveEvent: globalThis.PointerEvent) => {
      // Released where this window could not see it: the resize ends where it is.
      if (!(moveEvent.buttons & 1)) {
        stop();
        return;
      }
      const position = row ? moveEvent.clientX : moveEvent.clientY;
      sizes = resizeShares(split.sizes, index, (position - origin) / free);
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        setResizing({ splitId: split.id, sizes });
      });
    };
    const stop = () => {
      cancelAnimationFrame(frame);
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      window.removeEventListener("blur", stop);
      commitSizes(split.id, sizes);
      setResizing(null);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
    // Another program took the pointer, such as a screenshot tool, and gets the release.
    window.addEventListener("blur", stop);
  };

  // Over another pane's tab bar, the lifted pane shrinks away toward the pointer, into the tab
  // that opens there.
  const joiningTabs = !!drag && !!liftedTabId && joinsTabBar(drag.payload, drag.target);
  const className = [
    "dock",
    resizing && "is-resizing",
    away && "is-tab-away",
    drawnOnDesktop && "is-drawn-on-desktop",
    joiningTabs && "is-joining-tabs",
  ]
    .filter(Boolean)
    .join(" ");
  const grabStyle = drag
    ? ({
        "--grab-x": `${drag.payload.grab.x}px`,
        "--grab-y": `${drag.payload.grab.y}px`,
      } as CSSProperties)
    : undefined;
  return (
    <div ref={dockRef} className={className} style={grabStyle}>
      {groups.map((entry) =>
        entry.tabs.every((tab) => tab.id === liftedTabId) ? (
          <div
            key={entry.id}
            className="dock-slot"
            style={placementStyle(placements.get(entry.id)!)}
          />
        ) : (
          <TabGroup
            key={entry.id}
            group={entry}
            style={placementStyle(placements.get(entry.id)!)}
            focused={entry.id === activeGroupId}
            emptied={liftedTabId !== null && entry.activeTabId === liftedTabId}
          />
        ),
      )}
      {allSplits(layout).flatMap((split) =>
        split.children.slice(1).map((child, index) => (
          <div
            key={`${split.id}:${child.id}`}
            className={`dock-resizer is-${split.direction}`}
            role="separator"
            aria-orientation={split.direction === "row" ? "vertical" : "horizontal"}
            title="Drag to resize, double-click to even out"
            style={placementStyle(resizerPlacement(split, index, placements))}
            onPointerDown={(event) => startResize(split, index, event)}
            onDoubleClick={() =>
              commitSizes(
                split.id,
                split.children.map(() => 1 / split.children.length),
              )
            }
          />
        )),
      )}
      {tabs.map((tab) => {
        const owner = groupOf.get(tab.id)!;
        const lifted = tab.id === liftedTabId;
        const visible = lifted || tab.id === owner.activeTabId;
        return (
          <TabPanel
            key={tab.id}
            tab={tab}
            groupId={owner.id}
            visible={visible}
            active={visible && owner.id === activeGroupId}
            floating={lifted}
            style={
              floating && drag && lifted
                ? {
                    // Its own size all along, so its content never reflows while it changes.
                    left: floating.left,
                    top: floating.top + TAB_BAR_HEIGHT - 1,
                    width: drag.payload.size.width,
                    height: drag.payload.size.height - TAB_BAR_HEIGHT + 1,
                    opacity: contentOpacity,
                  }
                : placementStyle(belowTabBar(placements.get(owner.id)!))
            }
          />
        );
      })}
      {floating && drag && (
        <FloatingFrame
          tab={liftedTab}
          label={drag.payload.label}
          leavingTo={leaving && !away ? target : null}
          drop={dropTarget?.side ?? null}
          titleOpacity={contentOpacity}
          style={floating}
        />
      )}
      {floating && drag && toward > 0 && (
        <WindowSketch
          className="dock-floating-window"
          content={{
            label: drag.payload.label,
            kind: liftedTab?.kind ?? "welcome",
            drop: dropTarget?.side ?? null,
          }}
          style={{ ...floating, opacity: away ? undefined : sketchOpacity }}
        />
      )}
      {floating && drag && <div className="dock-floating-shadow" style={floating} />}
      {dropTarget && placements.has(dropTarget.groupId) && (
        <DropMarker
          side={dropTarget.side}
          labelled={!floating || joiningTabs}
          style={placementStyle(
            dropPlacement(placements.get(dropTarget.groupId)!, dropTarget.side),
          )}
        />
      )}
    </div>
  );
}

interface TabGroupProps {
  group: GroupNode;
  style: CSSProperties;
  focused: boolean;
  /** The group's shown tab is being dragged away. */
  emptied: boolean;
}

function TabGroup({ group, style, focused, emptied }: TabGroupProps) {
  const focusGroup = useLayoutStore((state) => state.focusGroup);
  const className = ["tab-group", focused && "is-focused", emptied && "is-emptied"]
    .filter(Boolean)
    .join(" ");
  return (
    <section
      className={className}
      style={style}
      data-dock-group={group.id}
      onPointerDownCapture={() => focusGroup(group.id)}
      onFocusCapture={() => focusGroup(group.id)}
    >
      <TabBar group={group} />
      <div className="tab-content" />
    </section>
  );
}

interface TabPanelProps {
  tab: PaneTab;
  groupId: string;
  visible: boolean;
  active: boolean;
  floating: boolean;
  style: CSSProperties;
}

function TabPanel({ tab, groupId, visible, active, floating, style }: TabPanelProps) {
  const focusGroup = useLayoutStore((state) => state.focusGroup);
  return (
    <div
      className={`tab-panel ${floating ? "is-floating" : ""}`}
      style={style}
      hidden={!visible}
      onPointerDownCapture={() => focusGroup(groupId)}
      onFocusCapture={() => focusGroup(groupId)}
    >
      <TabContent tab={tab} visible={visible} active={active} />
    </div>
  );
}

interface FloatingFrameProps {
  tab: PaneTab | undefined;
  label: string;
  /** Where the pane goes when the pointer is outside the window. */
  leavingTo: DropTarget | null;
  /** How the pane docks where the pointer is in the window. */
  drop: DropSide | null;
  titleOpacity: number | undefined;
  style: CSSProperties;
}

function leavingHint(target: DropTarget): string {
  if (target.kind !== "window") return "New window";
  return target.label === MAIN_WINDOW ? "Move to main window" : "Move to that window";
}

function FloatingFrame({ tab, label, leavingTo, drop, titleOpacity, style }: FloatingFrameProps) {
  const Icon = TAB_ICONS[tab?.kind ?? "welcome"];
  return (
    <div className="dock-floating" style={style}>
      <div className="dock-floating-title" style={{ opacity: titleOpacity }}>
        <Icon size={14} className={`tab-icon tab-icon-${tab?.kind}`} />
        <span className="tab-label">{label}</span>
        {leavingTo && (
          <span className="dock-floating-hint">
            <SquareArrowOutUpRight size={12} />
            {leavingHint(leavingTo)}
          </span>
        )}
        {drop && <DropHint side={drop} className="dock-floating-hint" iconSize={12} />}
      </div>
    </div>
  );
}
