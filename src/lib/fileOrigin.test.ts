import { describe, expect, it } from "vitest";
import {
  isInsideVault,
  sameFilesystem,
  toLocation,
  worksInPlace,
  type FileOrigin,
  type ServerIdentity,
} from "./fileOrigin";

const server: ServerIdentity = {
  protocol: "sftp",
  host: "Files.example.com",
  port: 22,
  username: "deploy",
};

describe("sameFilesystem", () => {
  it("treats every local pane as one filesystem", () => {
    expect(sameFilesystem({ kind: "local" }, { kind: "local" })).toBe(true);
    expect(sameFilesystem({ kind: "local" }, { kind: "remote", sessionId: "a" })).toBe(false);
  });

  it("matches a server by session, or by host, port and user", () => {
    const first: FileOrigin = { kind: "remote", sessionId: "a", server };
    expect(sameFilesystem(first, { kind: "remote", sessionId: "a" })).toBe(true);
    expect(
      sameFilesystem(first, {
        kind: "remote",
        sessionId: "b",
        server: { ...server, host: "files.example.com" },
      }),
    ).toBe(true);
    expect(
      sameFilesystem(first, {
        kind: "remote",
        sessionId: "b",
        server: { ...server, username: "root" },
      }),
    ).toBe(false);
    expect(sameFilesystem(first, { kind: "remote", sessionId: "b" })).toBe(false);
  });

  it("tells a server's protocols apart", () => {
    expect(
      sameFilesystem(
        { kind: "remote", sessionId: "a", server },
        { kind: "remote", sessionId: "b", server: { ...server, protocol: "ftps" } },
      ),
    ).toBe(false);
  });

  it("tells a vault from its server and from another vault there", () => {
    const vault: ServerIdentity = { ...server, protocol: "cryptomator", vaultRoot: "/srv/private" };
    const inVault: FileOrigin = { kind: "remote", sessionId: "v", server: vault };
    expect(sameFilesystem(inVault, { kind: "remote", sessionId: "a", server })).toBe(false);
    expect(
      sameFilesystem(inVault, {
        kind: "remote",
        sessionId: "w",
        server: { ...vault, vaultRoot: "/srv/ledger" },
      }),
    ).toBe(false);
    expect(sameFilesystem(inVault, { kind: "remote", sessionId: "w", server: { ...vault } })).toBe(
      true,
    );
    expect(isInsideVault(inVault)).toBe(true);
    expect(isInsideVault({ kind: "remote", sessionId: "a", server })).toBe(false);
    expect(isInsideVault({ kind: "local" })).toBe(false);
    expect(worksInPlace(inVault)).toBe(false);
  });

  it("works in place on this computer and over SFTP only", () => {
    expect(worksInPlace({ kind: "local" })).toBe(true);
    expect(worksInPlace({ kind: "remote", sessionId: "a", server })).toBe(true);
    expect(
      worksInPlace({ kind: "remote", sessionId: "a", server: { ...server, protocol: "ftp" } }),
    ).toBe(false);
  });

  it("names the place an operation runs", () => {
    expect(toLocation({ kind: "remote", sessionId: "a", server })).toEqual({
      kind: "remote",
      sessionId: "a",
    });
    expect(toLocation({ kind: "local" })).toEqual({ kind: "local" });
  });
});
