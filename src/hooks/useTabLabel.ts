import type { PaneTab } from "../lib/layout";
import { useSessionStore } from "../state/sessionStore";
import { useT } from "../i18n";

export function useTabLabel(tab: PaneTab | null): string {
  const t = useT();
  // A tab in an unlocked vault is named after the vault; its pane's title says where it is.
  const sessionLabel = useSessionStore((state) => {
    const info = tab?.kind === "remote" ? state.sessions[tab.sessionId]?.info : undefined;
    return info?.vault?.name ?? info?.label;
  });
  if (!tab) return "";
  if (tab.kind === "local") return t("workspace.local");
  if (tab.kind === "welcome") return t("workspace.newTab");
  if (tab.kind === "editor") return tab.name;
  if (tab.kind === "terminal") return tab.label;
  return sessionLabel ?? tab.restore?.server?.label ?? t("workspace.closedSession");
}
