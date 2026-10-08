import { describe, expect, it } from "vitest";
import {
  EMPTY_DRAFT,
  canConnect,
  connectionDetail,
  draftFromSaved,
  findSameAccount,
  profileFromDraft,
  savedFromDraft,
  withProtocol,
} from "./connectDraft";
import type { SavedConnection } from "./types";

const saved: SavedConnection = {
  id: "abc",
  protocol: "sftp",
  name: "Build box",
  host: "build.example.com",
  port: 2222,
  username: "ci",
  authType: "publicKey",
  keyPath: "/keys/ci",
  remotePath: "/srv",
  saveSecret: true,
  lastUsed: 5,
  ftpActive: false,
};

describe("profileFromDraft", () => {
  it("trims fields and falls back to port 22", () => {
    const profile = profileFromDraft({
      ...EMPTY_DRAFT,
      host: " example.com ",
      port: "",
      username: " deploy ",
      password: " secret ",
      initialPath: "  ",
    });
    expect(profile).toEqual({
      protocol: "sftp",
      host: "example.com",
      port: 22,
      username: "deploy",
      auth: { type: "password", password: " secret " },
      initialPath: null,
    });
  });

  it("builds key and agent authentication", () => {
    const keyProfile = profileFromDraft({
      ...EMPTY_DRAFT,
      authChoice: "publicKey",
      keyPath: " C:\\keys\\server.ppk ",
      port: "2222",
    });
    expect(keyProfile.port).toBe(2222);
    expect(keyProfile.auth).toEqual({
      type: "publicKey",
      keyPath: "C:\\keys\\server.ppk",
      passphrase: null,
    });
    expect(profileFromDraft({ ...EMPTY_DRAFT, authChoice: "agent" }).auth).toEqual({
      type: "agent",
    });
  });
});

describe("saved connections", () => {
  it("round-trips through a draft and lets the keychain fill the secret", () => {
    const draft = draftFromSaved(saved);
    expect(draft.hasSavedSecret).toBe(true);
    expect(profileFromDraft(draft).savedConnectionId).toBe("abc");
    expect(savedFromDraft(draft)).toEqual({ ...saved, lastUsed: undefined });
  });

  it("never marks agent connections as having a secret", () => {
    const draft = { ...EMPTY_DRAFT, host: "h", authChoice: "agent" as const, saveSecret: true };
    expect(savedFromDraft(draft).saveSecret).toBe(false);
    expect(savedFromDraft(draft).name).toBe("h");
  });

  it("finds the same account regardless of host case", () => {
    const account = { protocol: "sftp" as const, username: "ci" };
    expect(findSameAccount([saved], { ...account, host: "BUILD.example.com", port: 2222 })).toBe(
      saved,
    );
    expect(findSameAccount([saved], { ...account, host: "build.example.com", port: 22 })).toBe(
      undefined,
    );
    expect(
      findSameAccount([saved], {
        ...account,
        protocol: "ftp",
        host: "build.example.com",
        port: 2222,
      }),
    ).toBe(undefined);
  });
});

describe("protocols", () => {
  it("moves the default port and sign-in method along with the protocol", () => {
    const keyDraft = { ...EMPTY_DRAFT, host: "example.com", authChoice: "publicKey" as const };
    const ftp = withProtocol(keyDraft, "ftp");
    expect(ftp.port).toBe("21");
    expect(ftp.authChoice).toBe("password");
    expect(ftp.host).toBe("example.com");
    expect(withProtocol(ftp, "ftpsImplicit").port).toBe("990");
    expect(withProtocol({ ...ftp, port: "2121" }, "sftp").port).toBe("2121");
    const drive = withProtocol(ftp, "googleDrive");
    expect(drive.authChoice).toBe("oauth");
    expect(drive.host).toBe("");
  });

  it("forgets a stored secret that no longer fits the sign-in method", () => {
    const stored = { ...EMPTY_DRAFT, host: "example.com", hasSavedSecret: true };
    expect(withProtocol(stored, "ftp").hasSavedSecret).toBe(true);
    const keyDraft = { ...stored, authChoice: "publicKey" as const };
    expect(withProtocol(keyDraft, "ftp").hasSavedSecret).toBe(false);
    expect(withProtocol(stored, "oneDrive").hasSavedSecret).toBe(false);
  });

  it("lets FTP connect without a username and cloud storage only once signed in", () => {
    const ftp = { ...withProtocol(EMPTY_DRAFT, "ftp"), host: "files.example.com" };
    expect(canConnect(ftp)).toBe(true);
    expect(canConnect({ ...EMPTY_DRAFT, host: "example.com" })).toBe(false);
    const drive = withProtocol(EMPTY_DRAFT, "googleDrive");
    expect(canConnect(drive)).toBe(false);
    const signedIn = {
      ...drive,
      signedIn: { grantId: "g1", provider: "google" as const, account: "ada@example.com" },
    };
    expect(canConnect(signedIn)).toBe(true);
    expect(profileFromDraft(signedIn)).toEqual({
      protocol: "googleDrive",
      host: "",
      port: 443,
      username: "ada@example.com",
      auth: { type: "oauth", grantId: "g1" },
      initialPath: null,
    });
    expect(savedFromDraft(signedIn)).toMatchObject({
      name: "Google Drive (ada@example.com)",
      authType: "oauth",
      saveSecret: true,
    });
  });

  it("describes saved connections by protocol", () => {
    expect(connectionDetail(saved)).toBe("ci@build.example.com:2222");
    expect(connectionDetail({ ...saved, protocol: "ftp", port: 21, username: "" })).toBe(
      "FTP, anonymous@build.example.com",
    );
    expect(connectionDetail({ ...saved, protocol: "oneDrive", username: "ada@contoso.com" })).toBe(
      "OneDrive, ada@contoso.com",
    );
  });
});
