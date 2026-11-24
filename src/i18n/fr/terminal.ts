import type { TranslatedArea } from "../catalog";

export const terminal = {
  // components/TerminalPane
  couldNotReadTheClipboard:
    "Impossible de lire le presse-papiers. Collez plutôt avec Shift+Insert.",
  selectAll: "Tout sélectionner",
  couldNotOpenATerminal: "Impossible d’ouvrir un terminal",
  closeTab: "Fermer l’onglet",
  restart: "Redémarrer",
  connecting: "Connexion...",
  connected: "Connecté",
} satisfies TranslatedArea<"terminal">;
