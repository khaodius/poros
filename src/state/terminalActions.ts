import { terminal } from "../lib/ipc";
import { findTab, terminalTab } from "../lib/layout";
import type { TerminalEvent } from "../lib/types";
import { useLayoutStore } from "./layoutStore";
import { useSessionStore } from "./sessionStore";

/** Opens a shell on a session's server, beside the pane it was opened from. */
export function openTerminal(sessionId: string, nearGroupId?: string): void {
  const session = useSessionStore.getState().sessions[sessionId];
  if (!session) return;
  useLayoutStore.getState().openTab(terminalTab(sessionId, session.info.label), nearGroupId);
}

/** The size of a terminal on screen, read when needed, as it follows its pane. */
export interface TerminalScreen {
  readonly cols: number;
  readonly rows: number;
}

/**
 * Starts the shell of a terminal tab at the size its terminal has, and gives the tab the shell.
 * A size the terminal took while the shell opened, which no shell was there to hear, is sent
 * once it is open. Resolves the shell's id, or null with the shell closed again when the tab
 * went away or is `abandoned` meanwhile.
 */
export async function openShell(
  tabId: string,
  screen: TerminalScreen,
  onEvent: (event: TerminalEvent) => void,
  abandoned: () => boolean,
): Promise<string | null> {
  const tab = findTab(useLayoutStore.getState().root, tabId);
  if (tab?.kind !== "terminal") return null;
  const opened = { cols: screen.cols, rows: screen.rows };
  const info = await terminal.open(tab.sessionId, opened.cols, opened.rows, onEvent);
  const layout = useLayoutStore.getState();
  const current = findTab(layout.root, tabId);
  if (abandoned() || current?.kind !== "terminal") {
    void terminal.close(info.id).catch(() => undefined);
    return null;
  }
  layout.replaceTab(tabId, { ...current, terminalId: info.id });
  if (screen.cols !== opened.cols || screen.rows !== opened.rows) {
    void terminal.resize(info.id, screen.cols, screen.rows).catch(() => undefined);
  }
  return info.id;
}

/** Closes a terminal tab and ends its shell. */
export function closeTerminalTab(tabId: string): void {
  const tab = findTab(useLayoutStore.getState().root, tabId);
  if (tab?.kind !== "terminal") return;
  useLayoutStore.getState().closeTab(tabId);
  if (tab.terminalId) void terminal.close(tab.terminalId).catch(() => undefined);
}
