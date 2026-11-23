import { Fragment, useEffect, useRef, useState, type MouseEvent } from "react";
import {
  AppWindow,
  Columns2,
  CornerUpLeft,
  HardDrive,
  Plus,
  Rows2,
  SquareTerminal,
  X,
  XSquare,
} from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { findTab, isRestoring, welcomeTab, type GroupNode, type PaneTab } from "../lib/layout";
import { isLost, sessionFamily } from "../lib/vaults";
import { beginDrag, isLifted, markedDrop, tabDrag, useDragStore } from "../state/dragStore";
import { useUnsavedStore } from "../state/editorRegistry";
import { useLayoutStore } from "../state/layoutStore";
import { requestMount } from "../state/mountActions";
import { useMountStore } from "../state/mountStore";
import { getPane } from "../state/paneRegistry";
import { useTabLabel } from "../hooks/useTabLabel";
import { moveWindowFrom } from "../lib/windowDrag";
import { useSettingsStore } from "../state/settingsStore";
import { useSessionStore } from "../state/sessionStore";
import { openTerminal } from "../state/terminalActions";
import {
  dockTabBeside,
  isMainWindow,
  moveTabTo,
  moveTabToWindow,
  requestCloseTab,
  returnTabToMain,
  splitTab,
  tearOutTab,
} from "../state/tabActions";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { CONNECTION_ICONS, TAB_ICONS, connectionKind } from "./tabIcons";

interface TabMenu {
  x: number;
  y: number;
  tab: PaneTab;
}

export function TabBar({ group }: { group: GroupNode }) {
  const addTab = useLayoutStore((state) => state.addTab);
  const systemTitleBar = useSettingsStore((state) => state.settings.interface.systemTitleBar);
  const [menu, setMenu] = useState<TabMenu | null>(null);
  const dropIndex = useDragStore(({ target }) => {
    if (target?.kind === "tabBar" && target.groupId === group.id) return target.index;
    const merging =
      target?.kind === "dock" && target.groupId === group.id && target.side === "center";
    return merging ? group.tabs.length : null;
  });
  const liftedTabId = useDragStore(({ payload, target }) =>
    payload?.kind === "tab" && isLifted(payload, target) ? payload.tabId : null,
  );
  // A tab joining this group from elsewhere: a place opens for it where it will go.
  const joining = useDragStore(
    useShallow(({ payload, target, incoming }) => {
      if (markedDrop(payload, target)?.groupId !== group.id) return null;
      if (target?.kind === "dock" && target.side !== "center") return null;
      if (payload?.kind === "tab") return { tabId: payload.tabId, label: payload.label };
      return incoming ? { tabId: null, label: incoming.label, kind: incoming.kind } : null;
    }),
  );
  const joiningKind = useLayoutStore((state) =>
    joining?.tabId ? findTab(state.root, joining.tabId)?.kind : joining?.kind,
  );
  const ghost = joining && (
    <TabGhost key="ghost" label={joining.label} kind={joiningKind ?? "welcome"} />
  );

  const menuItems = (tab: PaneTab): MenuItem[] => {
    const others = group.tabs.filter((other) => other.id !== tab.id);
    const connected =
      tab.kind === "remote" && !isRestoring(tab)
        ? useSessionStore.getState().sessions[tab.sessionId]
        : undefined;
    return [
      ...(connected?.info.protocol === "sftp"
        ? [
            {
              label: "Open terminal",
              icon: <SquareTerminal size={14} />,
              onSelect: () => openTerminal(connected.info.id, group.id),
            },
          ]
        : []),
      ...(connected
        ? [
            {
              label: "Mount as drive...",
              icon: <HardDrive size={14} />,
              onSelect: () =>
                requestMount(
                  connected.info.id,
                  getPane(tab.id)?.path() ?? (tab.kind === "remote" ? tab.path : null),
                ),
            },
            "separator" as const,
          ]
        : []),
      {
        label: "Split right",
        icon: <Columns2 size={14} />,
        onSelect: () => splitTab(tab.id, "right"),
      },
      {
        label: "Split down",
        icon: <Rows2 size={14} />,
        onSelect: () => splitTab(tab.id, "bottom"),
      },
      "separator",
      {
        label: "Move to new window",
        icon: <AppWindow size={14} />,
        onSelect: () => void tearOutTab(tab.id),
      },
      ...(isMainWindow()
        ? []
        : [
            {
              label: "Move to main window",
              icon: <CornerUpLeft size={14} />,
              onSelect: () => void returnTabToMain(tab.id),
            },
          ]),
      "separator",
      {
        label: "Close other tabs",
        icon: <XSquare size={14} />,
        disabled: others.length === 0,
        onSelect: () => others.forEach((other) => void requestCloseTab(other.id)),
      },
      {
        label: "Close",
        icon: <X size={14} />,
        shortcut: "Ctrl+W",
        onSelect: () => void requestCloseTab(tab.id),
      },
    ];
  };

  return (
    <div
      className="tab-bar"
      data-tab-bar={group.id}
      role="tablist"
      onWheel={(event) => {
        // Tabs that do not fit scroll sideways under the wheel.
        if (Math.abs(event.deltaY) > Math.abs(event.deltaX))
          event.currentTarget.scrollLeft += event.deltaY;
      }}
      // The space beside the tabs moves the window, as the top bar does, so a window just torn
      // out can be taken up again without aiming for the top bar.
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !systemTitleBar) moveWindowFrom(event);
      }}
      onDoubleClick={(event) => {
        if (event.target === event.currentTarget) addTab(welcomeTab(), group.id);
      }}
    >
      {group.tabs.map((tab, index) => (
        <Fragment key={tab.id}>
          {dropIndex === index && ghost}
          <TabButton
            tab={tab}
            selected={tab.id === group.activeTabId}
            lifted={tab.id === liftedTabId}
            dropBefore={!joining && dropIndex === index}
            dropAfter={
              !joining && dropIndex === group.tabs.length && index === group.tabs.length - 1
            }
            onContextMenu={(event) => {
              event.preventDefault();
              setMenu({ x: event.clientX, y: event.clientY, tab });
            }}
          />
        </Fragment>
      ))}
      {dropIndex === group.tabs.length && ghost}
      <button
        type="button"
        className="icon-button tab-add"
        title="New tab (Ctrl+T)"
        aria-label="New tab"
        onClick={() => addTab(welcomeTab(), group.id)}
      >
        <Plus size={15} />
      </button>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menuItems(menu.tab)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}

