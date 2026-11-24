// Keeps the window's language on the one Settings names, here and in every other window, as
// each window loads the settings again when one saves them.

import { setLanguage, SYSTEM_LANGUAGE } from "../i18n";
import { useSettingsStore } from "./settingsStore";

/** Until the settings load, the language the last run used stays, so nothing flickers. */
export function followLanguageSetting(): () => void {
  const apply = () => {
    const { loaded, settings } = useSettingsStore.getState();
    if (loaded) setLanguage(settings.interface.language);
  };
  apply();
  const unsubscribe = useSettingsStore.subscribe((state, previous) => {
    if (
      state.loaded !== previous.loaded ||
      state.settings.interface.language !== previous.settings.interface.language
    ) {
      apply();
    }
  });
  // System follows the computer when its language changes while Poros runs.
  const followSystem = () => {
    if (useSettingsStore.getState().settings.interface.language === SYSTEM_LANGUAGE) apply();
  };
  window.addEventListener("languagechange", followSystem);
  return () => {
    unsubscribe();
    window.removeEventListener("languagechange", followSystem);
  };
}
