import { beforeEach, describe, expect, it, vi } from "vitest";
import { allTabs, group, type PaneTab } from "../lib/layout";
import type { Protocol, SessionInfo } from "../lib/types";
import { closeEditorTab } from "./editorActions";
import { useUnsavedStore } from "./editorRegistry";
import { useLayoutStore } from "./layoutStore";
import { useSessionStore } from "./sessionStore";
import {
  adoptHandoff,
  closeRemoteTab,
  familyOf,
  requestCloseTab,
  requestCloseTabs,
  type TabHandoff,
} from "./tabActions";
import { useUiStore } from "./uiStore";
import { requestLockVault } from "./vaultActions";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));
// Log lines are added with the window's timers.
vi.stubGlobal("window", { setTimeout, clearTimeout });

function server(id: string): SessionInfo {
  return {
    id,
    label: "deploy@web.example.com",
    host: "web.example.com",
    port: 22,
    username: "deploy",
    home: "/home/deploy",
    initialPath: "/home/deploy",
    protocol: "sftp",
  };
}

function vault(id: string, parentSessionId: string, root: string): SessionInfo {
  const name = root.slice(root.lastIndexOf("/") + 1);
  return {
    ...server(id),
    label: `${name} on deploy@web.example.com`,
    home: root,
    initialPath: root,
    protocol: "cryptomator",
    vault: { parentSessionId, root, name },
  };
}

/** What the other window's backend holds, by session. */
const held: Record<string, SessionInfo> = {
  s1: server("s1"),
  v1: vault("v1", "s1", "/srv/private"),
  v2: vault("v2", "s1", "/srv/ledger"),
};

const inVault: PaneTab = {
  id: "secret",
  kind: "remote",
  sessionId: "v1",
  path: "/srv/private/letters",
};

function handoff(tab: PaneTab): TabHandoff {
  return {
    layout: group([tab]),
    sessions: { s1: null, v1: null, v2: null },
    families: { [tab.kind === "remote" ? tab.sessionId : ""]: ["s1", "v1", "v2"] },
  };
}

const adoptedSessions = () =>
  invoke.mock.calls.flatMap(([command, args]) =>
    command === "adopt_session" ? [(args as { sessionId: string }).sessionId] : [],
  );

beforeEach(() => {
  invoke.mockReset();
  useSessionStore.setState({ sessions: {} });
});

describe("a tab handed over from another window", () => {
  it("brings its server and every vault unlocked on it, the server first", async () => {
    invoke.mockImplementation(async (_command: string, args: { sessionId: string }) => {
      return held[args.sessionId];
    });
    expect(await adoptHandoff(handoff(inVault))).toEqual([inVault]);
    expect(adoptedSessions()).toEqual(["s1", "v1", "v2"]);
    expect(familyOf("v2")).toEqual(["s1", "v1", "v2"]);
    expect(useSessionStore.getState().sessions.v1.info.vault?.parentSessionId).toBe("s1");
  });

  it("comes with its server alone when it shows no vault", async () => {
    invoke.mockImplementation(async (_command: string, args: { sessionId: string }) => {
      return held[args.sessionId];
    });
    const files: PaneTab = { id: "files", kind: "remote", sessionId: "s1", path: "/srv" };
    const alone: TabHandoff = { layout: group([files]), sessions: { s1: null } };
    expect(await adoptHandoff(alone)).toEqual([files]);
    expect(adoptedSessions()).toEqual(["s1"]);
  });

  it("stays on the server when its vault was locked on the way over", async () => {
    invoke.mockImplementation(async (_command: string, args: { sessionId: string }) => {
      if (args.sessionId === "v1") throw { kind: "sessionNotFound", message: "Closed." };
      return held[args.sessionId];
    });
    expect(await adoptHandoff(handoff(inVault))).toEqual([
      { id: "secret", kind: "remote", sessionId: "s1" },
    ]);
    expect(Object.keys(useSessionStore.getState().sessions).sort()).toEqual(["s1", "v2"]);
  });

  it("is a new tab when its server cannot be taken over, without asking for its vaults", async () => {
    invoke.mockRejectedValue({ kind: "sessionNotFound", message: "Closed." });
    expect(await adoptHandoff(handoff(inVault))).toEqual([{ id: "secret", kind: "welcome" }]);
    expect(adoptedSessions()).toEqual(["s1"]);
  });
});

const on = (protocol: Protocol, info: SessionInfo): SessionInfo => ({ ...info, protocol });
const page = (id: string, sessionId: string): PaneTab => ({
  id,
  kind: "editor",
  documentId: `document-${id}`,
  name: `${id}.html`,
  path: `/srv/www/${id}.html`,
  origin: "deploy@web.example.com",
  sessionId,
});

function know(...infos: SessionInfo[]) {
  useSessionStore.setState({
    sessions: Object.fromEntries(
      infos.map((info) => [info.id, { info, profile: null, status: "connected" as const }]),
    ),
  });
}

function open(...tabs: PaneTab[]) {
  useLayoutStore.getState().setRoot(group(tabs));
  for (const tab of tabs) useUnsavedStore.getState().mark(tab.id, tab.kind === "editor");
}

const tabIds = () =>
  allTabs(useLayoutStore.getState().root).flatMap((tab) => (tab.kind === "welcome" ? [] : tab.id));
const disconnected = () =>
  invoke.mock.calls.flatMap(([command, args]) =>
    command === "disconnect" ? [(args as { sessionId: string }).sessionId] : [],
  );
