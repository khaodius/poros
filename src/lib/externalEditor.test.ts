import { describe, expect, it } from "vitest";
import {
  defaultOpenWith,
  editStateLabel,
  editsOfSessions,
  editsSummary,
  formatPatterns,
  looksLikeText,
  matchesPattern,
  needsAttention,
  parsePatterns,
  programFor,
  programLabel,
  sessionForLeftover,
  transferFraction,
} from "./externalEditor";
import type { ExternalProgram } from "./settings";
import type { ExternalEdit, ExternalLeftover, SessionInfo } from "./types";

function program(id: string, patterns: string[], name = ""): ExternalProgram {
  return { id, name, command: `/usr/bin/${id}`, arguments: "", patterns };
}

function edit(change: Partial<ExternalEdit> = {}): ExternalEdit {
  return {
    id: "edit-1",
    name: "report.xlsx",
    remotePath: "/srv/report.xlsx",
    sessionId: "session-1",
    server: "ada@files.example",
    inVault: false,
    outlivesSession: true,
    localPath: "/cache/editing/1/report.xlsx",
    program: "LibreOffice",
    state: "watching",
    transferred: 0,
    size: 2048,
    blocked: false,
    uploads: 0,
    ...change,
  };
}

function session(change: Partial<SessionInfo> = {}): SessionInfo {
  return {
    id: "session-1",
    label: "ada@files.example",
    host: "files.example",
    port: 22,
    username: "ada",
    home: "/home/ada",
    initialPath: "/home/ada",
    protocol: "sftp",
    ...change,
  };
}

const leftover: ExternalLeftover = {
  id: "left-1",
  name: "notes.txt",
  remotePath: "/srv/notes.txt",
  server: "ada@files.example",
  protocol: "sftp",
  host: "files.example",
  port: 22,
  username: "ada",
  size: 12,
};

describe("matchesPattern", () => {
  it("matches wildcards in any case", () => {
    expect(matchesPattern("photo.psd", "*.psd")).toBe(true);
    expect(matchesPattern("PHOTO.PSD", "*.psd")).toBe(true);
    expect(matchesPattern("photo.psd.bak", "*.psd")).toBe(false);
    expect(matchesPattern("report-07.csv", "report-??.csv")).toBe(true);
    expect(matchesPattern("report-7.csv", "report-??.csv")).toBe(false);
    expect(matchesPattern("access.log.1", "*.log*")).toBe(true);
  });

  it("takes a bare extension for every file with it, and a plain name for that name", () => {
    expect(matchesPattern("server.log", ".log")).toBe(true);
    expect(matchesPattern("Makefile", "Makefile")).toBe(true);
    expect(matchesPattern("Makefile.am", "Makefile")).toBe(false);
  });

  it("reads nothing in a pattern as a regular expression", () => {
    expect(matchesPattern("a+b.txt", "a+b.txt")).toBe(true);
    expect(matchesPattern("aab.txt", "a+b.txt")).toBe(false);
    expect(matchesPattern("x.txt", "(x).txt")).toBe(false);
    expect(matchesPattern("anything", "  ")).toBe(false);
  });
});

describe("patterns typed in a box", () => {
  it("split at commas, semicolons and spaces without repeats", () => {
    expect(parsePatterns(" *.psd, *.xcf;*.psd  *.kra ")).toEqual(["*.psd", "*.xcf", "*.kra"]);
    expect(parsePatterns("")).toEqual([]);
    expect(formatPatterns(["*.psd", "*.xcf"])).toBe("*.psd, *.xcf");
  });
});

describe("which program Open uses", () => {
  const programs = [program("gimp", ["*.psd", "*.xcf"]), program("code", ["*.log", "*.psd"])];

  it("is the first program whose pattern names the file", () => {
    expect(programFor("art.PSD", programs)?.id).toBe("gimp");
    expect(programFor("server.log", programs)?.id).toBe("code");
    expect(defaultOpenWith("art.xcf", programs)).toEqual({ kind: "program", id: "gimp" });
  });

  it("is the system's own for any other file", () => {
    expect(programFor("report.xlsx", programs)).toBeUndefined();
    expect(defaultOpenWith("report.xlsx", programs)).toEqual({ kind: "default" });
    expect(defaultOpenWith("report.xlsx", [])).toEqual({ kind: "default" });
  });
});

