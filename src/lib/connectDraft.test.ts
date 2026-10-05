import { describe, expect, it } from "vitest";
import { EMPTY_DRAFT, profileFromDraft } from "./connectDraft";

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
