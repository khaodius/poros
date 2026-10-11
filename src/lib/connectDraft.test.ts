import { describe, expect, it } from "vitest";
import {
  EMPTY_DRAFT,
  canConnect,
  connectionDetail,
  draftFromSaved,
  findSameAccount,
  profileFromDraft,
  savedFromDraft,
  withField,
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
  bypassProxy: true,
  jumpConnectionId: "bastion",
  rsyncPath: "sudo rsync",
  syncArguments: "--delete --exclude=.git/",
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

  it("carries the route to the server", () => {
    const profile = profileFromDraft(draftFromSaved(saved));
    expect(profile.jumpConnectionId).toBe("bastion");
    expect(profile.bypassProxy).toBe(true);
    const direct = savedFromDraft({ ...draftFromSaved(saved), jumpConnectionId: "" });
    expect(direct.jumpConnectionId).toBeNull();
  });

  it("keeps the site's rsync command to one line and its arguments to SFTP", () => {
    const draft = { ...draftFromSaved(saved), rsyncPath: "  /opt/rsync \nrm -rf ~" };
    expect(profileFromDraft(draft).rsyncPath).toBe("/opt/rsync");
    expect(savedFromDraft(draft).rsyncPath).toBe("/opt/rsync");
    expect(profileFromDraft({ ...draft, rsyncPath: " " }).rsyncPath).toBeUndefined();
    const ftp = withProtocol(draftFromSaved(saved), "ftp");
    expect(profileFromDraft(ftp).rsyncPath).toBeUndefined();
    expect(savedFromDraft(ftp).rsyncPath).toBeNull();
    expect(savedFromDraft(ftp).syncArguments).toBeNull();
  });

  it("keeps the proxy and jump host to SFTP connections", () => {
    const ftp = withProtocol(draftFromSaved(saved), "ftp");
    expect(profileFromDraft(ftp).jumpConnectionId).toBeUndefined();
    expect(profileFromDraft(ftp).bypassProxy).toBeUndefined();
    expect(savedFromDraft(ftp).jumpConnectionId).toBeNull();
    expect(savedFromDraft(ftp).bypassProxy).toBe(false);
  });

  it("never marks agent connections as having a secret", () => {
    const draft = { ...EMPTY_DRAFT, host: "h", authChoice: "agent" as const, saveSecret: true };
    expect(savedFromDraft(draft).saveSecret).toBe(false);
    expect(savedFromDraft(draft).name).toBe("h");
  });

  it("forgets a saved secret once the connection points elsewhere", () => {
    const draft = draftFromSaved(saved);
    expect(withField(draft, "name", "Renamed").hasSavedSecret).toBe(true);
    expect(withField(draft, "initialPath", "/tmp").hasSavedSecret).toBe(true);
    expect(withField(draft, "host", draft.host).hasSavedSecret).toBe(true);
    const moved = withField(draft, "host", "evil.example.com");
    expect(moved.host).toBe("evil.example.com");
    expect(moved.hasSavedSecret).toBe(false);
    expect(withField(draft, "port", "22").hasSavedSecret).toBe(false);
    expect(withField(draft, "username", "root").hasSavedSecret).toBe(false);
    expect(withField(draft, "authChoice", "password").hasSavedSecret).toBe(false);
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

  it("forgets a stored secret that no longer fits the sign-in method or port", () => {
    const stored = { ...EMPTY_DRAFT, host: "example.com", port: "2222", hasSavedSecret: true };
    expect(withProtocol(stored, "ftp").hasSavedSecret).toBe(true);
    expect(withProtocol({ ...stored, port: "22" }, "ftp").hasSavedSecret).toBe(false);
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

  it("signs in to S3 and Backblaze B2 with keys", () => {
    const s3 = {
      ...withProtocol({ ...EMPTY_DRAFT, host: "example.com", username: "root" }, "s3"),
      username: "AKIAEXAMPLE",
      password: "secret",
      s3Region: " eu-west-1 ",
    };
    expect(s3.host).toBe("");
    expect(s3.port).toBe("443");
    expect(s3.authChoice).toBe("password");
    expect(canConnect(s3)).toBe(true);
    expect(canConnect({ ...s3, username: "" })).toBe(false);
    expect(profileFromDraft(s3)).toEqual({
      protocol: "s3",
      host: "",
      port: 443,
      username: "AKIAEXAMPLE",
      auth: { type: "password", password: "secret" },
      initialPath: null,
      s3: { region: "eu-west-1", addressing: "automatic" },
    });
    const minio = { ...s3, host: "http://localhost:9000", s3Addressing: "path" as const };
    const stored = savedFromDraft(minio);
    expect(stored).toMatchObject({
      name: "http://localhost:9000",
      s3: { region: "eu-west-1", addressing: "path" },
    });
    expect(draftFromSaved(stored)).toMatchObject({ s3Region: "eu-west-1", s3Addressing: "path" });
    expect(savedFromDraft(s3).name).toBe("Amazon S3");

    const b2 = { ...withProtocol(s3, "backblazeB2"), username: "0012ab", password: "K001" };
    expect(b2.username).toBe("0012ab");
    expect(canConnect(b2)).toBe(true);
    expect(profileFromDraft(b2)).toMatchObject({ host: "", port: 443, username: "0012ab" });
    expect(profileFromDraft(b2).s3).toBeUndefined();
    expect(savedFromDraft(b2)).toMatchObject({ name: "Backblaze B2 (0012ab)", saveSecret: false });
    expect(withProtocol(b2, "s3").username).toBe("");
    expect(withProtocol(b2, "sftp").username).toBe("");
  });

  it("lets WebDAV connect without a username and Box only once signed in", () => {
    const dav = withProtocol(
      { ...EMPTY_DRAFT, host: "dav.example.com", username: "ada" },
      "webDavs",
    );
    expect(dav.host).toBe("dav.example.com");
    expect(dav.username).toBe("ada");
    expect(dav.port).toBe("443");
    expect(withProtocol(dav, "webDav").port).toBe("80");
    expect(canConnect({ ...dav, username: "" })).toBe(true);
    const box = withProtocol(dav, "box");
    expect(box.authChoice).toBe("oauth");
    expect(box.host).toBe("");
    expect(canConnect(box)).toBe(false);
  });

  it("describes saved connections by protocol", () => {
    expect(connectionDetail(saved)).toBe("ci@build.example.com:2222");
    expect(connectionDetail({ ...saved, protocol: "ftp", port: 21, username: "" })).toBe(
      "FTP, anonymous@build.example.com",
    );
    expect(connectionDetail({ ...saved, protocol: "oneDrive", username: "ada@contoso.com" })).toBe(
      "OneDrive, ada@contoso.com",
    );
    expect(connectionDetail({ ...saved, protocol: "webDavs", port: 443, username: "" })).toBe(
      "WebDAVS, build.example.com",
    );
    expect(
      connectionDetail({ ...saved, protocol: "s3", host: "localhost", port: 9000, username: "K" }),
    ).toBe("S3, localhost:9000");
    expect(connectionDetail({ ...saved, protocol: "backblazeB2", username: "0012ab" })).toBe(
      "Backblaze B2, 0012ab",
    );
  });
});
