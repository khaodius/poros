import { describe, expect, it } from "vitest";
import {
  EMPTY_DRAFT,
  draftFromSaved,
  findSameAccount,
  profileFromDraft,
  savedFromDraft,
} from "./connectDraft";
import type { SavedConnection } from "./types";

const saved: SavedConnection = {
  id: "abc",
  name: "Build box",
  host: "build.example.com",
  port: 2222,
  username: "ci",
  authType: "publicKey",
  keyPath: "/keys/ci",
  remotePath: "/srv",
  saveSecret: true,
  lastUsed: 5,
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
    expect(
      findSameAccount([saved], { host: "BUILD.example.com", port: 2222, username: "ci" }),
    ).toBe(saved);
    expect(findSameAccount([saved], { host: "build.example.com", port: 22, username: "ci" })).toBe(
      undefined,
    );
  });
});
