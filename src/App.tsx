import { useEffect, useState } from "react";
import { CalendarClock, FolderSync, Plus, SlidersHorizontal } from "lucide-react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CloseTabDialog } from "./components/CloseTabDialog";
import { CommandOutputDialog } from "./components/CommandOutputDialog";
import { ConflictDialog } from "./components/ConflictDialog";
import { ConnectDialog } from "./components/ConnectDialog";
import { DragGhost } from "./components/DragGhost";
import { FileOperationsHost } from "./components/FileOperationsHost";
import { HostKeyDialog } from "./components/HostKeyDialog";
import { QuickConnectBar } from "./components/QuickConnectBar";
import { PorosLogo } from "./components/PorosLogo";
import { RunCommandDialog } from "./components/RunCommandDialog";
import { SaveConnectionDialog } from "./components/SaveConnectionDialog";
import { SchedulerDialog } from "./components/SchedulerDialog";
import { SettingsDialog } from "./components/SettingsDialog";
import { SplashScreen } from "./components/SplashScreen";
import { StatusBar } from "./components/StatusBar";
import { SyncDialog } from "./components/SyncDialog";
import { Toasts } from "./components/Toasts";
import { UnsavedChangesDialog } from "./components/UnsavedChangesDialog";
import { UpdateDialog } from "./components/UpdateDialog";
import { WhenDoneCountdown } from "./components/WhenDoneCountdown";
import { WindowControls } from "./components/WindowControls";
import { Workspace } from "./components/Workspace";
import { useTabDragsBetweenWindows } from "./hooks/useTabDragsBetweenWindows";
import { useWindowMaximized } from "./hooks/useWindowMaximized";
import { EMPTY_DRAFT } from "./lib/connectDraft";
import {
  RETURN_TAB_EVENT,
  onLog,
  onQueueFinished,
  onSessionClosed,
  onStoreChanged,
  onTransfers,
  toAppError,
  windows,
} from "./lib/ipc";
import { findGroup, group, welcomeTab } from "./lib/layout";
import { DEFAULT_SETTINGS, FONT_SIZE_LIMITS } from "./lib/settings";
import { dragWindowFrom } from "./lib/windowDrag";
import { matchWindowBackground, revealWindow } from "./lib/windowReveal";
import type { StoreName } from "./lib/types";
import { startClipboard } from "./state/clipboardStore";
import { hitTest, useDragStore, type DragPayload } from "./state/dragStore";
import { persistLayout, restoreLayout, useLayoutStore } from "./state/layoutStore";
import { useLogStore } from "./state/logStore";
import { askToSave, unsavedEditorTabs } from "./state/editorActions";
import { useSavedConnections } from "./state/savedConnectionsStore";
import { reloadSchedules } from "./state/scheduleStore";
import { useSessionStore } from "./state/sessionStore";
import { saveSettingsSection, useSettingsStore } from "./state/settingsStore";
import { adoptHandoff, isMainWindow, receiveTabs, requestCloseTab } from "./state/tabActions";
import { applyTheme, findTheme, rememberTheme, useThemeStore } from "./state/themeStore";
import { uploadDroppedPaths } from "./state/transferActions";
import { useTransferStore } from "./state/transferStore";
import { useUiStore } from "./state/uiStore";
import { checkForUpdates } from "./state/updateStore";
import { handleQueueFinished } from "./state/whenDone";

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
    case "schedules":
      return reloadSchedules().catch((caught) =>
        reportError("Could not read scheduled tasks", caught),
      );
  }
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
          void receiveTabs(event.payload, null);
        }),
        // Only the main window acts, so the action happens once however many windows are open.
        onQueueFinished((finished) => void handleQueueFinished(finished)),
      );
    }
    return () => {
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
    };
  }, []);
}

/**
 * Applies the active theme and the appearance settings. A component of its own, so theme edits
 * and slider previews re-render only this instead of the whole app.
 */
