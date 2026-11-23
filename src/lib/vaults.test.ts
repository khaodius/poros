import { describe, expect, it } from "vitest";
import type { FileEntry, SessionInfo } from "./types";
import {
  isInVault,
  isLost,
  isVaultSession,
  serverSessionId,
  sessionFamily,
  sessionShowing,
  vaultSessions,
  vaultSign,
} from "./vaults";

function entry(name: string, kind: FileEntry["kind"] = "file"): FileEntry {
  return {
    name,
    path: `/srv/private/${name}`,
    kind,
    size: 0,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

function server(id: string): SessionInfo {
  return {
    id,
    label: `deploy@${id}.example.com`,
    host: `${id}.example.com`,
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
    ...server(parentSessionId),
    id,
    label: `${name} on deploy@${parentSessionId}.example.com`,
    home: root,
    initialPath: root,
    protocol: "cryptomator",
    vault: { parentSessionId, root, name },
  };
}

type Status = "connected" | "lost";

function known(infos: SessionInfo[], lost: string[] = []) {
  return Object.fromEntries(
    infos.map((info) => [
      info.id,
      { info, status: (lost.includes(info.id) ? "lost" : "connected") as Status },
    ]),
  );
}

describe("vaultSign", () => {
  it("tells a vault by its configuration file", () => {
    expect(vaultSign([entry("d", "dir"), entry("vault.cryptomator")])).toBe("locked");
    expect(
      vaultSign([entry("d", "dir"), entry("masterkey.cryptomator"), entry("vault.cryptomator")]),
    ).toBe("locked");
  });

  it("tells an older vault by its key and data folder without a configuration", () => {
    expect(vaultSign([entry("d", "dir"), entry("masterkey.cryptomator")])).toBe("outdated");
  });

  it("sees no vault in an ordinary folder", () => {
    expect(vaultSign([])).toBe("none");
    expect(vaultSign([entry("index.html"), entry("d", "dir")])).toBe("none");
    expect(vaultSign([entry("masterkey.cryptomator")])).toBe("none");
    expect(vaultSign([entry("masterkey.cryptomator"), entry("d")])).toBe("none");
  });

  it("takes a folder or a differently spelled name for no configuration", () => {
    expect(vaultSign([entry("vault.cryptomator", "dir")])).toBe("none");
    expect(vaultSign([entry("Vault.Cryptomator")])).toBe("none");
    expect(vaultSign([entry("vault.cryptomator.bak")])).toBe("none");
  });
});

describe("isInVault", () => {
  it("holds the vault's folder and all below it", () => {
    expect(isInVault("/srv/private", "/srv/private")).toBe(true);
    expect(isInVault("/srv/private", "/srv/private/")).toBe(true);
    expect(isInVault("/srv/private", "/srv/private/letters/2024")).toBe(true);
  });

  it("leaves out what is above or beside the vault", () => {
    expect(isInVault("/srv/private", "/srv")).toBe(false);
    expect(isInVault("/srv/private", "/")).toBe(false);
    expect(isInVault("/srv/private", "/srv/private-old")).toBe(false);
    expect(isInVault("/srv/private", "/srv/public/private")).toBe(false);
  });

  it("follows a typed path out of the vault and back in", () => {
    expect(isInVault("/srv/private", "/srv/private/..")).toBe(false);
    expect(isInVault("/srv/private", "/srv/private/letters/../../public")).toBe(false);
    expect(isInVault("/srv/private", "/srv/public/../private/letters")).toBe(true);
    expect(isInVault("/srv/private", "/srv/./private")).toBe(true);
  });

  it("holds everything in a vault that is the whole server", () => {
    expect(isInVault("/", "/")).toBe(true);
    expect(isInVault("/", "/letters")).toBe(true);
  });
});

describe("session families", () => {
  const sessions = known([
    server("s1"),
    vault("v1", "s1", "/srv/private"),
    vault("v2", "s1", "/srv/ledger"),
    server("s2"),
    vault("v3", "s2", "/srv/private"),
  ]);

  it("knows a vault's session from a server's", () => {
    expect(isVaultSession(sessions.s1.info)).toBe(false);
    expect(isVaultSession(sessions.v1.info)).toBe(true);
  });

  it("finds the server a session's files are on", () => {
    expect(serverSessionId(sessions.s1.info)).toBe("s1");
    expect(serverSessionId(sessions.v1.info)).toBe("s1");
    expect(serverSessionId(sessions.v3.info)).toBe("s2");
  });

  it("lists the vaults unlocked on a server", () => {
    expect(vaultSessions(sessions, "s1").map((info) => info.id)).toEqual(["v1", "v2"]);
    expect(vaultSessions(sessions, "s2").map((info) => info.id)).toEqual(["v3"]);
    expect(vaultSessions(sessions, "v1")).toEqual([]);
    expect(vaultSessions(sessions, "gone")).toEqual([]);
  });

  it("gathers a family from any member, the server first", () => {
    expect(sessionFamily(sessions, "s1")).toEqual(["s1", "v1", "v2"]);
    expect(sessionFamily(sessions, "v2")).toEqual(["s1", "v1", "v2"]);
    expect(sessionFamily(sessions, "v3")).toEqual(["s2", "v3"]);
    expect(sessionFamily(known([server("s1")]), "s1")).toEqual(["s1"]);
  });

  it("takes an unknown session for a family of its own", () => {
    expect(sessionFamily(sessions, "gone")).toEqual(["gone"]);
  });
});

describe("isLost", () => {
  it("loses a vault with its server", () => {
    const sessions = known([server("s1"), vault("v1", "s1", "/srv/private")], ["s1"]);
    expect(isLost(sessions, "s1")).toBe(true);
    expect(isLost(sessions, "v1")).toBe(true);
  });

  it("keeps a connected family", () => {
    const sessions = known([server("s1"), vault("v1", "s1", "/srv/private")]);
    expect(isLost(sessions, "s1")).toBe(false);
    expect(isLost(sessions, "v1")).toBe(false);
  });

  it("counts a session this window does not have as lost", () => {
    expect(isLost(known([vault("v1", "s1", "/srv/private")]), "v1")).toBe(true);
    expect(isLost({}, "s1")).toBe(true);
  });
});

describe("sessionShowing", () => {
  const sessions = known([
    server("s1"),
    vault("v1", "s1", "/srv/private"),
    vault("v2", "s1", "/srv/ledger"),
    server("s2"),
  ]);

  it("enters an unlocked vault at its folder and below", () => {
    expect(sessionShowing(sessions, "s1", "/srv/private")).toBe("v1");
    expect(sessionShowing(sessions, "s1", "/srv/private/letters")).toBe("v1");
    expect(sessionShowing(sessions, "s1", "/srv/ledger/2024")).toBe("v2");
  });

  it("leaves a vault for its server above the vault's folder", () => {
    expect(sessionShowing(sessions, "v1", "/srv")).toBe("s1");
    expect(sessionShowing(sessions, "v1", "/")).toBe("s1");
    expect(sessionShowing(sessions, "v1", "/srv/private/..")).toBe("s1");
  });

  it("goes from one vault straight into another on the same server", () => {
    expect(sessionShowing(sessions, "v1", "/srv/ledger")).toBe("v2");
  });

  it("stays where it is while the path is the session's own", () => {
    expect(sessionShowing(sessions, "s1", "/srv/public")).toBe("s1");
    expect(sessionShowing(sessions, "v1", "/srv/private/letters")).toBe("v1");
  });

  it("keeps to the vaults of the pane's own server", () => {
    expect(sessionShowing(sessions, "s2", "/srv/private")).toBe("s2");
  });

  it("stays on the server while the vault is locked", () => {
    const locked = known([server("s1")]);
    expect(sessionShowing(locked, "s1", "/srv/private/letters")).toBe("s1");
  });

  it("picks the innermost vault were one to hold another", () => {
    const nested = known([
      server("s1"),
      vault("outer", "s1", "/srv/private"),
      vault("inner", "s1", "/srv/private/deeper"),
    ]);
    expect(sessionShowing(nested, "s1", "/srv/private/deeper/notes")).toBe("inner");
    expect(sessionShowing(nested, "inner", "/srv/private/other")).toBe("outer");
  });

  it("leaves a session it does not know, or a vault without its server, alone", () => {
    expect(sessionShowing(sessions, "gone", "/srv/private")).toBe("gone");
    const orphan = known([vault("v1", "s1", "/srv/private")]);
    expect(sessionShowing(orphan, "v1", "/srv")).toBe("v1");
  });
});
