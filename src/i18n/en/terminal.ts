import type { MessageArea } from "../catalog";

export const terminal = {
  // components/TerminalPane
  couldNotReadTheClipboard: "Could not read the clipboard. Paste with Shift+Insert instead.",
  selectAll: "Select all",
  couldNotOpenATerminal: "Could not open a terminal",
  closeTab: "Close tab",
  restart: "Restart",
} as const satisfies MessageArea;
