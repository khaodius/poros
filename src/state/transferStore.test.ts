import { afterEach, describe, expect, it, vi } from "vitest";
import { EMPTY_SNAPSHOT } from "../lib/transfers";
import type { SessionInfo, TransferUpdate } from "../lib/types";
import { registerPane } from "./paneRegistry";
import { currentSessionId, useSessionStore } from "./sessionStore";
import { useTransferStore } from "./transferStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));
// Panes reload with the window's timers, which the test's fake timers stand in for.
vi.stubGlobal("window", {
  setTimeout: (...args: Parameters<typeof setTimeout>) => setTimeout(...args),
  clearTimeout: (timer: ReturnType<typeof setTimeout>) => clearTimeout(timer),
});

function server(id: string): SessionInfo {
  return {
    id,
    label: "deploy@web.example.com",
    host: "web.example.com",
    port: 22,
    username: "deploy",
    home: "/home/deploy",
    initialPath: "/home/deploy",
    protocol: "ftp",
  };
}

const unregister: (() => void)[] = [];
afterEach(() => {
  unregister.splice(0).forEach((remove) => remove());
  vi.useRealTimers();
});

describe("files a transfer queued before a reconnect wrote", () => {
  it("reload the pane that shows their folder on the new session", async () => {
    vi.useFakeTimers();
    useSessionStore.setState({
      sessions: { s1: { info: server("s1"), profile: null, status: "connected" } },
    });
    let next = 2;
    invoke.mockImplementation(async (command: string) =>
      command === "reconnect" ? server(`s${next++}`) : undefined,
    );
    await useSessionStore.getState().reconnect("s1");
    await useSessionStore.getState().reconnect("s2");
    expect(currentSessionId("s1")).toBe("s3");
    expect(currentSessionId("s3")).toBe("s3");

    const refresh = vi.fn();
    unregister.push(
      registerPane({
        tabId: "files",
        kind: "remote",
        sessionId: "s3",
        label: "deploy@web.example.com",
        path: () => "/srv/www",
        refresh,
        selected: () => [],
        visible: true,
        activatedAt: 0,
      }),
    );
    const update: TransferUpdate = {
      sequence: EMPTY_SNAPSHOT.sequence,
      jobs: [],
      removed: [],
      changedDirectories: [{ side: "remote", sessionId: "s1", path: "/srv/www" }],
      stats: EMPTY_SNAPSHOT.stats,
    };
    useTransferStore.getState().apply(update);
    vi.runAllTimers();
    expect(refresh).toHaveBeenCalledTimes(1);
  });
});
