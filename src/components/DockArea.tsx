import { Fragment, useRef, type PointerEvent as ReactPointerEvent } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  dockTab,
  groupOfTab,
  placeGroup,
  resizeShares,
  type Extent,
  type GroupNode,
  type LayoutNode,
  type SplitNode,
} from "../lib/layout";
import { useDragStore } from "../state/dragStore";
import { useLayoutStore } from "../state/layoutStore";
import { TabBar } from "./TabBar";
import { TabContent } from "./TabContent";

export function DockArea() {
  const root = useLayoutStore((state) => state.root);
  return (
    <div className="dock">
      <LayoutView node={root} />
      <DockPreview root={root} />
    </div>
  );
}

// Matches the width of .dock-resizer.
const RESIZER_SIZE = 8;

const cssLength = ({ share, pixels }: Extent) => `calc(${share * 100}% + ${pixels}px)`;

/** Outlines where a dragged tab will end up, from the layout the drop would produce. */
function DockPreview({ root }: { root: LayoutNode }) {
  const drop = useDragStore(
    useShallow(({ payload, target }) =>
      payload?.kind === "tab" && target?.kind === "dock"
        ? { tabId: payload.tabId, groupId: target.groupId, side: target.side }
        : null,
    ),
  );
  if (!drop) return null;
  const next = dockTab(root, drop.tabId, drop.groupId, drop.side);
  const owner = next === root ? null : groupOfTab(next, drop.tabId);
  const placement = owner && placeGroup(next, owner.id, RESIZER_SIZE);
  if (!placement) return null;
  return (
    <div
      className="dock-preview"
      style={{
        left: cssLength(placement.left),
        top: cssLength(placement.top),
        width: cssLength(placement.width),
        height: cssLength(placement.height),
      }}
    />
  );
}

function LayoutView({ node }: { node: LayoutNode }) {
  return node.type === "group" ? <TabGroup group={node} /> : <DockSplit split={node} />;
}

function DockSplit({ split }: { split: SplitNode }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const setSizes = useLayoutStore((state) => state.setSizes);
  const row = split.direction === "row";

  // Dragging writes flex-grow straight to the two panels and commits on release, so the
  // panes do not re-render on every pointer move.
  const startResize = (index: number, event: ReactPointerEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    if (!container || event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const panels = [...container.children].filter((child): child is HTMLElement =>
      child.classList.contains("dock-panel"),
    );
    const handles = container.children.length - panels.length;
    const bounds = container.getBoundingClientRect();
    const handleSize = row ? handle.offsetWidth : handle.offsetHeight;
    const extent = (row ? bounds.width : bounds.height) - handles * handleSize;
    const origin = row ? event.clientX : event.clientY;
    let sizes = split.sizes;
    let frame = 0;

    const move = (moveEvent: PointerEvent) => {
      const position = row ? moveEvent.clientX : moveEvent.clientY;
      sizes = resizeShares(split.sizes, index, (position - origin) / Math.max(1, extent));
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        panels[index].style.flexGrow = String(sizes[index]);
        panels[index + 1].style.flexGrow = String(sizes[index + 1]);
      });
    };
    const stop = () => {
      cancelAnimationFrame(frame);
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      setSizes(split.id, sizes);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  };

  const equalize = () =>
    setSizes(
      split.id,
      split.children.map(() => 1 / split.children.length),
    );

  return (
    <div ref={containerRef} className={`dock-split dock-${split.direction}`}>
      {split.children.map((child, index) => (
        <Fragment key={child.id}>
          {index > 0 && (
            <div
              className="dock-resizer"
              role="separator"
              aria-orientation={row ? "vertical" : "horizontal"}
              title="Drag to resize, double-click to even out"
              onPointerDown={(event) => startResize(index - 1, event)}
              onDoubleClick={equalize}
            />
          )}
          <div className="dock-panel" style={{ flexGrow: split.sizes[index] }}>
            <LayoutView node={child} />
          </div>
        </Fragment>
      ))}
    </div>
  );
}

function TabGroup({ group }: { group: GroupNode }) {
  const focused = useLayoutStore((state) => state.activeGroupId === group.id);
  const focusGroup = useLayoutStore((state) => state.focusGroup);

  return (
    <section
      className={`tab-group ${focused ? "is-focused" : ""}`}
      data-dock-group={group.id}
      onPointerDownCapture={() => focusGroup(group.id)}
      onFocusCapture={() => focusGroup(group.id)}
    >
      <TabBar group={group} />
      <div className="tab-content">
        {group.tabs.map((tab) => {
          const visible = tab.id === group.activeTabId;
          return (
            <div key={tab.id} className="tab-panel" hidden={!visible}>
              <TabContent tab={tab} visible={visible} active={visible && focused} />
            </div>
          );
        })}
      </div>
    </section>
  );
}
