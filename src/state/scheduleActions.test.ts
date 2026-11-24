import { beforeEach, describe, expect, it, vi } from "vitest";
import { newTask, transferAction } from "../lib/schedule";
import type { SessionInfo, TaskAction, TaskView } from "../lib/types";
import { forgetWhatRunsChanged } from "./scheduleActions";
import { useSessionStore } from "./sessionStore";

const forgotten = vi.hoisted(() => ({
  within: vi.fn<(sessionId: string, folder: string) => void>(),
  server: vi.fn<(sessionId: string) => void>(),
}));
vi.mock("./listings", () => ({
  forgetListingsWithin: forgotten.within,
  forgetServerListings: forgotten.server,
}));

function session(id: string, savedConnectionId?: string, vault = false): SessionInfo {
  return {
    id,
    label: id,
    host: "web.example.com",
    port: 22,
    username: "deploy",
    home: "/home/deploy",
    initialPath: "/home/deploy",
    protocol: vault ? "cryptomator" : "sftp",
    savedConnectionId,
    ...(vault
      ? { vault: { parentSessionId: "s1", root: "/var/www/private", name: "private" } }
      : {}),
  };
}

function task(id: string, action: TaskAction, finished: number | null): TaskView {
  return {
    ...newTask(action),
    id,
    running: false,
    lastRun:
      finished === null ? null : { started: finished - 5, finished, succeeded: true, message: "" },
  };
}

describe("forgetWhatRunsChanged", () => {
  beforeEach(() => {
    forgotten.within.mockClear();
    forgotten.server.mockClear();
    useSessionStore.setState({
      sessions: Object.fromEntries(
        [session("s1", "web"), session("s2", "nas"), session("v1", "web", true)].map((info) => [
          info.id,
          { info, profile: null, status: "connected" as const },
        ]),
      ),
    });
  });

  it("forgets the folders finished runs wrote into, on sessions of the same connection", () => {
    const upload = transferAction("web", "upload", { destination: " /var/www " });
    const command: TaskAction = { type: "command", connectionId: "nas", command: "make" };
    const download = transferAction("web", "download", { destination: "/home/me" });
    forgetWhatRunsChanged(
      [task("up", upload, 100), task("cmd", command, null), task("down", download, 100)],
      [task("up", upload, 200), task("cmd", command, 200), task("down", download, 200)],
    );
    expect(forgotten.within.mock.calls).toEqual([["s1", "/var/www"]]);
    expect(forgotten.server.mock.calls).toEqual([["s2"]]);
  });

  it("leaves listings alone when no run finished", () => {
    const upload = transferAction("web", "upload", { destination: "/var/www" });
    forgetWhatRunsChanged([task("up", upload, 100)], [task("up", upload, 100)]);
    expect(forgotten.within).not.toHaveBeenCalled();
    expect(forgotten.server).not.toHaveBeenCalled();
  });
});
