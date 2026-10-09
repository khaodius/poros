import { terminal } from "../lib/ipc";
import { findTab, terminalTab } from "../lib/layout";
import { useLayoutStore } from "./layoutStore";
import { useSessionStore } from "./sessionStore";

/** Opens a shell on a session's server, beside the pane it was opened from. */
export function openTerminal(sessionId: string, nearGroupId?: string): void {
  const session = useSessionStore.getState().sessions[sessionId];
  if (!session) return;
  useLayoutStore.getState().openTab(terminalTab(sessionId, session.info.label), nearGroupId);
}

/** Closes a terminal tab and ends its shell. */
export function closeTerminalTab(tabId: string): void {
  const tab = findTab(useLayoutStore.getState().root, tabId);
  if (tab?.kind !== "terminal") return;
  useLayoutStore.getState().closeTab(tabId);
  if (tab.terminalId) void terminal.close(tab.terminalId).catch(() => undefined);
}
