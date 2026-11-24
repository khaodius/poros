import type { TranslatedArea } from "../catalog";

export const terminal = {
  // components/TerminalPane
  couldNotReadTheClipboard:
    "Die Zwischenablage konnte nicht gelesen werden. Fügen Sie stattdessen mit Shift+Insert ein.",
  selectAll: "Alles auswählen",
  couldNotOpenATerminal: "Terminal konnte nicht geöffnet werden",
  closeTab: "Tab schließen",
  restart: "Neu starten",
  connecting: "Wird verbunden...",
  connected: "Verbunden",
} satisfies TranslatedArea<"terminal">;