/**
 * Where a pane dropped now would join the tabs, as a tab of its own: it opens a place between
 * the tabs, moving on as the pointer does, and glows until the drop.
 */
function TabGhost({ label, kind }: { label: string; kind: PaneTab["kind"] }) {
  const Icon = TAB_ICONS[kind];
  return (
    <div className="tab-button tab-ghost" aria-hidden>
      <Icon size={14} className="tab-icon" />
      <span className="tab-label">{label}</span>
    </div>
  );
}

interface TabButtonProps {
  tab: PaneTab;
  selected: boolean;
  lifted: boolean;
  dropBefore: boolean;
  dropAfter: boolean;
  onContextMenu: (event: MouseEvent) => void;
}

function TabButton({
  tab,
  selected,
  lifted,
  dropBefore,
  dropAfter,
  onContextMenu,
}: TabButtonProps) {
  const label = useTabLabel(tab);
  const activateTab = useLayoutStore((state) => state.activateTab);
  const restoring = isRestoring(tab);
  const lost = useSessionStore((state) =>
    tab.kind === "remote" && !restoring ? isLost(state.sessions, tab.sessionId) : false,
  );
  const unsaved = useUnsavedStore((state) => Boolean(state.tabs[tab.id]));
  const protocol = useSessionStore((state) =>
    tab.kind === "remote"
      ? (state.sessions[tab.sessionId]?.info.protocol ?? tab.restore?.server?.protocol)
      : undefined,
  );
  const Icon = protocol ? CONNECTION_ICONS[connectionKind(protocol)] : TAB_ICONS[tab.kind];
  const tabRef = useRef<HTMLDivElement>(null);
  // The tab's server and its vaults, whose drives go when the tab closes.
  const family = useSessionStore(
    useShallow((state) =>
      tab.kind === "remote" && !restoring ? sessionFamily(state.sessions, tab.sessionId) : [],
    ),
  );
  const mountedAt = useMountStore(
    useShallow((state) =>
      state.drives
        .filter((drive) => family.includes(drive.sessionId))
        .map((drive) => drive.mountPoint),
    ),
  );

  // A tab chosen with the keyboard, or opened past the end of a full bar, scrolls into view.
  useEffect(() => {
    const element = tabRef.current;
    const bar = element?.parentElement;
    if (!selected || !element || !bar) return;
    const tabBounds = element.getBoundingClientRect();
    const barBounds = bar.getBoundingClientRect();
    if (tabBounds.left < barBounds.left) bar.scrollLeft -= barBounds.left - tabBounds.left;
    else if (tabBounds.right > barBounds.right) bar.scrollLeft += tabBounds.right - barBounds.right;
  }, [selected]);

  return (
    <div
      ref={tabRef}
      className={[
        "tab-button",
        selected && "is-selected",
        lifted && "is-lifted",
        dropBefore && "drop-before",
        dropAfter && "drop-after",
      ]
        .filter(Boolean)
        .join(" ")}
      data-tab-id={tab.id}
      role="tab"
      aria-selected={selected}
      title={label}
      onPointerDown={(event) => {
        if ((event.target as HTMLElement).closest(".tab-close")) return;
        activateTab(tab.id);
        const button = event.currentTarget;
        const start = { x: event.clientX, y: event.clientY };
        beginDrag(
          event,
          () => tabDrag(button, tab.id, label, start),
          (target) => {
            if (target.kind === "tabBar") moveTabTo(tab.id, target.groupId, target.index);
            else if (target.kind === "dock") dockTabBeside(tab.id, target.groupId, target.side);
            else if (target.kind === "window") {
              void moveTabToWindow(tab.id, target.label, target.x, target.y);
            } else if (target.kind === "outside") {
              void tearOutTab(tab.id, target.screen);
            }
          },
        );
      }}
      onAuxClick={(event) => {
        if (event.button === 1) void requestCloseTab(tab.id);
      }}
      onContextMenu={onContextMenu}
    >
      <Icon size={14} className={`tab-icon tab-icon-${tab.kind}`} />
      <span className="tab-label">{label}</span>
      {mountedAt.length > 0 && (
        <span className="tab-drive" title={`Mounted as a drive at ${mountedAt.join(", ")}`}>
          <HardDrive size={11} aria-label="Mounted as a drive" />
        </span>
      )}
      {lost && <span className="tab-lost" title="Disconnected" />}
      {unsaved && <span className="tab-unsaved" title="Unsaved changes" />}
      <button
        type="button"
        className="tab-close"
        aria-label={`Close ${label}`}
        title="Close (Ctrl+W)"
        onClick={() => void requestCloseTab(tab.id)}
      >
        <X size={12} />
      </button>
    </div>
  );
}
