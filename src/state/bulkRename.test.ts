import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_RULES, type NameTarget, type RenameRules } from "../lib/bulkRename";
import type { FileOrigin } from "../lib/fileOrigin";
import { DEFAULT_SETTINGS } from "../lib/settings";
import type { Protocol, RenameRequest, RenameSummary } from "../lib/types";
import { nameTargetOf, runBulkRename, useBulkRenameStore } from "./bulkRename";
import { useLogStore } from "./logStore";
import { registerPane } from "./paneRegistry";
import { useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

const LOCAL: FileOrigin = { kind: "local" };

function server(protocol: Protocol, vaultRoot?: string): FileOrigin {
  return {
    kind: "remote",
    sessionId: "s1",
    server: { protocol, host: "web.example.com", port: 22, username: "deploy", vaultRoot },
  };
}

const POSIX: NameTarget = {
  forbidden: "/",
  windowsNames: false,
  caseInsensitive: false,
  normalizes: false,
  maxLength: 255,
  lengthUnit: "bytes",
};

interface Call {
  command: string;
  args: Record<string, unknown>;
}

/** What the backend was asked, in order, and what it answers to each command. */
let calls: Call[] = [];
let answers: Record<string, (args: Record<string, unknown>) => unknown> = {};
const toasts: string[] = [];
const logged: string[] = [];
/** The paths each reload of the pane on /srv/photos was asked to select. */
const refreshes: (string | readonly string[] | undefined)[] = [];
const unregister: (() => void)[] = [];

const asked = (command: string) => calls.filter((call) => call.command === command);
const renameRequest = () => asked("files_rename")[0].args.request as RenameRequest;

beforeEach(() => {
  calls = [];
  answers = {};
  toasts.length = 0;
  logged.length = 0;
  refreshes.length = 0;
  invoke.mockReset();
  invoke.mockImplementation((command: string, args: Record<string, unknown> = {}) => {
    calls.push({ command, args });
    try {
      return Promise.resolve(answers[command]?.(args) ?? null);
    } catch (thrown) {
      return Promise.reject(thrown as Error);
    }
  });
  vi.stubGlobal("window", { setTimeout, clearTimeout });
  useToastStore.setState({ show: (_tone, message) => void toasts.push(message) });
  useLogStore.setState({ write: (_level, message) => void logged.push(message) });
  useSettingsStore.setState({ settings: DEFAULT_SETTINGS });
  useBulkRenameStore.setState({ rules: DEFAULT_RULES, report: null });
  unregister.push(
    registerPane({
      tabId: "photos",
      kind: "remote",
      sessionId: "s1",
      label: "photos",
      path: () => "/srv/photos",
      refresh: (focus) => void refreshes.push(focus),
      selected: () => [],
      visible: true,
      activatedAt: 0,
    }),
  );
});

afterEach(() => {
  unregister.splice(0).forEach((remove) => remove());
  vi.unstubAllGlobals();
});

describe("nameTargetOf", () => {
  const names = ["notes.txt", "Photo.JPG"];

  it("asks a folder on this computer for one of its names in the other case", async () => {
    answers.local_stat = () => [{ name: "NOTES.TXT" }];
    expect((await nameTargetOf(LOCAL, "/home/ana", names)).caseInsensitive).toBe(true);
    expect(asked("local_stat")[0].args).toEqual({ paths: ["/home/ana/NOTES.TXT"] });

    answers.local_stat = () => [];
    expect((await nameTargetOf(LOCAL, "/home/ana", names)).caseInsensitive).toBe(false);
  });

  it("asks a server reached over SFTP, FTP or WebDAV the same way", async () => {
    for (const protocol of ["sftp", "ftp", "webDavs"] as const) {
      calls = [];
      answers.files_conflicts = () => ["NOTES.TXT"];
      const target = await nameTargetOf(server(protocol), "/srv/photos", names);
      expect(target.caseInsensitive).toBe(true);
      expect(asked("files_conflicts")[0].args).toEqual({
        location: { kind: "remote", sessionId: "s1" },
        names: ["NOTES.TXT"],
        directory: "/srv/photos",
      });
    }
    answers.files_conflicts = () => [];
    expect((await nameTargetOf(server("sftp"), "/srv/photos", names)).caseInsensitive).toBe(false);
  });

  it("takes case to matter when the folder cannot be asked", async () => {
    answers.files_conflicts = () => {
      throw new Error("The connection was lost");
    };
    expect((await nameTargetOf(server("sftp"), "/srv/photos", names)).caseInsensitive).toBe(false);
    // No name with a letter to spell the other way.
    expect(
      (await nameTargetOf(server("sftp"), "/srv/photos", ["2026", "1.5"])).caseInsensitive,
    ).toBe(false);
  });

  it("asks the folder for one of its names in the other Unicode form, as a Mac stores it", async () => {
    const accented = ["notes.txt", "r\u00e9sum\u00e9.pdf"];
    const decomposed = "re\u0301sume\u0301.pdf";
    answers.files_conflicts = ({ names: probed }) =>
      (probed as string[]).filter((name) => name === decomposed);
    const mac = await nameTargetOf(server("sftp"), "/Users/ana", accented);
    expect(mac).toMatchObject({ caseInsensitive: false, normalizes: true });
    expect(asked("files_conflicts").map((call) => call.args.names)).toEqual([
      ["NOTES.TXT"],
      [decomposed],
    ]);

    answers.local_stat = ({ paths }) =>
      (paths as string[]).filter((path) => path.endsWith(decomposed)).map((path) => ({ path }));
    const local = await nameTargetOf(LOCAL, "/Users/ana", accented);
    expect(local).toMatchObject({ caseInsensitive: false, normalizes: true });
    expect(asked("local_stat").map((call) => call.args.paths)).toContainEqual([
      `/Users/ana/${decomposed}`,
    ]);

    answers.files_conflicts = () => [];
    expect((await nameTargetOf(server("webDav"), "/dav", accented)).normalizes).toBe(false);
    answers.files_conflicts = () => {
      throw new Error("The connection was lost");
    };
    expect((await nameTargetOf(server("ftp"), "/pub", accented)).normalizes).toBe(false);

    // Names without accents leave nothing to ask about form.
    calls = [];
    answers.files_conflicts = () => [];
    expect((await nameTargetOf(server("sftp"), "/srv", names)).normalizes).toBe(false);
    expect(asked("files_conflicts")).toHaveLength(1);
  });

  it("knows without asking where the kind of place settles it", async () => {
    expect((await nameTargetOf(server("s3"), "/bucket/photos", names)).caseInsensitive).toBe(false);
    expect((await nameTargetOf(server("oneDrive"), "/Photos", names)).caseInsensitive).toBe(true);
    expect(await nameTargetOf(server("googleDrive"), "/Photos", names)).toMatchObject({
      caseInsensitive: true,
      normalizes: false,
    });
    const vault = await nameTargetOf(server("cryptomator", "/srv/vault"), "/srv/vault", names);
    expect(vault).toMatchObject({ caseInsensitive: false, normalizes: true });
    expect(calls).toEqual([]);
  });
});

describe("runBulkRename", () => {
  const rules: RenameRules = {
    ...DEFAULT_RULES,
    find: { ...DEFAULT_RULES.find, mode: "regex", pattern: "^IMG_", replacement: "trip-" },
  };
  const swap = [
    { from: "a.jpg", to: "b.jpg" },
    { from: "b.jpg", to: "a.jpg" },
    { from: "c.jpg", to: "d.jpg" },
  ];
  const held = ["a.jpg", "b.jpg", "c.jpg", "notes.txt"];
  const run = (renames = swap) =>
    runBulkRename({
      origin: server("sftp"),
      folder: "/srv/photos",
      renames,
      held,
      target: POSIX,
      rules,
    });
  const renamed = (from: string, to: string) => ({ from, to, path: `/srv/photos/${to}` });

  it("sends every rename in one call, with names that trade places going through a temporary one", async () => {
    answers.files_rename = (): RenameSummary => ({
      renamed: [renamed("b.jpg", "a.jpg"), renamed("a.jpg", "b.jpg"), renamed("c.jpg", "d.jpg")],
      cancelled: false,
    });
    await run();

    expect(asked("files_rename")).toHaveLength(1);
    const request = renameRequest();
    expect(request.location).toEqual({ kind: "remote", sessionId: "s1" });
    expect(request.directory).toBe("/srv/photos");
    expect(request.groups).toHaveLength(2);
    const [ring, single] = request.groups;
    expect(ring.renames).toEqual([swap[0], swap[1]]);
    expect(ring.temporary).toMatch(/^poros-rename-[0-9a-f]{8}-1\.tmp$/);
    expect(single).toEqual({ renames: [swap[2]] });

    expect(refreshes).toEqual([["/srv/photos/a.jpg", "/srv/photos/b.jpg", "/srv/photos/d.jpg"]]);
    expect(logged).toEqual(["Renamed 3 items in /srv/photos"]);
    expect(toasts).toEqual([]);
    expect(useBulkRenameStore.getState().report).toBeNull();
  });

  it("remembers the rules for the session and the pattern in the settings", async () => {
    answers.files_rename = (): RenameSummary => ({ renamed: [], cancelled: false });
    answers.settings_set = ({ value }) => value;
    await run();
    await run();

    expect(useBulkRenameStore.getState().rules).toBe(rules);
    const saved = asked("settings_set");
    expect(saved).toHaveLength(1);
    expect(useSettingsStore.getState().settings.interface.renameHistory).toEqual([
      {
        mode: "regex",
        pattern: "^IMG_",
        replacement: "trip-",
        caseSensitive: false,
        allMatches: true,
        scope: "name",
      },
    ]);
  });

  it("reports which items were renamed and which were not when a rename fails", async () => {
    answers.files_rename = (): RenameSummary => ({
      renamed: [renamed("b.jpg", "a.jpg"), renamed("a.jpg", "b.jpg")],
      failure: { from: "c.jpg", to: "d.jpg", message: "Permission denied" },
      cancelled: false,
    });
    await run();

    const report = useBulkRenameStore.getState().report;
    expect(report?.folder).toBe("/srv/photos");
    expect(report?.outcomes).toEqual([
      { ...swap[0], status: "renamed" },
      { ...swap[1], status: "renamed" },
      { ...swap[2], status: "failed", message: "Permission denied" },
    ]);
    expect(refreshes).toEqual([["/srv/photos/a.jpg", "/srv/photos/b.jpg"]]);
    expect(logged).toEqual([
      "Renaming stopped in /srv/photos with 2 of 3 items renamed",
      "c.jpg to d.jpg: Permission denied",
    ]);
  });

  it("says where an item was left when it could not get its name back", async () => {
    answers.files_rename = (): RenameSummary => ({
      renamed: [],
      failure: { from: "a.jpg", to: "b.jpg", message: "Permission denied" },
      stranded: { from: "a.jpg", to: "b.jpg", temporary: "poros-1.tmp", message: "It is locked" },
      cancelled: false,
    });
    await run();

    expect(useBulkRenameStore.getState().report?.outcomes[0]).toEqual({
      ...swap[0],
      status: "stranded",
      message: "Now named poros-1.tmp: It is locked",
    });
    expect(logged).toContain("a.jpg is left named poros-1.tmp: It is locked");
  });

  it("says how far a cancelled run got, without a report", async () => {
    answers.files_rename = (): RenameSummary => ({
      renamed: [renamed("c.jpg", "d.jpg")],
      cancelled: true,
    });
    await run();

    expect(toasts).toEqual(["Renaming cancelled. 1 of 3 items renamed."]);
    expect(useBulkRenameStore.getState().report).toBeNull();
    expect(refreshes).toEqual([["/srv/photos/d.jpg"]]);
  });

  it("shows why nothing ran when the backend refuses the request, and reloads the folder", async () => {
    answers.files_rename = () => {
      throw { kind: "invalidInput", message: '"a/b" is not a valid name' };
    };
    await run();

    expect(toasts).toEqual(['"a/b" is not a valid name']);
    expect(refreshes).toEqual([[]]);
    expect(useBulkRenameStore.getState().report).toBeNull();
  });

  it("does nothing without renames", async () => {
    await run([]);
    expect(calls).toEqual([]);
    expect(refreshes).toEqual([]);
  });
});
