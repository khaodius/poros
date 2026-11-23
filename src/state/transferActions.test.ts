import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FileEntry, SessionInfo } from "../lib/types";
import type { PaneHandle } from "./paneRegistry";
import { useSessionStore } from "./sessionStore";
import { useToastStore } from "./toastStore";
import { refusesLockedVault, transferBetween } from "./transferActions";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

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
      infos.map((info) => [info.id, { info, profile: null, status: "connected" as const }]),
    ),
  });
}

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

function pane(kind: PaneHandle["kind"], path: string, more: Partial<PaneHandle> = {}): PaneHandle {
  return {
    tabId: `${kind}-tab`,
    kind,
    label: kind,
    path: () => path,
    refresh: () => undefined,
    selected: () => [],
    visible: true,
    activatedAt: 0,
    ...more,
  };
}

/** What the server lists, by folder; a folder not named here cannot be listed. */
let folders: Record<string, FileEntry[]> = {};
const toasts: string[] = [];
const commands = () => invoke.mock.calls.map(([command]) => command as string);

beforeEach(() => {
  folders = {};
  toasts.length = 0;
  invoke.mockReset();
  invoke.mockImplementation((command: string, args: { path?: string }) => {
    if (command !== "remote_list") return Promise.resolve(1);
    const entries = folders[args.path ?? ""] as FileEntry[] | undefined;
    if (!entries) return Promise.reject(new Error("No such folder"));
    return Promise.resolve({ path: args.path, entries });
  });
  useToastStore.setState({ show: (_tone, message) => void toasts.push(message) });
  know(server("s1"));
});

describe("refusesLockedVault", () => {
  const onServer = pane("remote", "/srv", { sessionId: "s1" });

  it("refuses a folder that holds a vault nobody unlocked", async () => {
    folders["/srv/private"] = [
      entry("/srv/private/vault.cryptomator"),
      entry("/srv/private/masterkey.cryptomator"),
      entry("/srv/private/d", "dir"),
    ];
    expect(await refusesLockedVault(onServer, "/srv/private")).toBe(true);
    expect(toasts).toEqual(["private is a locked vault. Open it to copy files into it."]);
  });

  it("refuses a vault in a format too old to open", async () => {
    folders["/srv/old"] = [entry("/srv/old/masterkey.cryptomator"), entry("/srv/old/d", "dir")];
    expect(await refusesLockedVault(onServer, "/srv/old")).toBe(true);
  });

  it("lets files into any other folder", async () => {
    folders["/srv/www"] = [entry("/srv/www/index.html"), entry("/srv/www/d", "dir")];
    expect(await refusesLockedVault(onServer, "/srv/www")).toBe(false);
    expect(toasts).toEqual([]);
  });

  it("leaves a folder that cannot be listed to the copy", async () => {
    expect(await refusesLockedVault(onServer, "/srv/missing")).toBe(false);
    expect(toasts).toEqual([]);
  });

  it("goes by the card where the pane shows the vault's own folder", async () => {
    const locked = pane("remote", "/srv/private", { sessionId: "s1", locked: true });
    expect(await refusesLockedVault(locked, "/srv/private")).toBe(true);
    expect(toasts).toEqual(["Unlock the vault to copy files into it."]);
    // With the encrypted files shown instead of the card, the user is working on the server.
    const revealed = pane("remote", "/srv/private", { sessionId: "s1" });
    expect(await refusesLockedVault(revealed, "/srv/private")).toBe(false);
    expect(commands()).toEqual([]);
  });

  it("asks nothing inside an unlocked vault or on this computer", async () => {
    know(server("s1"), vault("v1", "s1", "/srv/private"));
    const inVault = pane("remote", "/srv/private", { sessionId: "v1" });
    expect(await refusesLockedVault(inVault, "/srv/private/photos")).toBe(false);
    expect(await refusesLockedVault(pane("local", "/home/me"), "/home/me/photos")).toBe(false);
    expect(commands()).toEqual([]);
  });
});

describe("transferBetween", () => {
  const local = pane("local", "/home/me");
  const files = [entry("/home/me/notes.txt")];

  it("queues nothing for a locked vault's row in the folder the vault is in", async () => {
    folders["/srv/private"] = [entry("/srv/private/vault.cryptomator")];
    const onServer = pane("remote", "/srv", { sessionId: "s1" });
    await transferBetween(local, files, onServer, "/srv/private");
    expect(commands()).toEqual(["remote_list"]);
  });

  it("uploads into the vault's session once the vault is unlocked", async () => {
    know(server("s1"), vault("v1", "s1", "/srv/private"));
    const onServer = pane("remote", "/srv", { sessionId: "s1" });
    await transferBetween(local, files, onServer, "/srv/private");
    expect(invoke).toHaveBeenCalledWith("transfer_enqueue", {
      request: expect.objectContaining({ sessionId: "v1", targetDirectory: "/srv/private" }),
    });
    expect(commands()).not.toContain("remote_list");
  });
});
