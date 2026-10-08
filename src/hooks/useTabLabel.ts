import type { PaneTab } from "../lib/layout";
import { useSessionStore } from "../state/sessionStore";

export function useTabLabel(tab: PaneTab | null): string {
  const sessionLabel = useSessionStore((state) =>
    tab?.kind === "remote" ? state.sessions[tab.sessionId]?.info.label : undefined,
  );
  if (!tab) return "";
  if (tab.kind === "local") return "Local";
  if (tab.kind === "welcome") return "New tab";
  if (tab.kind === "editor") return tab.name;
  if (tab.kind === "terminal") return tab.label;
  return sessionLabel ?? "Closed session";
}
