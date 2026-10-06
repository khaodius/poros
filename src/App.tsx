import { useEffect, useState } from "react";
import { Plus, SlidersHorizontal } from "lucide-react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CloseTabDialog } from "./components/CloseTabDialog";
import { ConflictDialog } from "./components/ConflictDialog";
import { ConnectDialog } from "./components/ConnectDialog";
import { DragGhost } from "./components/DragGhost";
import { HostKeyDialog } from "./components/HostKeyDialog";
import { QuickConnectBar } from "./components/QuickConnectBar";
import { SaveConnectionDialog } from "./components/SaveConnectionDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { StatusBar } from "./components/StatusBar";
import { Toasts } from "./components/Toasts";
import { WindowControls } from "./components/WindowControls";
import { Workspace } from "./components/Workspace";
import { useWindowMaximized } from "./hooks/useWindowMaximized";
import { EMPTY_DRAFT } from "./lib/connectDraft";
import {
  RETURN_TAB_EVENT,
  onLog,
  onSessionClosed,
  onStoreChanged,
  onTransfers,
  toAppError,
  windows,
} from "./lib/ipc";
import { findGroup, group, welcomeTab } from "./lib/layout";
import { dragWindowFrom } from "./lib/windowDrag";
import type { StoreName } from "./lib/types";
import { hitTest, useDragStore, type DragPayload } from "./state/dragStore";
import { persistLayout, restoreLayout, useLayoutStore } from "./state/layoutStore";
import { useLogStore } from "./state/logStore";
import { useSavedConnections } from "./state/savedConnectionsStore";
import { useSessionStore } from "./state/sessionStore";
import { useSettingsStore } from "./state/settingsStore";
import { adoptHandoff, isMainWindow, requestCloseTab } from "./state/tabActions";
import { applyTheme, findTheme, useThemeStore } from "./state/themeStore";
import { uploadDroppedPaths } from "./state/transferActions";
import { useTransferStore } from "./state/transferStore";
import { useUiStore } from "./state/uiStore";

let starting: Promise<void> | null = null;

/** Loads stored state and this window's tabs, once per window. */
function startApp(): Promise<void> {
  starting ??= (async () => {
    await useSettingsStore.getState().load();
    await Promise.all([
      useSavedConnections
        .getState()
        .load()
        .catch((caught) => reportError("Could not read saved connections", caught)),
      useThemeStore.getState().load(),
      useTransferStore.getState().load(),
    ]);
    if (isMainWindow()) {
      const remember = () => useSettingsStore.getState().settings.interface.rememberLayout;
      const restored = remember() ? restoreLayout() : null;
      if (restored) useLayoutStore.getState().setRoot(restored);
      persistLayout(remember);
    } else {
      const handoff = await windows.initialLayout().catch(() => null);
      const tabs = await adoptHandoff(handoff);
      useLayoutStore.getState().setRoot(group(tabs.length > 0 ? tabs : [welcomeTab()]));
    }
  })();
  return starting;
}

function reportError(context: string, caught: unknown): void {
  useLogStore.getState().write("error", `${context}: ${toAppError(caught).message}`);
}

function reloadStore(store: StoreName): Promise<void> {
  switch (store) {
    case "settings":
      return useSettingsStore.getState().load();
    case "connections":
      return useSavedConnections
        .getState()
        .load()
        .catch((caught) => reportError("Could not read saved connections", caught));
    case "themes":
      return useThemeStore.getState().load();
  }
}

/** Adds tabs a torn-out window handed back, and brings this window forward. */
async function receiveTabs(handoff: unknown): Promise<void> {
  for (const tab of await adoptHandoff(handoff)) useLayoutStore.getState().addTab(tab);
  await getCurrentWindow()
    .setFocus()
    .catch(() => undefined);
}

function activeTabId(): string | null {
  const { root, activeGroupId } = useLayoutStore.getState();
  return findGroup(root, activeGroupId)?.activeTabId ?? null;
}

function useBackendEvents() {
  useEffect(() => {
    const subscriptions = [
      onLog((record) => useLogStore.getState().append(record)),
      onSessionClosed(({ sessionId, reason }) =>
        useSessionStore.getState().markLost(sessionId, reason),
      ),
      onTransfers((update) => useTransferStore.getState().apply(update)),
      onStoreChanged((store) => void reloadStore(store)),
    ];
    if (isMainWindow()) {
      subscriptions.push(
        getCurrentWindow().listen<unknown>(RETURN_TAB_EVENT, (event) => {
          void receiveTabs(event.payload);
        }),
      );
    }
    return () => {
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
    };
  }, []);
}