describe("programLabel", () => {
  it("uses the name given, else the program's file name", () => {
    expect(programLabel({ name: " GIMP ", command: "/usr/bin/gimp" })).toBe("GIMP");
    expect(programLabel({ name: "", command: "/usr/bin/gimp-2.10" })).toBe("gimp-2");
    expect(programLabel({ name: "", command: "C:\\Program Files\\Notepad++\\notepad++.exe" })).toBe(
      "notepad++",
    );
    expect(programLabel({ name: "", command: "code" })).toBe("code");
  });
});

describe("how an edit is described", () => {
  it("names each state, and a file held back as a program", () => {
    expect(editStateLabel(edit())).toBe("Watching for saves");
    expect(editStateLabel(edit({ state: "conflict" }))).toBe("Changed on the server");
    expect(editStateLabel(edit({ blocked: true }))).toBe("Not opened: it is a program");
  });

  it("measures downloads and uploads only", () => {
    expect(transferFraction(edit({ state: "downloading", transferred: 512 }))).toBe(0.25);
    expect(transferFraction(edit({ state: "uploading", transferred: 4096 }))).toBe(1);
    expect(transferFraction(edit({ state: "uploading", size: 0 }))).toBe(0);
    expect(transferFraction(edit({ state: "failed", transferred: 512 }))).toBeNull();
  });

  it("asks for attention for conflicts, failures and files held back", () => {
    expect(needsAttention(edit())).toBe(false);
    expect(needsAttention(edit({ state: "changed" }))).toBe(false);
    expect(needsAttention(edit({ state: "conflict" }))).toBe(true);
    expect(needsAttention(edit({ state: "failed" }))).toBe(true);
    expect(needsAttention(edit({ blocked: true }))).toBe(true);
  });

  it("sums up what goes on in the status bar", () => {
    expect(editsSummary([])).toBeNull();
    expect(editsSummary([edit()])).toBe("Editing report.xlsx");
    expect(editsSummary([edit(), edit({ id: "edit-2" })])).toBe("Editing 2 files");
    expect(
      editsSummary([edit(), edit({ id: "edit-2", state: "downloading", transferred: 1024 })]),
    ).toBe("Opening report.xlsx, 50%");
    expect(editsSummary([edit({ state: "uploading" })])).toBe("Uploading report.xlsx");
    expect(editsSummary([edit({ state: "conflict" })])).toBe("report.xlsx: changed on the server");
    expect(editsSummary([edit({ state: "failed" }), edit({ blocked: true })])).toBe(
      "2 edited files need attention",
    );
    expect(editsSummary([], [leftover])).toBe("1 file not uploaded");
  });

  it("finds the edits of a tab's sessions", () => {
    const edits = [
      edit(),
      edit({ id: "edit-2", sessionId: "vault-1" }),
      edit({ id: "edit-3", sessionId: "other" }),
    ];
    expect(editsOfSessions(edits, ["session-1", "vault-1"]).map((found) => found.id)).toEqual([
      "edit-1",
      "edit-2",
    ]);
  });
});

describe("sessionForLeftover", () => {
  it("finds a session to the same account on the same server", () => {
    const sessions = [session({ id: "other", username: "bob" }), session()];
    expect(sessionForLeftover(leftover, sessions)?.id).toBe("session-1");
    expect(sessionForLeftover(leftover, [session({ port: 2222 })])).toBeUndefined();
    expect(sessionForLeftover(leftover, [session({ protocol: "ftp" })])).toBeUndefined();
  });

  it("finds a file of a vault only through that vault's session", () => {
    const vault = session({
      id: "vault-1",
      protocol: "cryptomator",
      vault: { parentSessionId: "session-1", root: "/srv/vault", name: "vault" },
    });
    const inVault = { ...leftover, vaultRoot: "/srv/vault" };
    expect(sessionForLeftover(inVault, [session(), vault])?.id).toBe("vault-1");
    expect(sessionForLeftover(inVault, [session()])).toBeUndefined();
    // A vault's session without its server's is not enough to know the server.
    expect(sessionForLeftover(inVault, [vault])).toBeUndefined();
    expect(sessionForLeftover(leftover, [vault])).toBeUndefined();
  });
});

describe("looksLikeText", () => {
  it("tells text files by their extension", () => {
    expect(looksLikeText("notes.TXT")).toBe(true);
    expect(looksLikeText("config.yaml")).toBe(true);
    expect(looksLikeText("photo.psd")).toBe(false);
    expect(looksLikeText("README")).toBe(false);
    expect(looksLikeText(".log")).toBe(false);
  });
});
