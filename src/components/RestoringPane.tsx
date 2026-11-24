import { useEffect, useLayoutEffect, useRef } from "react";
import { FileText, KeyRound, LoaderCircle, RotateCw } from "lucide-react";
import type { RestoringTab } from "../lib/layout";
import { useLayoutStore } from "../state/layoutStore";
import {
  reconnect,
  serverKey,
  signInToRestore,
  startRestoring,
  useRestoreStore,
} from "../state/restoreActions";
import { ConnectionIcon } from "./ConnectionIcon";
import { useT } from "../i18n";

/** What the tab shows once it is back, for a tab that needs its server first. */
function reopens(tab: RestoringTab): string {
  if (tab.kind === "editor") return `Reopens ${tab.name} once connected.`;
  if (tab.kind === "terminal") return "Opens a new shell once connected.";
  return tab.path ? `Reopens ${tab.path} once connected.` : "Reopens once connected.";
}

/**
 * Stands in for a tab put back from the last time Poros ran until its server is connected again,
 * or its file is open.
 */
export function RestoringPane({ tab }: { tab: RestoringTab }) {
  const t = useT();
  const server = tab.restore.server;
  const key = serverKey(tab);
  const status = useRestoreStore((state) => state.servers[key]);
  const closeTab = useLayoutStore((state) => state.closeTab);
  const tabRef = useRef(tab);
  useLayoutEffect(() => {
    tabRef.current = tab;
  });
  // Once per tab, which a move to another group or window mounts again.
  const tabId = tab.id;
  useEffect(() => startRestoring(tabRef.current), [tabId]);

  if (!server) {
    return (
      <section className="pane">
        <div className="pane-empty restoring-pane">
          <FileText size={36} className="pane-empty-icon" />
          <p className="pane-empty-title">Opening {tab.kind === "editor" ? tab.name : "file"}...</p>
        </div>
      </section>
    );
  }

  const phase = status?.phase ?? "connecting";
  const title =
    phase === "connecting"
      ? `Reconnecting to ${server.label}...`
      : phase === "signIn"
        ? `Sign in to ${server.label}`
        : phase === "failed"
          ? `Could not reconnect to ${server.label}`
          : server.label;
  const detail =
    status?.phase === "failed" || (status?.phase === "signIn" && status.message)
      ? status.message
      : phase === "signIn"
        ? "Poros keeps no password for this server."
        : null;
  return (
    <section className="pane">
      <div className="pane-empty restoring-pane">
        {phase === "connecting" ? (
          <LoaderCircle size={36} className="pane-empty-icon spin" />
        ) : (
          <ConnectionIcon protocol={server.protocol} size={36} className="pane-empty-icon" />
        )}
        <p className="pane-empty-title">{title}</p>
        {detail && <p className="pane-empty-detail">{detail}</p>}
        <div className="pane-overlay-actions">
          {phase === "signIn" && (
            <button
              type="button"
              className="button button-primary"
              onClick={() => signInToRestore(tab)}
            >
              <KeyRound size={14} />
              {t("workspace.signIn")}
            </button>
          )}
          {(phase === "failed" || phase === "waiting") && (
            <button
              type="button"
              className="button button-primary"
              onClick={() => void reconnect(key, server)}
            >
              <RotateCw size={14} />
              {phase === "waiting" ? t("common.connect") : t("common.tryAgain")}
            </button>
          )}
          <button type="button" className="button" onClick={() => closeTab(tab.id)}>
            {t("workspace.closeTab")}
          </button>
        </div>
        <p className="pane-empty-detail">{reopens(tab)}</p>
      </div>
    </section>
  );
}
