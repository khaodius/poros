import type { TranslatedArea } from "../catalog";

export const terminal = {
  // components/TerminalPane
  couldNotReadTheClipboard: "No se pudo leer el portapapeles. Pega con Shift+Insert en su lugar.",
  selectAll: "Seleccionar todo",
  couldNotOpenATerminal: "No se pudo abrir un terminal",
  closeTab: "Cerrar pestaña",
  restart: "Reiniciar",
  connecting: "Conectando...",
  connected: "Conectado",
} satisfies TranslatedArea<"terminal">;
