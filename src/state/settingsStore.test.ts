import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_SETTINGS, type Settings } from "../lib/settings";
import { previewSettingsSection, saveSettingsSection, useSettingsStore } from "./settingsStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

interface Call {
  command: string;
  value?: Settings;
  answer: (value: unknown) => void;
}

/** Commands sent to the backend and not answered yet, oldest first. */
let calls: Call[] = [];

function answer(command: string, value?: Settings): Settings | undefined {
  const index = calls.findIndex((call) => call.command === command);
  const [call] = calls.splice(index, 1);
  // The backend answers a save with what it stored, which is what it was sent.
  const answered = value ?? call.value;
  call.answer(answered);
  return answered;
}

/** Lets the store act on the answers given so far. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
const shown = () => useSettingsStore.getState().settings;

beforeEach(() => {
  calls = [];
  invoke.mockImplementation(
    (command: string, args?: { value?: Settings }) =>
      new Promise((resolve) => calls.push({ command, value: args?.value, answer: resolve })),
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
    const stored = answer("settings_set");
    await settle();
    const expected = {
      fontSize: 14,
      rowHeight: 30,
      stripedRows: !DEFAULT_SETTINGS.appearance.stripedRows,
    };
    expect(shown().appearance).toMatchObject(expected);
    expect(stored?.appearance).toMatchObject(expected);
  });

  it("ignores a reload that set out before a save was answered", async () => {
    void saveSettingsSection("appearance", { fontSize: 14 });
    // The backend announces each save, and every window reads the settings again.
    void useSettingsStore.getState().load();
    answer("settings_get", DEFAULT_SETTINGS);
    await settle();
    expect(shown().appearance.fontSize).toBe(14);
    answer("settings_set");
    await settle();
    expect(shown().appearance.fontSize).toBe(14);
  });

  it("ignores a reload that a change overtook", async () => {
    void useSettingsStore.getState().load();
    void saveSettingsSection("appearance", { fontSize: 14 });
    answer("settings_get", DEFAULT_SETTINGS);
    await settle();
    expect(shown().appearance.fontSize).toBe(14);
    answer("settings_set");
    await settle();
  });

  it("takes a reload with nothing of its own on the way, as another window's change", async () => {
    const elsewhere = {
      ...DEFAULT_SETTINGS,
      appearance: { ...DEFAULT_SETTINGS.appearance, fontSize: 18 },
    };
    void saveSettingsSection("appearance", { fontSize: 14 });
    answer("settings_set");
    await settle();
    void useSettingsStore.getState().load();
    answer("settings_get", elsewhere);
    await settle();
    expect(shown().appearance.fontSize).toBe(18);
  });

  it("keeps what is typed after a save set out, until that is saved too", async () => {
    previewSettingsSection("sync", { arguments: "--bwlimit" });
    void saveSettingsSection("sync", {});
    previewSettingsSection("sync", { arguments: "--bwlimit=2m" });
    void useSettingsStore.getState().load();
    const stored = answer("settings_set");
    answer("settings_get", stored);
    await settle();
    expect(shown().sync.arguments).toBe("--bwlimit=2m");
  });
});
