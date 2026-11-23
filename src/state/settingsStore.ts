import { create } from "zustand";
import { settingsStore as settingsIpc } from "../lib/ipc";
import { DEFAULT_SETTINGS, sanitizeSettings, type Settings } from "../lib/settings";

interface SettingsState {
  settings: Settings;
  loaded: boolean;
  load: () => Promise<void>;
  /** Applies at once and persists; every window reloads when the backend confirms. */
  save: (next: Settings) => Promise<void>;
}

/** Changes made in this window, saved or only shown, counted so that an answer or a reload that
 * set out before the latest of them cannot put older settings back on show. */
let changes = 0;
/** Saves the backend has not answered yet, which a reload may have read the settings from before. */
let unansweredSaves = 0;

export const useSettingsStore = create<SettingsState>((set) => ({
  settings: DEFAULT_SETTINGS,
  loaded: false,
  load: async () => {
    const changesBefore = changes;
    const savesBefore = unansweredSaves;
    const stored = await settingsIpc.get().catch(() => undefined);
    // What it read may be older than what is on show; the answers to the saves bring the rest.
    const outdated = savesBefore > 0 || changes !== changesBefore;
    set(outdated ? { loaded: true } : { settings: sanitizeSettings(stored), loaded: true });
  },
  save: async (next) => {
    changes += 1;
    const change = changes;
    unansweredSaves += 1;
    set({ settings: next });
    try {
      const saved = await settingsIpc.set(next);
      if (change === changes) set({ settings: sanitizeSettings(saved) });
    } finally {
      unansweredSaves -= 1;
    }
  },
}));

/** Replaces one section of the settings, for settings pages that edit a single field. */
export function saveSettingsSection<K extends keyof Settings>(
  section: K,
  change: Partial<Settings[K]>,
): Promise<void> {
  const { settings, save } = useSettingsStore.getState();
  return save({ ...settings, [section]: { ...settings[section], ...change } });
}

/** Shows a change in this window without saving it, for sliders while they move. */
export function previewSettingsSection<K extends keyof Settings>(
  section: K,
  change: Partial<Settings[K]>,
): void {
  changes += 1;
  useSettingsStore.setState(({ settings }) => ({
    settings: { ...settings, [section]: { ...settings[section], ...change } },
  }));
}
