import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DriveView, ExternalEdit } from "../lib/types";
import { exitNow, useExternalEditStore } from "./externalEditStore";
import { useMountStore } from "./mountStore";
import { quitWaits } from "./quitGuard";
import { useUiStore } from "./uiStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));
const closeWindow = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ close: closeWindow, setFocus: async () => undefined }),
  getAllWindows: async () => [],
}));

const savedElsewhere: ExternalEdit = {
  id: "index",
  name: "index.html",
  remotePath: "/var/www/index.html",
  sessionId: "server",
  server: "web.example.com",
  inVault: false,
  outlivesSession: true,
  localPath: "/tmp/index.html",
  program: "VS Code",
  state: "changed",
  transferred: 0,
  size: 4,
  blocked: false,
  uploads: 0,
};

const inUse: DriveView = {
  id: "drive",
  sessionId: "server",
  label: "deploy@web.example.com",
  protocol: "sftp",
  mountPoint: "/home/me/Poros/web",
  remoteRoot: "/var/www",
  readOnly: false,
  bytesRead: 0,
  bytesWritten: 0,
  openFiles: ["notes.txt"],
  uploads: [],
};

/** Answers the questions quitting asks with these files and drives. */
function serverHolds(edits: ExternalEdit[], drives: DriveView[]) {
  invoke.mockImplementation(async (command: string) => {
    if (command === "external_edit_unsaved") return edits;
    if (command === "mount_list") return drives;
    if (command === "mount_unsent") return [];
    return undefined;
  });
}

const quitAsked = () => useUiStore.getState().dialog?.kind === "quit";

beforeEach(() => {
  invoke.mockReset();
  closeWindow.mockReset();
  closeWindow.mockResolvedValue(undefined);
  useExternalEditStore.setState({ edits: [savedElsewhere], closing: null, exiting: null });
  useMountStore.setState({ drives: [], unsent: [] });
  useUiStore.setState({ dialog: null });
});

describe("quitWaits", () => {
  it("asks about saves in other programs first, then about drives in use, once each", async () => {
    serverHolds([savedElsewhere], [inUse]);
    expect(await quitWaits()).toBe(true);
    expect(useExternalEditStore.getState().exiting).toEqual(["index"]);
    expect(quitAsked()).toBe(false);

    // Agreeing closes the window again, which comes back with the drives' question.
    exitNow();
    expect(closeWindow).toHaveBeenCalledTimes(1);
    expect(await quitWaits()).toBe(true);
    expect(useUiStore.getState().dialog).toEqual({ kind: "quit", drives: [inUse.mountPoint] });
  });

  it("asks about drives in use alone when no save waits", async () => {
    serverHolds([], [inUse]);
    expect(await quitWaits()).toBe(true);
    expect(useExternalEditStore.getState().exiting).toBeNull();
    expect(quitAsked()).toBe(true);
  });

  it("lets the quit go on when nothing is in use", async () => {
    serverHolds([], [{ ...inUse, openFiles: [] }]);
    expect(await quitWaits()).toBe(false);
    expect(quitAsked()).toBe(false);
  });
});
