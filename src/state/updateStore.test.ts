import { describe, expect, it } from "vitest";
import type { ExternalEdit } from "../lib/types";
import { describeCheckError, notUploadedWarning, percentDownloaded } from "./updateStore";

describe("describeCheckError", () => {
  it("explains a release without update information", () => {
    expect(describeCheckError("Could not fetch a valid release JSON from the remote")).toBe(
      "The latest release on GitHub does not offer in-app updates yet",
    );
  });

  it("passes other failures through", () => {
    expect(describeCheckError("error sending request")).toBe(
      "Could not check for updates: error sending request",
    );
  });
});

describe("percentDownloaded", () => {
  it("rounds down, so 100 means the whole download", () => {
    expect(percentDownloaded(0, 2000)).toBe(0);
    expect(percentDownloaded(1999, 2000)).toBe(99);
    expect(percentDownloaded(2000, 2000)).toBe(100);
  });

  it("is unknown without a size", () => {
    expect(percentDownloaded(500, null)).toBeNull();
  });

  it("stops at 100 when the server understated the size", () => {
    expect(percentDownloaded(3000, 2000)).toBe(100);
  });
});

describe("notUploadedWarning", () => {
  const edit = (id: string, state: ExternalEdit["state"], inVault = false): ExternalEdit => ({
    id,
    name: `${id}.txt`,
    remotePath: `/srv/${id}.txt`,
    sessionId: inVault ? "vault" : "server",
    server: "files.example",
    inVault,
    outlivesSession: !inVault,
    localPath: `/tmp/${id}.txt`,
    program: "",
    state,
    transferred: 0,
    size: 1,
    blocked: false,
    uploads: 0,
  });

  it("says nothing while every file holds what its server has", () => {
    expect(notUploadedWarning([edit("a", "watching"), edit("b", "downloading")])).toBeNull();
  });

  it("tells saves that are offered again from vault saves that are wiped", () => {
    expect(notUploadedWarning([edit("a", "changed")])).toBe(
      "A file open in another program holds a save not uploaded yet. Poros offers to upload it after the restart.",
    );
    expect(notUploadedWarning([edit("a", "failed", true), edit("b", "conflict", true)])).toBe(
      "2 files open in other programs hold saves not uploaded yet. Restarting wipes them, as vault files are not kept unencrypted.",
    );
    expect(notUploadedWarning([edit("a", "uploading"), edit("b", "waiting", true)])).toBe(
      "2 files open in other programs hold saves not uploaded yet. Those from vaults are wiped; Poros offers to upload the others after the restart.",
    );
  });
});
