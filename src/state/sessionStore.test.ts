import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { allTabs, group, type PaneTab } from "../lib/layout";
import type { ConnectProfile, SessionInfo } from "../lib/types";
import { closeEditorTab } from "./editorActions";
import { useLayoutStore } from "./layoutStore";
import { registerPane } from "./paneRegistry";
import { savedServerOf, serverEntry, useSessionStore, vaultOf } from "./sessionStore";
import { closeRemoteTab } from "./tabActions";
import { closeTerminalTab } from "./terminalActions";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

const profile: ConnectProfile = {
  protocol: "sftp",
  host: "web.example.com",
  port: 22,
  username: "deploy",
  auth: { type: "agent" },
};

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

function know(...infos: SessionInfo[]) {
  useSessionStore.setState({
    sessions: Object.fromEntries(
      infos.map((info) => [
        info.id,
        { info, profile: info.vault ? null : profile, status: "connected" as const },
      ]),
    ),
  });
}

function show(...tabs: PaneTab[]) {
  useLayoutStore.getState().setRoot(group(tabs));
}

const sessionIds = () => Object.keys(useSessionStore.getState().sessions).sort();

const unregister: (() => void)[] = [];

beforeEach(() => {
  invoke.mockReset();
  useSessionStore.setState({ sessions: {} });
});

afterEach(() => unregister.splice(0).forEach((remove) => remove()));