function AppliedTheme(): null {
  const appearance = useSettingsStore((state) => state.settings.appearance);
  const theme = useThemeStore((state) => findTheme(state.files, appearance.theme).theme);
  useEffect(() => {
    const apply = () => {
      applyTheme(theme, appearance);
      void matchWindowBackground();
    };
    apply();
    rememberTheme(theme, appearance);
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    scheme.addEventListener("change", apply);
    return () => scheme.removeEventListener("change", apply);
  }, [theme, appearance]);

  // Theme files edited in another program apply when the user switches back.
  useEffect(() => {
    const reload = () => void useThemeStore.getState().load();
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, []);
  return null;
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

/** Asks about unsaved files before the window closes. */
function useCloseGuard() {
  useEffect(() => {
    const subscription = getCurrentWindow().onCloseRequested((event) => {
      const unsaved = unsavedEditorTabs();
      if (unsaved.length === 0) return;
      event.preventDefault();
      askToSave(
        unsaved.map((tab) => tab.id),
        true,
      );
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
}

function useShortcuts() {
  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      if (useUiStore.getState().dialog) return;
      // Ctrl+W, Ctrl+T and the like edit the command line in a shell.
      if ((event.target as HTMLElement).closest?.(".terminal-host")) return;
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
      if (!target.closest("input, textarea, .selectable, .cm-content")) event.preventDefault();
    };
    window.addEventListener("keydown", handleKey);
    window.addEventListener("contextmenu", suppressNativeMenu);
    return () => {
      window.removeEventListener("keydown", handleKey);
      window.removeEventListener("contextmenu", suppressNativeMenu);
    };
  }, []);
}

/** Leaves the start-up screen and the first listings to finish before asking for updates. */
const UPDATE_CHECK_DELAY_MILLIS = 3000;

/** Looks for a newer release once the main window has loaded, unless turned off. */
function useUpdateCheckOnStart(ready: boolean) {
  useEffect(() => {
    // Development builds are not installed copies, so they only check when asked to.
    if (!ready || !isMainWindow() || import.meta.env.DEV) return;
    if (!useSettingsStore.getState().settings.updates.checkOnStart) return;
    const timer = window.setTimeout(() => void checkForUpdates("start"), UPDATE_CHECK_DELAY_MILLIS);
    return () => window.clearTimeout(timer);
  }, [ready]);
}

const FONT_SIZE_KEYS: Record<string, number> = { "=": 1, "+": 1, "-": -1, _: -1 };
/** Wheel distance, in pixels, that changes the font size by one step on a touchpad. */
const WHEEL_STEP = 50;
const WHEEL_IDLE_MILLIS = 300;

function changeFontSize(change: (size: number) => number): void {
  const current = useSettingsStore.getState().settings.appearance.fontSize;
  const next = Math.min(FONT_SIZE_LIMITS.max, Math.max(FONT_SIZE_LIMITS.min, change(current)));
  if (next !== current) void saveSettingsSection("appearance", { fontSize: next });
}

/** Ctrl with +, - or 0, or with the mouse wheel, changes the text size like a browser zoom. */
function useFontSizeShortcuts() {
  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      if (event.key in FONT_SIZE_KEYS) {
        changeFontSize((size) => size + FONT_SIZE_KEYS[event.key]);
      } else if (event.key === "0" && !event.shiftKey) {
        changeFontSize(() => DEFAULT_SETTINGS.appearance.fontSize);
      } else {
        return;
      }
      event.preventDefault();
    };
    let wheelDistance = 0;
    let lastWheelAt = 0;
    const handleWheel = (event: WheelEvent) => {
      if (!event.ctrlKey) return;
      event.preventDefault();
      if (event.timeStamp - lastWheelAt > WHEEL_IDLE_MILLIS) wheelDistance = 0;
      lastWheelAt = event.timeStamp;
      wheelDistance +=
        event.deltaMode === WheelEvent.DOM_DELTA_PIXEL ? event.deltaY : event.deltaY * WHEEL_STEP;
      // One step per wheel notch, however far the system scrolls for one.
      if (Math.abs(wheelDistance) < WHEEL_STEP) return;
      changeFontSize((size) => size - Math.sign(wheelDistance));
      wheelDistance = 0;
    };
    window.addEventListener("keydown", handleKey);
    window.addEventListener("wheel", handleWheel, { passive: false });
    return () => {
      window.removeEventListener("keydown", handleKey);
      window.removeEventListener("wheel", handleWheel);
    };
  }, []);
}

export function App() {
  const [ready, setReady] = useState(false);
  const openDialog = useUiStore((state) => state.open);

  useEffect(() => {
    void revealWindow();
    void startApp()
      .catch((caught) => reportError("Could not start", caught))
      .finally(() => setReady(true));
  }, []);
  useEffect(() => startClipboard(), []);
  useBackendEvents();
  useCloseGuard();
  useTabDragsBetweenWindows();
  useSystemFileDrops();
  useShortcuts();
  useFontSizeShortcuts();
  useUpdateCheckOnStart(ready);
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
          <PorosLogo size={20} />
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
            title="Synchronize folders"
            aria-label="Synchronize folders"
            onClick={() => openDialog({ kind: "sync" })}
          >
            <FolderSync size={15} />
          </button>
          <button
            type="button"
            className="icon-button topbar-button"
            title="Scheduled tasks"
            aria-label="Scheduled tasks"
            onClick={() => openDialog({ kind: "schedules" })}
          >
            <CalendarClock size={15} />
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
      {ready && <AppliedTheme />}
      <AppDialog />
      <ConflictDialog />
      <FileOperationsHost />
      <HostKeyDialog />
      <CommandOutputDialog />
      {isMainWindow() && <WhenDoneCountdown />}
      <Toasts />
      <DragGhost />
      {isMainWindow() && <SplashScreen ready={ready} />}
    </div>
  );
}

function AppDialog() {
  const dialog = useUiStore((state) => state.dialog);
  const open = useUiStore((state) => state.open);
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
    case "sync":
      return (
        <SyncDialog
          localPath={dialog.localPath}
          sessionId={dialog.sessionId}
          remotePath={dialog.remotePath}
          onClose={close}
        />
      );
    case "update":
      return <UpdateDialog onClose={() => (dialog.returnTo ? open(dialog.returnTo) : close())} />;
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
    case "unsavedChanges":
      return (
        <UnsavedChangesDialog
          tabIds={dialog.tabIds}
          closeWindow={dialog.closeWindow}
          onClose={close}
        />
      );
    case "runCommand":
      return (
        <RunCommandDialog sessionId={dialog.sessionId} target={dialog.target} onClose={close} />
      );
    case "schedules":
      return <SchedulerDialog initialDraft={dialog.draft} onClose={close} />;
    default:
      return null;
  }
}
