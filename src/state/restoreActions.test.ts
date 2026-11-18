import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  allTabs,
  findTab,
  group,
  type PaneTab,
  type RestoringTab,
  type SavedServer,
} from "../lib/layout";
import type { DocumentInfo, SessionInfo } from "../lib/types";
import { useLayoutStore } from "./layoutStore";
import { reopenWithSession, startRestoring, useRestoreStore } from "./restoreActions";
import { useSessionStore } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));
// Notices time out with the window's timers.
vi.stubGlobal("window", { setTimeout, clearTimeout });

const server: SavedServer = {
  label: "web",
  protocol: "sftp",
  profile: {
    protocol: "sftp",
    host: "web.example.com",
    port: 22,
    username: "deploy",
    auth: { type: "agent" },
  },
};

function session(id: string): SessionInfo {
  return {
    id,
    label: "web",
    host: "web.example.com",
    port: 22,
    username: "deploy",
    home: "/home/deploy",
    initialPath: "/home/deploy",
    protocol: "sftp",
  };
}

const files: RestoringTab = {
  id: "files",
  kind: "remote",
  sessionId: "before",
  path: "/var/www",
  restore: { server },
};
const shell: RestoringTab = {
  id: "shell",
  kind: "terminal",
  sessionId: "before",
  label: "deploy@web",
  restore: { server },
};
const other: RestoringTab = {
  id: "other",
  kind: "remote",
  sessionId: "elsewhere",
  restore: { server: { ...server, label: "db" } },
};
const notes: RestoringTab = {
  id: "notes",
  kind: "editor",
  documentId: "old-document",
  name: "notes.txt",
  path: "/home/me/notes.txt",
  origin: "Local",
  restore: {},
};

function show(...tabs: PaneTab[]) {
  useLayoutStore.getState().setRoot(group(tabs));
}

function setReconnectOnStart(reconnectOnStart: boolean) {
  const { settings } = useSettingsStore.getState();
  useSettingsStore.setState({
    settings: { ...settings, interface: { ...settings.interface, reconnectOnStart } },
  });
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  invoke.mockReset();
  useRestoreStore.setState({ servers: {} });
  useSessionStore.setState({ sessions: {} });
  setReconnectOnStart(true);
});

afterEach(() => useToastStore.setState({ toasts: [] }));

describe("restoring tabs", () => {
  it("shows every tab that shared a session in the session that replaces it", () => {
    show(files, shell, other);
    useRestoreStore.setState({ servers: { before: { phase: "connecting" } } });
    reopenWithSession("before", session("after"));

    const tabs = allTabs(useLayoutStore.getState().root);
    expect(tabs).toEqual([
      { id: "files", kind: "remote", sessionId: "after", path: "/var/www" },
      { id: "shell", kind: "terminal", sessionId: "after", label: "deploy@web" },
      other,
    ]);
    expect(useRestoreStore.getState().servers.before).toBeUndefined();
  });

  it("disconnects a session no tab waits for any more", () => {
    show(other);
    reopenWithSession("before", session("after"));
    expect(invoke).toHaveBeenCalledWith("disconnect", { sessionId: "after" });
  });

  it("connects once for tabs that shared a session", async () => {
    show(files, shell);
    invoke.mockImplementation(async (command: string) =>
      command === "connect" ? session("after") : undefined,
    );
    startRestoring(files);
    startRestoring(shell);
    await flush();

    expect(invoke.mock.calls.filter(([command]) => command === "connect")).toHaveLength(1);
    expect(allTabs(useLayoutStore.getState().root).map((tab) => tab.kind)).toEqual([
      "remote",
      "terminal",
    ]);
  });

  it("waits to be asked when reconnecting on start is off", () => {
    setReconnectOnStart(false);
    show(files);
    startRestoring(files);
    expect(invoke).not.toHaveBeenCalled();
    expect(useRestoreStore.getState().servers.before).toEqual({ phase: "waiting" });
  });

  it("asks for a sign-in when the server turns the old one down", async () => {
    show(files);
    invoke.mockRejectedValue({ kind: "authFailed", message: "Permission denied" });
    startRestoring(files);
    await flush();
    expect(useRestoreStore.getState().servers.before).toEqual({
      phase: "signIn",
      message: "Permission denied",
    });
    expect(findTab(useLayoutStore.getState().root, "files")).toEqual(files);
  });

  it("reopens a local file, or closes its tab when the file is gone", async () => {
    const document: DocumentInfo = {
      id: "new-document",
      name: "notes.txt",
      path: "/home/me/notes.txt",
      origin: "Local",
    };
    show(notes);
    invoke.mockResolvedValue(document);
    startRestoring(notes);
    await flush();
    expect(invoke).toHaveBeenCalledWith("editor_open", {
      location: { side: "local", path: "/home/me/notes.txt" },
    });
    expect(findTab(useLayoutStore.getState().root, "notes")).toMatchObject({
      kind: "editor",
      documentId: "new-document",
    });
    expect(findTab(useLayoutStore.getState().root, "notes")).not.toHaveProperty("restore");

    show(notes, other);
    invoke.mockRejectedValue({ kind: "notFound", message: "No such file" });
    startRestoring(notes);
    await flush();
    expect(findTab(useLayoutStore.getState().root, "notes")).toBeNull();
    expect(useToastStore.getState().toasts.map((toast) => toast.message)).toContain(
      "Could not reopen notes.txt: No such file",
    );
  });
});