describe("unlocked vaults", () => {
  it("keeps an unlocked vault as a session beside its server's, without its password", async () => {
    know(server("s1"));
    invoke.mockResolvedValue(vault("v1", "s1", "/srv/private"));
    const info = await useSessionStore.getState().unlockVault("s1", "/srv/private", "open sesame");

    expect(invoke).toHaveBeenCalledWith("vault_unlock", {
      sessionId: "s1",
      path: "/srv/private",
      password: "open sesame",
    });
    expect(info.id).toBe("v1");
    expect(sessionIds()).toEqual(["s1", "v1"]);
    expect(useSessionStore.getState().sessions.v1).toMatchObject({
      profile: null,
      status: "connected",
    });
    expect(vaultOf("v1")).toEqual({ parentSessionId: "s1", root: "/srv/private", name: "private" });
    expect(vaultOf("s1")).toBeNull();
    expect(JSON.stringify(useSessionStore.getState())).not.toContain("open sesame");
  });

  it("keeps a new vault the same way", async () => {
    know(server("s1"));
    invoke.mockResolvedValue(vault("v1", "s1", "/srv/ledger"));
    await useSessionStore.getState().createVault("s1", "/srv", "ledger", "open sesame");

    expect(invoke).toHaveBeenCalledWith("vault_create", {
      sessionId: "s1",
      parent: "/srv",
      name: "ledger",
      password: "open sesame",
    });
    expect(sessionIds()).toEqual(["s1", "v1"]);
    expect(JSON.stringify(useSessionStore.getState())).not.toContain("open sesame");
  });

  it("leaves the store alone when the password is wrong", async () => {
    know(server("s1"));
    invoke.mockRejectedValue({ kind: "wrongPassword", message: "Wrong password" });
    await expect(
      useSessionStore.getState().unlockVault("s1", "/srv/private", "guess"),
    ).rejects.toMatchObject({ kind: "wrongPassword" });
    expect(sessionIds()).toEqual(["s1"]);
  });

  it("reaches a vault over its server, which is the one to connect to again", () => {
    know(server("s1"), vault("v1", "s1", "/srv/private"));
    const { sessions } = useSessionStore.getState();
    expect(serverEntry(sessions, "v1")).toBe(sessions.s1);
    expect(serverEntry(sessions, "s1")).toBe(sessions.s1);
    expect(serverEntry(sessions, "gone")).toBeUndefined();
    expect(savedServerOf("s1")).toMatchObject({ protocol: "sftp" });
    expect(savedServerOf("v1")).toBeNull();
  });

  it("locks one vault alone", async () => {
    know(server("s1"), vault("v1", "s1", "/srv/private"), vault("v2", "s1", "/srv/ledger"));
    await useSessionStore.getState().disconnect("v1");
    expect(sessionIds()).toEqual(["s1", "v2"]);
    expect(invoke).toHaveBeenCalledWith("disconnect", { sessionId: "v1" });
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("closes a server's vaults with the server", async () => {
    know(
      server("s1"),
      vault("v1", "s1", "/srv/private"),
      server("s2"),
      vault("v2", "s2", "/srv/private"),
    );
    await useSessionStore.getState().disconnect("s1");
    expect(sessionIds()).toEqual(["s2", "v2"]);
    // The backend closes the vaults of a server it disconnects.
    expect(invoke).toHaveBeenCalledWith("disconnect", { sessionId: "s1" });
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("lets a server go to another window with its vaults, closing nothing", () => {
    know(server("s1"), vault("v1", "s1", "/srv/private"), server("s2"));
    useSessionStore.getState().release("s1");
    expect(sessionIds()).toEqual(["s2"]);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("keeps the vaults of a server that reconnects, under its new session", async () => {
    know(server("s1"), vault("v1", "s1", "/srv/private"), server("s2"));
    show(
      { id: "files", kind: "remote", sessionId: "s1", path: "/srv/public" },
      { id: "secret", kind: "remote", sessionId: "v1", path: "/srv/private/letters" },
      { id: "other", kind: "remote", sessionId: "s2", path: "/srv" },
    );
    const refresh = vi.fn();
    unregister.push(
      registerPane({
        tabId: "secret",
        kind: "remote",
        sessionId: "v1",
        label: "private",
        path: () => "/srv/private/letters",
        refresh,
        selected: () => [],
        visible: true,
        activatedAt: 0,
      }),
    );
    invoke.mockImplementation(async (command: string) =>
      command === "reconnect" ? server("s3") : undefined,
    );
    useSessionStore.getState().markLost("s1", "The connection was lost.");
    const info = await useSessionStore.getState().reconnect("s1");

    expect(info.id).toBe("s3");
    expect(sessionIds()).toEqual(["s2", "s3", "v1"]);
    expect(vaultOf("v1")).toEqual({ parentSessionId: "s3", root: "/srv/private", name: "private" });
    expect(useSessionStore.getState().sessions.s3).toMatchObject({ profile, status: "connected" });
    expect(allTabs(useLayoutStore.getState().root)).toEqual([
      { id: "files", kind: "remote", sessionId: "s3", path: "/srv/public" },
      { id: "secret", kind: "remote", sessionId: "v1", path: "/srv/private/letters" },
      { id: "other", kind: "remote", sessionId: "s2", path: "/srv" },
    ]);
    // A vault's session stays, so its pane is told to list again.
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalledWith(
      "reconnect",
      expect.objectContaining({ sessionId: "v1" }),
    );
  });
});

const page = (id: string, sessionId: string): PaneTab => ({
  id,
  kind: "editor",
  documentId: `document-${id}`,
  name: "index.html",
  path: "/srv/www/index.html",
  origin: "deploy@web.example.com",
  sessionId,
});

const disconnected = () =>
  invoke.mock.calls.flatMap(([command, args]) =>
    command === "disconnect" ? [(args as { sessionId: string }).sessionId] : [],
  );

describe("a server that reconnects", () => {
  it("takes the editors and terminals opened from it along to its new session", async () => {
    know(server("s1"), server("s2"));
    show(
      { id: "files", kind: "remote", sessionId: "s1", path: "/srv/www" },
      page("page", "s1"),
      { id: "shell", kind: "terminal", sessionId: "s1", label: "deploy@web.example.com" },
      page("elsewhere", "s2"),
    );
    invoke.mockImplementation(async (command: string) =>
      command === "reconnect" ? server("s3") : undefined,
    );
    await useSessionStore.getState().reconnect("s1");

    expect(allTabs(useLayoutStore.getState().root)).toEqual([
      { id: "files", kind: "remote", sessionId: "s3", path: "/srv/www" },
      page("page", "s3"),
      { id: "shell", kind: "terminal", sessionId: "s3", label: "deploy@web.example.com" },
      page("elsewhere", "s2"),
    ]);
  });

  /** Holds the reconnect until `finish` is called. */
  function reconnectHeld(): { reconnecting: Promise<SessionInfo>; finish: () => void } {
    let finish: () => void = () => undefined;
    invoke.mockImplementation((command: string) =>
      command === "reconnect"
        ? new Promise<SessionInfo>((resolve) => (finish = () => resolve(server("s3"))))
        : Promise.resolve(),
    );
    return { reconnecting: useSessionStore.getState().reconnect("s1"), finish: () => finish() };
  }

  it("closes the session it opened when the tab was closed meanwhile", async () => {
    know(server("s1"));
    show({ id: "files", kind: "remote", sessionId: "s1", path: "/srv/www" });
    const { reconnecting, finish } = reconnectHeld();
    closeRemoteTab("files", "s1");
    finish();

    await expect(reconnecting).rejects.toMatchObject({ kind: "sessionNotFound" });
    expect(sessionIds()).toEqual([]);
    expect(disconnected()).toEqual(["s1", "s3"]);
  });

  it("leaves the session alone that went to another window meanwhile", async () => {
    know(server("s1"));
    show({ id: "files", kind: "remote", sessionId: "s1", path: "/srv/www" });
    const { reconnecting, finish } = reconnectHeld();
    useSessionStore.getState().release("s1");
    finish();

    await expect(reconnecting).rejects.toMatchObject({ kind: "sessionNotFound" });
    expect(sessionIds()).toEqual([]);
    expect(disconnected()).toEqual(["s3"]);
  });
});

describe("a session no file tab shows", () => {
  it("closes with the last editor or terminal tab that used it", () => {
    know(server("s1"), server("s2"));
    show(
      page("page", "s1"),
      { id: "shell", kind: "terminal", sessionId: "s1", label: "deploy@web.example.com" },
      { id: "files", kind: "remote", sessionId: "s2", path: "/srv" },
      page("elsewhere", "s2"),
    );
    closeEditorTab("page");
    expect(disconnected()).toEqual([]);
    closeTerminalTab("shell");
    expect(disconnected()).toEqual(["s1"]);
    closeEditorTab("elsewhere");
    expect(sessionIds()).toEqual(["s2"]);
  });

  it("stays while a tab shows one of the vaults unlocked on it", () => {
    know(server("s1"), vault("v1", "s1", "/srv/private"));
    show(page("page", "s1"), {
      id: "secret",
      kind: "remote",
      sessionId: "v1",
      path: "/srv/private",
    });
    closeEditorTab("page");
    expect(disconnected()).toEqual([]);
    expect(sessionIds()).toEqual(["s1", "v1"]);
  });
});
