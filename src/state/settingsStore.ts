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

export const useSettingsStore = create<SettingsState>((set) => ({
  settings: DEFAULT_SETTINGS,
  loaded: false,
  load: async () => {
    const stored = await settingsIpc.get().catch(() => undefined);
    set({ settings: sanitizeSettings(stored), loaded: true });
  },
  save: async (next) => {
    set({ settings: next });
    const saved = await settingsIpc.set(next);
    set({ settings: sanitizeSettings(saved) });
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
  useSettingsStore.setState(({ settings }) => ({
    settings: { ...settings, [section]: { ...settings[section], ...change } },
  }));
}
