import { beforeEach, describe, expect, it, vi } from "vitest";
import { group, type PaneTab } from "../lib/layout";
import type { SessionInfo } from "../lib/types";
import { useSessionStore } from "./sessionStore";
import { adoptHandoff, familyOf, type TabHandoff } from "./tabActions";

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
