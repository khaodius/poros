import { afterEach, describe, expect, it } from "vitest";
import type { FileEntry, Protocol } from "../lib/types";
import { dropAction } from "./fileOperations";
import { registerPane, type PaneHandle } from "./paneRegistry";
import { useSessionStore } from "./sessionStore";

function entry(path: string, kind: FileEntry["kind"] = "file"): FileEntry {
  return {
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind,
    size: 0,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

const unregister: (() => void)[] = [];

function pane(
  tabId: string,
  kind: PaneHandle["kind"],
  path: string,
  sessionId?: string,
  protocol: Protocol = "sftp",
) {
  if (sessionId) {
    const info = {
      id: sessionId,
      label: sessionId,
      host: `${sessionId}.example.com`,
      port: 22,
      username: "deploy",
      home: "/",
      initialPath: "/",
      protocol,
    };
    useSessionStore.setState((state) => ({
      sessions: { ...state.sessions, [sessionId]: { info, profile: null, status: "connected" } },
    }));
  }
  unregister.push(
    registerPane({
      tabId,
      kind,
      sessionId,
      label: tabId,
      path: () => path,
      refresh: () => undefined,
      selected: () => [],
      visible: true,
      activatedAt: 0,
    }),
  );
}

/** A pane inside the vault unlocked at `root` on the server of `parentSessionId`. */
function vaultPane(
  tabId: string,
  path: string,
  sessionId: string,
  parentSessionId: string,
  root: string,
) {
  pane(tabId, "remote", path, sessionId, "cryptomator");
  useSessionStore.setState((state) => {
    const entry = state.sessions[sessionId];
    const info = {
      ...entry.info,
      host: `${parentSessionId}.example.com`,
      vault: { parentSessionId, root, name: root.slice(root.lastIndexOf("/") + 1) },
    };
    return { sessions: { ...state.sessions, [sessionId]: { ...entry, info } } };
  });
}

afterEach(() => {
  unregister.splice(0).forEach((remove) => remove());
  useSessionStore.setState({ sessions: {} });
});

const onPane = (tabId: string, folder: string | null) => ({
  kind: "pane" as const,
  tabId,
  folder,
});

describe("dropAction", () => {
  it("moves within a server, or copies with the copy key", () => {
    pane("server", "remote", "/srv/www", "s1");
    const files = [entry("/srv/www/index.html")];
    expect(dropAction("server", files, onPane("server", "/srv/www/assets"), false)).toMatchObject({
      kind: "place",
      mode: "move",
      folder: "/srv/www/assets",
    });
    expect(dropAction("server", files, onPane("server", "/srv/www/assets"), true)).toMatchObject({
      kind: "place",
      mode: "copy",
    });
  });

  it("does nothing where the files already are", () => {
    pane("server", "remote", "/srv/www", "s1");
    pane("same", "remote", "/srv/www", "s1");
    const files = [entry("/srv/www/index.html")];
    expect(dropAction("server", files, onPane("server", null), false)).toBeNull();
    expect(dropAction("server", files, onPane("same", null), false)).toBeNull();
    expect(dropAction("server", files, onPane("same", null), true)).toMatchObject({
      kind: "place",
      mode: "copy",
    });
  });

  it("keeps folders out of themselves", () => {
    pane("server", "remote", "/srv", "s1");
    pane("inside", "remote", "/srv/www/assets", "s1");
    const folder = [entry("/srv/www", "dir")];
    expect(dropAction("server", folder, onPane("server", "/srv/www"), false)).toBeNull();
    expect(dropAction("server", folder, onPane("inside", null), false)).toMatchObject({
      kind: "blocked",
    });
  });

  it("transfers between this computer and a server", () => {
    pane("local", "local", "/home/me");
    pane("server", "remote", "/srv/www", "s1");
    const files = [entry("/home/me/notes.txt")];
    expect(dropAction("local", files, onPane("server", null), false)).toEqual({
      kind: "transfer",
      direction: "upload",
      folder: "/srv/www",
    });
  });

  it("copies between two servers through the transfer queue", () => {
    pane("first", "remote", "/srv", "s1");
    pane("second", "remote", "/srv", "s2");
    expect(dropAction("first", [entry("/srv/a.txt")], onPane("second", null), false)).toEqual({
      kind: "transfer",
      direction: "relay",
      folder: "/srv",
    });
  });

  it("copies into an unlocked vault through the vault, also from its server's pane", () => {
    pane("server", "remote", "/srv", "s1");
    pane("beside", "remote", "/srv", "s1");
    vaultPane("vault", "/srv/private/letters", "v1", "s1", "/srv/private");
    const files = [entry("/srv/a.txt")];
    const relay = { kind: "transfer", direction: "relay" };
    expect(dropAction("server", files, onPane("vault", null), false)).toEqual({
      ...relay,
      folder: "/srv/private/letters",
    });
    expect(dropAction("server", files, onPane("server", "/srv/private"), false)).toEqual({
      ...relay,
      folder: "/srv/private",
    });
    expect(dropAction("beside", files, onPane("server", "/srv/private"), false)).toEqual({
      ...relay,
      folder: "/srv/private",
    });
    // A folder beside the vault is the server's own.
    expect(dropAction("server", files, onPane("server", "/srv/public"), false)).toMatchObject({
      kind: "place",
    });
  });

  it("copies out of a vault to its server", () => {
    pane("server", "remote", "/srv", "s1");
    vaultPane("vault", "/srv/private", "v1", "s1", "/srv/private");
    const files = [entry("/srv/private/letter.txt")];
    expect(dropAction("vault", files, onPane("server", null), false)).toEqual({
      kind: "transfer",
      direction: "relay",
      folder: "/srv",
    });
  });

  it("keeps the folder a vault is in out of the vault", () => {
    pane("server", "remote", "/srv", "s1");
    pane("above", "remote", "/", "s1");
    vaultPane("vault", "/srv/private/letters", "v1", "s1", "/srv/private");
    const vaultFolder = [entry("/srv/private", "dir")];
    expect(dropAction("server", vaultFolder, onPane("server", "/srv/private"), false)).toBeNull();
    expect(dropAction("server", vaultFolder, onPane("vault", null), false)).toMatchObject({
      kind: "blocked",
    });
    expect(dropAction("above", [entry("/srv", "dir")], onPane("vault", null), false)).toMatchObject(
      { kind: "blocked" },
    );
  });

  it("moves nothing inside a vault", () => {
    vaultPane("vault", "/srv/private", "v1", "s1", "/srv/private");
    vaultPane("other", "/srv/private/letters", "v1", "s1", "/srv/private");
    const files = [entry("/srv/private/a.txt")];
    expect(dropAction("vault", files, onPane("vault", "/srv/private/letters"), false)).toEqual({
      kind: "blocked",
      reason: "Files cannot be moved inside a vault",
    });
    expect(dropAction("vault", files, onPane("other", null), false)).toMatchObject({
      kind: "transfer",
      direction: "relay",
    });
  });

  it("moves nothing on a server reached other than over SFTP", () => {
    pane("ftp", "remote", "/srv", "s1", "ftp");
    pane("other", "remote", "/srv/www", "s1", "ftp");
    const files = [entry("/srv/a.txt")];
    expect(dropAction("ftp", files, onPane("ftp", "/srv/www"), false)).toMatchObject({
      kind: "blocked",
    });
    expect(dropAction("ftp", files, onPane("other", null), false)).toMatchObject({
      kind: "transfer",
      direction: "relay",
    });
  });
});