/** What "Don't save" does: the files' tabs close, then the question does. */
const discard = (...editorTabIds: string[]) => {
  editorTabIds.forEach(closeEditorTab);
  useUiStore.getState().close();
};

describe("closing what the files in the editor are saved through", () => {
  beforeEach(() => {
    useUiStore.setState({ dialog: null });
    invoke.mockImplementation(async (command: string) =>
      command === "transfer_session_jobs" ? 0 : undefined,
    );
  });

  it("asks about them before closing a server other than over SFTP", async () => {
    know(on("ftp", server("f1")));
    open({ id: "files", kind: "remote", sessionId: "f1", path: "/srv/www" }, page("index", "f1"));
    const closing = requestCloseTab("files");

    expect(useUiStore.getState().dialog).toEqual({
      kind: "unsavedChanges",
      tabIds: ["index"],
      closeWindow: false,
    });
    discard("index");
    await closing;
    expect(tabIds()).toEqual([]);
    expect(disconnected()).toEqual(["f1"]);
  });

  it("keeps the server when the question is cancelled", async () => {
    know(on("webDav", server("w1")));
    open({ id: "files", kind: "remote", sessionId: "w1", path: "/" }, page("index", "w1"));
    const closing = requestCloseTab("files");
    useUiStore.getState().close();
    await closing;

    expect(tabIds()).toEqual(["files", "index"]);
    expect(disconnected()).toEqual([]);
    expect(useUiStore.getState().dialog).toBeNull();
  });

  it("asks only about files in the vaults of an SFTP server, whose own files save anyway", async () => {
    know(server("s1"), vault("v1", "s1", "/srv/private"), on("ftp", server("f1")));
    open(
      { id: "files", kind: "remote", sessionId: "s1", path: "/srv" },
      page("index", "s1"),
      page("letter", "v1"),
      page("elsewhere", "f1"),
    );
    const closing = requestCloseTab("files");

    expect(useUiStore.getState().dialog).toMatchObject({ tabIds: ["letter"] });
    discard("letter");
    await closing;
    expect(tabIds()).toEqual(["index", "elsewhere"]);
    expect(disconnected()).toEqual(["s1"]);
  });

  it("asks about the files in a vault before locking it", async () => {
    know(server("s1"), vault("v1", "s1", "/srv/private"));
    open(
      { id: "secret", kind: "remote", sessionId: "v1", path: "/srv/private" },
      page("letter", "v1"),
    );
    const locking = requestLockVault("v1");

    expect(useUiStore.getState().dialog).toMatchObject({ tabIds: ["letter"] });
    expect(disconnected()).toEqual([]);
    discard("letter");
    await locking;
    expect(disconnected()).toEqual(["v1"]);
    expect(allTabs(useLayoutStore.getState().root)).toEqual([
      { id: "secret", kind: "remote", sessionId: "s1", path: "/srv/private" },
    ]);
  });
});

describe("closing the other tabs", () => {
  beforeEach(() => {
    useUiStore.setState({ dialog: null });
    invoke.mockImplementation(async (command: string) =>
      command === "transfer_session_jobs" ? 2 : undefined,
    );
  });

  /** Lets the closing go on to its next question. */
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
  const asked = () => useUiStore.getState().dialog;
  /** What "Keep transferring" does: the server's tab closes, then the question does. */
  const keepTransferring = (tabId: string, sessionId: string) => {
    closeRemoteTab(tabId, sessionId);
    useUiStore.getState().close();
  };

  it("asks about each server with transfers in turn", async () => {
    know(on("ftp", server("f1")), on("ftp", server("f2")));
    open(
      { id: "first", kind: "remote", sessionId: "f1", path: "/srv" },
      { id: "second", kind: "remote", sessionId: "f2", path: "/srv" },
    );
    const closing = requestCloseTabs(["first", "second"]);
    await settle();

    expect(asked()).toMatchObject({ kind: "closeTab", tabId: "first", pendingTransfers: 2 });
    keepTransferring("first", "f1");
    await settle();
    expect(asked()).toMatchObject({ kind: "closeTab", tabId: "second" });
    keepTransferring("second", "f2");
    await closing;
    expect(tabIds()).toEqual([]);
    expect(disconnected()).toEqual(["f1", "f2"]);
  });

  it("stops at the first tab the user keeps", async () => {
    know(on("ftp", server("f1")), on("ftp", server("f2")));
    open(
      { id: "first", kind: "remote", sessionId: "f1", path: "/srv" },
      { id: "second", kind: "remote", sessionId: "f2", path: "/srv" },
    );
    const closing = requestCloseTabs(["first", "second"]);
    await settle();
    useUiStore.getState().close();
    await closing;

    expect(asked()).toBeNull();
    expect(tabIds()).toEqual(["first", "second"]);
    expect(disconnected()).toEqual([]);
  });

  it("asks about every unsaved file in one question before the servers", async () => {
    know(on("ftp", server("f1")));
    open(
      page("index", "f1"),
      { id: "files", kind: "remote", sessionId: "f1", path: "/srv" },
      page("about", "f1"),
    );
    const closing = requestCloseTabs(["index", "files", "about"]);

    expect(asked()).toEqual({
      kind: "unsavedChanges",
      tabIds: ["index", "about"],
      closeWindow: false,
    });
    discard("index", "about");
    await settle();
    expect(asked()).toMatchObject({ kind: "closeTab", tabId: "files" });
    keepTransferring("files", "f1");
    await closing;
    expect(tabIds()).toEqual([]);
  });
});