function useAppliedTheme() {
  const themeId = useSettingsStore((state) => state.settings.appearance.theme);
  const fontSize = useSettingsStore((state) => state.settings.appearance.fontSize);
  const themes = useThemeStore((state) => state.files);
  useEffect(() => {
    const { theme } = findTheme(themes, themeId);
    const apply = () => applyTheme(theme, fontSize);
    apply();
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    scheme.addEventListener("change", apply);
    return () => scheme.removeEventListener("change", apply);
  }, [themes, themeId, fontSize]);

  // Theme files edited in another program apply when the user switches back.
  useEffect(() => {
    const reload = () => void useThemeStore.getState().load();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, []);
}

/** Shows the system title bar only when asked to; windows open without one. */
function useSystemTitleBar(): boolean {
  const systemTitleBar = useSettingsStore((state) => state.settings.interface.systemTitleBar);
  useEffect(() => {
    void getCurrentWindow()
      .setDecorations(systemTitleBar)
      .catch(() => undefined);
  }, [systemTitleBar]);
  return systemTitleBar;
}

// Windows gives frameless windows a system shadow and border; Linux window managers do not.
const NEEDS_DRAWN_BORDER = navigator.userAgent.includes("Linux");

// Windows reports drop positions in physical pixels; WebKitGTK and WKWebView in CSS pixels.
const DROP_POSITION_SCALE = navigator.userAgent.includes("Windows")
  ? () => window.devicePixelRatio || 1
  : () => 1;

/** Highlights where files dragged in from the system would land, and uploads them on drop. */
function useSystemFileDrops() {
  useEffect(() => {
    let dragged: DragPayload | null = null;
    const subscription = getCurrentWebview().onDragDropEvent(({ payload: event }) => {
      if (event.type === "enter") {
        dragged = event.paths.length > 0 ? { kind: "external", paths: event.paths } : null;
      }
      if (event.type === "leave" || !dragged) {
        useDragStore.setState({ payload: null, target: null });
        return;
      }
      const scale = DROP_POSITION_SCALE();
      const clientX = event.position.x / scale;
      const clientY = event.position.y / scale;
      const target = hitTest({ clientX, clientY, screenX: 0, screenY: 0 }, dragged);
      if (event.type === "drop") {
        dragged = null;
        useDragStore.setState({ payload: null, target: null });
        if (target?.kind === "pane") {
          void uploadDroppedPaths(event.paths, target.tabId, target.folder);
        }
        return;
      }
      useDragStore.setState({ payload: dragged, target, pointer: { x: clientX, y: clientY } });
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
}

function useShortcuts() {
  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      if (useUiStore.getState().dialog) return;
      const key = event.key.toLowerCase();
      if (key === "t") {
        useLayoutStore.getState().addTab(welcomeTab());
      } else if (key === "w") {
        const tabId = activeTabId();
        if (tabId) void requestCloseTab(tabId);
      } else if (key === ",") {
        useUiStore.getState().open({ kind: "settings", section: "transfers" });
      } else {
        return;
      }
      event.preventDefault();
    };
    const suppressNativeMenu = (event: MouseEvent) => {
      const target = event.target as HTMLElement;
      if (!target.closest("input, textarea, .selectable")) event.preventDefault();
    };
    window.addEventListener("keydown", handleKey);
    window.addEventListener("contextmenu", suppressNativeMenu);
    return () => {
      window.removeEventListener("keydown", handleKey);
      window.removeEventListener("contextmenu", suppressNativeMenu);
    };
  }, []);
}

export function App() {
  const [ready, setReady] = useState(false);
  const openDialog = useUiStore((state) => state.open);

  useEffect(() => {
    void startApp()
      .catch((caught) => reportError("Could not start", caught))
      .finally(() => setReady(true));
  }, []);
  useBackendEvents();
  useAppliedTheme();
  useSystemFileDrops();
  useShortcuts();
  const systemTitleBar = useSystemTitleBar();
  const maximized = useWindowMaximized();
  const drawnBorder = NEEDS_DRAWN_BORDER && !systemTitleBar && !maximized;

  return (
    <div className={`app ${drawnBorder ? "has-drawn-border" : ""}`}>
      <header
        className="topbar"
        data-window-drag={systemTitleBar ? undefined : ""}
        onMouseDown={systemTitleBar ? undefined : dragWindowFrom}
      >
        <div className="brand">
          <img src="/poros.svg" alt="" width={20} height={20} />
          <span>Poros</span>
        </div>
        <QuickConnectBar />
        <div className="topbar-actions">
          <button
            type="button"
            className="icon-button topbar-button"
            title="Connect to server"
            aria-label="Connect to server"
            onClick={() => openDialog({ kind: "connect", draft: EMPTY_DRAFT })}
          >
            <Plus size={17} />
          </button>
          <button
            type="button"
            className="icon-button topbar-button"
            title="Settings (Ctrl+,)"
            aria-label="Settings"
            onClick={() => openDialog({ kind: "settings", section: "transfers" })}
          >
            <SlidersHorizontal size={15} />
          </button>
        </div>
        {!systemTitleBar && <WindowControls maximized={maximized} />}
      </header>

      {ready ? <Workspace /> : <main className="workspace" />}

      <StatusBar />
      <AppDialog />
      <ConflictDialog />
      <HostKeyDialog />
      <Toasts />
      <DragGhost />
    </div>
  );
}

function AppDialog() {
  const dialog = useUiStore((state) => state.dialog);
  const close = useUiStore((state) => state.close);
  switch (dialog?.kind) {
    case "connect":
      return (
        <ConnectDialog
          initialDraft={dialog.draft}
          targetTabId={dialog.targetTabId}
          initialError={dialog.error}
          onClose={close}
        />
      );
    case "settings":
      return <SettingsDialog section={dialog.section} />;
    case "saveConnection":
      return <SaveConnectionDialog sessionId={dialog.sessionId} onClose={close} />;
    case "closeTab":
      return (
        <CloseTabDialog
          tabId={dialog.tabId}
          sessionId={dialog.sessionId}
          label={dialog.label}
          pendingTransfers={dialog.pendingTransfers}
          onClose={close}
        />
      );
    default:
      return null;
  }
}
