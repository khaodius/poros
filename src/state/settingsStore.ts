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

/** Saves set out from this window, counted so that an answer or a reload that set out before
 * the latest of them cannot put older settings back on show. */
let saves = 0;
/** Saves the backend has not answered yet, which a reload may have read the settings from before. */
let unansweredSaves = 0;
/** Changes shown since the latest save set out, such as a slider on its way, which that save
 * did not carry: whatever is read from the backend is shown with them on top. */
let previews: ((settings: Settings) => Settings)[] = [];
/** A reload was passed over, or a save failed, so the settings are read again once every save
 * is answered: another window, or the backend itself, may have changed them meanwhile. */
let reloadOwed = false;

const withPreviews = (stored: Settings) =>
  previews.reduce((settings, preview) => preview(settings), stored);

function withSection<K extends keyof Settings>(
  settings: Settings,
  section: K,
  change: Partial<Settings[K]>,
): Settings {
  return { ...settings, [section]: { ...settings[section], ...change } };
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: DEFAULT_SETTINGS,
  loaded: false,
  load: async () => {
    const savesBefore = saves;
    const unansweredBefore = unansweredSaves;
    const stored = await settingsIpc.get().catch(() => undefined);
    if (unansweredBefore === 0 && saves === savesBefore) {
      reloadOwed = false;
      set({ settings: withPreviews(sanitizeSettings(stored)), loaded: true });
      return;
    }
    // What it read may be older than what is on show; it is read again once the saves are in.
    reloadOwed = true;
    set({ loaded: true });
    if (unansweredSaves === 0) void get().load();
  },
  save: async (next) => {
    saves += 1;
    const save = saves;
    unansweredSaves += 1;
    previews = [];
    set({ settings: next });
    try {
      const saved = await settingsIpc.set(next);
      if (save === saves) set({ settings: withPreviews(sanitizeSettings(saved)) });
    } catch (caught) {
      // What is on show was never stored.
      reloadOwed = true;
      throw caught;
    } finally {
      unansweredSaves -= 1;
      if (unansweredSaves === 0 && reloadOwed) void get().load();
    }
  },
}));

/** Replaces one section of the settings, for settings pages that edit a single field. */
export function saveSettingsSection<K extends keyof Settings>(
  section: K,
  change: Partial<Settings[K]>,
): Promise<void> {
  const { settings, save } = useSettingsStore.getState();
  return save(withSection(settings, section, change));
}

/** Shows a change in this window without saving it, for sliders while they move. */
export function previewSettingsSection<K extends keyof Settings>(
  section: K,
  change: Partial<Settings[K]>,
): void {
  previews.push((settings) => withSection(settings, section, change));
  useSettingsStore.setState(({ settings }) => ({
    settings: withSection(settings, section, change),
  }));
}
