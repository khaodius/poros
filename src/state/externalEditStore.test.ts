import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS } from "../lib/settings";
import type { ExternalEdit, SessionInfo } from "../lib/types";
import {
  closePastEdits,
  editsHoldClose,
  editsHoldExit,
  editsHoldLock,
  exitNow,
  openWithDefault,
  useExternalEditStore,
  type OpenTarget,
} from "./externalEditStore";
import { useLogStore } from "./logStore";
import { useSessionStore } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";
import { requestLockVault } from "./vaultActions";
import { performNow } from "./whenDone";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));
const closeWindow = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: closeWindow, setFocus: async () => undefined }),
  getAllWindows: async () => [],
}));

function onServer(name: string): OpenTarget {
  return { side: "remote", sessionId: "s1", path: `/var/www/${name}`, name };
}

function editing(id: string, sessionId: string, state: ExternalEdit["state"]): ExternalEdit {
  return {
    id,
    name: `${id}.txt`,
    remotePath: `/srv/${id}.txt`,
    sessionId,
    server: "files.example",
    inVault: sessionId === "vault",
    outlivesSession: sessionId !== "vault",
    localPath: `/tmp/${id}.txt`,
    program: "VS Code",
    state,
    transferred: 0,
    size: 4,
    blocked: false,
    uploads: 0,
  };
}

/** The commands sent, by name. */
const commands = () => invoke.mock.calls.map(([command]) => command as string);

/** The request each call to open a server file sent. */
const openRequests = () =>
  invoke.mock.calls.flatMap(([command, args]) =>
    command === "external_edit_open" ? [(args as { request: unknown }).request] : [],
  );

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue({ kind: "opened" });
  closeWindow.mockReset();
  closeWindow.mockResolvedValue(undefined);
  useExternalEditStore.setState({ edits: [], closing: null, exiting: null });
  useSettingsStore.setState({
    settings: {
      ...DEFAULT_SETTINGS,
      editing: {
        ...DEFAULT_SETTINGS.editing,
        programs: [
          {
            id: "code",
            name: "VS Code",
            command: "/usr/bin/code",
            arguments: "--wait {file}",
            patterns: ["*.php"],
          },
        ],
      },
    },
  });
});

describe("Open", () => {
  it("opens a script in the program set for its kind without asking again", async () => {
    await openWithDefault(onServer("index.php"));
    expect(openRequests()).toEqual([
      {
        sessionId: "s1",
        path: "/var/www/index.php",
        with: { kind: "program", id: "code" },
        confirmedExecutable: true,
      },
    ]);
  });

  it("leaves the system's own program to ask about a file of a type that runs", async () => {
    await openWithDefault(onServer("deploy.sh"));
    expect(openRequests()).toEqual([
      {
        sessionId: "s1",
        path: "/var/www/deploy.sh",
        with: { kind: "default" },
        confirmedExecutable: false,
      },
    ]);
  });
});

describe("Closing with files open in other programs", () => {
  it("asks before a vault locks, and asks again once that lock is done", async () => {
    useExternalEditStore.setState({ edits: [editing("notes", "vault", "watching")] });
    expect(editsHoldLock("vault", "Private")).toBe(true);
    const prompt = useExternalEditStore.getState().closing;
    expect(prompt).toEqual({
      kind: "lockVault",
      sessionId: "vault",
      label: "Private",
      sessionIds: ["vault"],
    });
    let heldDuring: boolean | null = null;
    await closePastEdits(prompt!, async () => {
      heldDuring = editsHoldLock("vault", "Private");
    });
    expect(heldDuring).toBe(false);
    expect(editsHoldLock("vault", "Private")).toBe(true);
    // A vault with nothing open in other programs locks unasked.
    expect(editsHoldLock("other-vault", "Other")).toBe(false);
  });

  it("does not lock a vault from its tab before the user answers", async () => {
    const info: SessionInfo = {
      id: "vault",
      label: "Private on deploy@web.example.com",
      host: "web.example.com",
      port: 22,
      username: "deploy",
      home: "/srv/private",
      initialPath: "/srv/private",
      protocol: "cryptomator",
      vault: { parentSessionId: "server", root: "/srv/private", name: "Private" },
    };
    useSessionStore.setState({ sessions: { vault: { info, profile: null, status: "connected" } } });
    useLogStore.setState({ write: () => undefined });
    useExternalEditStore.setState({ edits: [editing("notes", "vault", "watching")] });
    await requestLockVault("vault");
    expect(useExternalEditStore.getState().closing?.kind).toBe("lockVault");
    expect(useSessionStore.getState().sessions.vault).toBeDefined();
    expect(commands()).toEqual([]);

    const prompt = useExternalEditStore.getState().closing!;
    await closePastEdits(prompt, () => requestLockVault("vault"));
    expect(useSessionStore.getState().sessions.vault).toBeUndefined();
    expect(commands()).toContain("disconnect");
  });

  it("takes back a tab's close the user agreed to when that close does not happen", async () => {
    useExternalEditStore.setState({ edits: [editing("index", "server", "changed")] });
    expect(editsHoldClose("tab-1", "Web server", ["server"])).toBe(true);
    const prompt = useExternalEditStore.getState().closing!;
    // The close stops at another question the user then cancels.
    await closePastEdits(prompt, async () => undefined);
    expect(editsHoldClose("tab-1", "Web server", ["server"])).toBe(true);
    await expect(
      closePastEdits(prompt, () => Promise.reject(new Error("Could not close"))),
    ).rejects.toThrow("Could not close");
    expect(editsHoldClose("tab-1", "Web server", ["server"])).toBe(true);
  });
});

describe("Quitting with saves not uploaded", () => {
  beforeEach(() => {
    invoke.mockImplementation(async (command: string) =>
      command === "external_edit_unsaved" ? [editing("index", "server", "changed")] : undefined,
    );
  });

  it("asks again after a quit the user agreed to does not happen", async () => {
    expect(await editsHoldExit()).toBe(true);
    expect(useExternalEditStore.getState().exiting).toEqual(["index"]);
    exitNow();
    expect(closeWindow).toHaveBeenCalledTimes(1);
    // The window's close asks once and is let through.
    expect(await editsHoldExit()).toBe(false);
    // That quit stopped at the question about the built-in editor's unsaved files.
    expect(await editsHoldExit()).toBe(true);
  });

  it("asks again when the window could not close", async () => {
    const toasts: string[] = [];
    useToastStore.setState({ show: (_tone, message) => toasts.push(message) });
    closeWindow.mockRejectedValueOnce(new Error("The window is busy"));
    exitNow();
    await vi.waitFor(() => expect(toasts).toEqual(["The window is busy"]));
    expect(await editsHoldExit()).toBe(true);
  });

  it("closes Poros when the queue is done only once those saves are dealt with", async () => {
    await performNow("close");
    expect(useExternalEditStore.getState().exiting).toEqual(["index"]);
    expect(commands()).not.toContain("app_exit");

    invoke.mockImplementation(async (command: string) =>
      command === "external_edit_unsaved" ? [] : undefined,
    );
    await performNow("close");
    expect(commands()).toContain("app_exit");
  });
});
