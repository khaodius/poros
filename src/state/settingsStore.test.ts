import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS, type Settings } from "../lib/settings";
import { previewSettingsSection, saveSettingsSection, useSettingsStore } from "./settingsStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

interface Call {
  command: string;
  value?: Settings;
  answer: (value: unknown) => void;
  fail: (error: unknown) => void;
}

/** Commands sent to the backend and not answered yet, oldest first. */
let calls: Call[] = [];
/** What the backend has stored, which every window reads. */
let stored: Settings;

function take(command: string): Call {
  const index = calls.findIndex((call) => call.command === command);
  if (index < 0) throw new Error(`Nobody waits for ${command}`);
  return calls.splice(index, 1)[0];
}

/** Answers the oldest call of `command`: a save is stored, a read gets what is stored. */
function answer(command: "settings_set" | "settings_get"): void {
  const call = take(command);
  if (call.value) stored = call.value;
  call.answer(stored);
}

/** Another window, or the backend itself, stores a change. */
function storedElsewhere(change: (settings: Settings) => Settings): void {
  stored = change(stored);
}

const closeWhenDone = (settings: Settings): Settings => ({
  ...settings,
  automation: { ...settings.automation, whenDone: "close" },
});

/** Lets the store act on the answers given so far. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const shown = () => useSettingsStore.getState().settings;
const reload = () => void useSettingsStore.getState().load();

beforeEach(() => {
  calls = [];
  stored = DEFAULT_SETTINGS;
  invoke.mockImplementation(
    (command: string, args?: { value?: Settings }) =>
      new Promise((answer, fail) => calls.push({ command, value: args?.value, answer, fail })),
  );
  useSettingsStore.setState({ settings: DEFAULT_SETTINGS, loaded: true });
});

afterEach(() => {
  expect(calls).toEqual([]);
});

describe("settingsStore", () => {
  it("keeps a newer change on show when an older save is answered", async () => {
    void saveSettingsSection("appearance", { fontSize: 14 });
    void saveSettingsSection("appearance", { fontSize: 15 });
    answer("settings_set");
    await settle();
    expect(shown().appearance.fontSize).toBe(15);
    answer("settings_set");
    await settle();
    expect(shown().appearance.fontSize).toBe(15);
  });

  it("builds each save on the changes before it, answered or not", async () => {
    void saveSettingsSection("appearance", { fontSize: 14 });
    void saveSettingsSection("appearance", { rowHeight: 30 });
    answer("settings_set");
    await settle();
    void saveSettingsSection("appearance", {
      stripedRows: !DEFAULT_SETTINGS.appearance.stripedRows,
    });
    answer("settings_set");
    answer("settings_set");
    await settle();
    const expected = {
      fontSize: 14,
      rowHeight: 30,
      stripedRows: !DEFAULT_SETTINGS.appearance.stripedRows,
    };
    expect(shown().appearance).toMatchObject(expected);
    expect(stored.appearance).toMatchObject(expected);
  });

  it("passes over a reload that set out before a save was answered, and reads again after", async () => {
    void saveSettingsSection("appearance", { fontSize: 14 });
    // The backend announces each save, and every window reads the settings again.
    reload();
    take("settings_get").answer(DEFAULT_SETTINGS);
    await settle();
    expect(shown().appearance.fontSize).toBe(14);
    answer("settings_set");
    await settle();
    expect(shown().appearance.fontSize).toBe(14);
    answer("settings_get");
    await settle();
    expect(shown().appearance.fontSize).toBe(14);
  });

  it("passes over a reload that a save overtook, and reads again after", async () => {
    reload();
    void saveSettingsSection("appearance", { fontSize: 14 });
    answer("settings_get");
    await settle();
    expect(shown().appearance.fontSize).toBe(14);
    answer("settings_set");
    await settle();
    answer("settings_get");
    await settle();
    expect(shown().appearance.fontSize).toBe(14);
  });

  it("takes a reload with nothing of its own on the way, as another window's change", async () => {
    storedElsewhere((settings) => ({
      ...settings,
      appearance: { ...settings.appearance, fontSize: 18 },
    }));
    reload();
    answer("settings_get");
    await settle();
    expect(shown().appearance.fontSize).toBe(18);
  });

  it("keeps what is typed after a save set out, until that is saved too", async () => {
    previewSettingsSection("sync", { arguments: "--bwlimit" });
    void saveSettingsSection("sync", {});
    previewSettingsSection("sync", { arguments: "--bwlimit=2m" });
    reload();
    answer("settings_set");
    answer("settings_get");
    await settle();
    expect(shown().sync.arguments).toBe("--bwlimit=2m");
    // Read again, the save having been on its way.
    answer("settings_get");
    await settle();
    expect(shown().sync.arguments).toBe("--bwlimit=2m");
    void saveSettingsSection("sync", {});
    answer("settings_set");
    await settle();
    expect(stored.sync.arguments).toBe("--bwlimit=2m");
  });

  it("shows another window's change that a slider on its way overtook, and saves it along", async () => {
    storedElsewhere(closeWhenDone);
    reload();
    previewSettingsSection("appearance", { rowHeight: 30 });
    answer("settings_get");
    await settle();
    expect(shown().automation.whenDone).toBe("close");
    expect(shown().appearance.rowHeight).toBe(30);
    // The slider is let go.
    void saveSettingsSection("appearance", { rowHeight: 30 });
    answer("settings_set");
    await settle();
    expect(stored.automation.whenDone).toBe("close");
    expect(stored.appearance.rowHeight).toBe(30);
  });

  it("reads again a change made elsewhere while a save was answered", async () => {
    void saveSettingsSection("appearance", { fontSize: 14 });
    // Stored, and the answer on its way back, when another window stores its change.
    answer("settings_set");
    storedElsewhere(closeWhenDone);
    reload();
    answer("settings_get");
    await settle();
    answer("settings_get");
    await settle();
    expect(shown().automation.whenDone).toBe("close");
    expect(shown().appearance.fontSize).toBe(14);
  });

  it("shows what is stored once a save fails, rather than what it tried to store", async () => {
    const saved = saveSettingsSection("appearance", { fontSize: 14 });
    const failed = saveSettingsSection("appearance", { fontSize: 15 }).catch((error) => error);
    answer("settings_set");
    take("settings_set").fail(new Error("The disk is full"));
    await settle();
    await saved;
    expect(await failed).toMatchObject({ message: "The disk is full" });
    answer("settings_get");
    await settle();
    expect(shown().appearance.fontSize).toBe(14);
  });
});
