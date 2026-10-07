import { useState, type MouseEvent } from "react";
import {
  AppWindow,
  Columns2,
  CornerUpLeft,
  HardDrive,
  Plus,
  Rows2,
  Server,
  Sparkles,
  X,
  XSquare,
} from "lucide-react";
import { welcomeTab, type GroupNode, type PaneTab } from "../lib/layout";
import { beginDrag, isLifted, tabDrag, useDragStore } from "../state/dragStore";
import { useLayoutStore } from "../state/layoutStore";
import { useTabLabel } from "../hooks/useTabLabel";
import { useSessionStore } from "../state/sessionStore";
import {
  dockTabBeside,
  isMainWindow,
  moveTabTo,
  requestCloseTab,
  returnTabToMain,
  splitTab,
  tearOutTab,
} from "../state/tabActions";
import { ContextMenu, type MenuItem } from "./ContextMenu";

interface TabMenu {
  x: number;
  y: number;
  tab: PaneTab;
}

export function TabBar({ group }: { group: GroupNode }) {
  const addTab = useLayoutStore((state) => state.addTab);
  const [menu, setMenu] = useState<TabMenu | null>(null);
  const dropIndex = useDragStore(({ target }) =>
    target?.kind === "tabBar" && target.groupId === group.id ? target.index : null,
  );
  const liftedTabId = useDragStore(({ payload, target }) =>
    payload?.kind === "tab" && isLifted(payload, target) ? payload.tabId : null,
  );

  const menuItems = (tab: PaneTab): MenuItem[] => {
    const others = group.tabs.filter((other) => other.id !== tab.id);
    return [
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
    <div className="tab-bar" data-tab-bar={group.id} role="tablist">
      {group.tabs.map((tab, index) => (
        <TabButton
          key={tab.id}
          tab={tab}
          selected={tab.id === group.activeTabId}
          lifted={tab.id === liftedTabId}
          dropBefore={dropIndex === index}
          dropAfter={dropIndex === group.tabs.length && index === group.tabs.length - 1}
          onContextMenu={(event) => {
            event.preventDefault();
            setMenu({ x: event.clientX, y: event.clientY, tab });
          }}
        />
      ))}
      <button
        type="button"
        className="icon-button tab-add"
        title="New tab"
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
  const lost = useSessionStore((state) =>
    tab.kind === "remote" ? state.sessions[tab.sessionId]?.status !== "connected" : false,
  );
  const Icon = tab.kind === "local" ? HardDrive : tab.kind === "remote" ? Server : Sparkles;

  return (
    <div
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
            else if (target.kind === "outside") {
              void tearOutTab(tab.id, target.screenX, target.screenY);
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
      {lost && <span className="tab-lost" title="Disconnected" />}
      <button
        type="button"
        className="tab-close"
        aria-label={`Close ${label}`}
        title="Close"
        onClick={() => void requestCloseTab(tab.id)}
      >
        <X size={12} />
      </button>
    </div>
  );
}
