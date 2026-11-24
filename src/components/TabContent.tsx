import { lazy, memo, Suspense, useMemo, useState } from "react";
import { HardDrive, RotateCw, Unplug } from "lucide-react";
import { localSource, remoteSource, startingAt } from "../lib/fileSource";
import { isRestoring, type PaneTab } from "../lib/layout";
import { attemptUnderWay, useAutoReconnect } from "../hooks/useAutoReconnect";
import { reconnectWithPrompts } from "../state/connectFlow";
import { useLayoutStore } from "../state/layoutStore";
import { serverEntry, useSessionStore } from "../state/sessionStore";
import { ConnectionIcon } from "./ConnectionIcon";
import { FilePane } from "./FilePane";
import { RestoringPane } from "./RestoringPane";
import { WelcomePanel } from "./WelcomePanel";
import { useT } from "../i18n";

// Loaded on first use, so the editor and terminal cost nothing until one opens.
const EditorPane = lazy(() => import("./EditorPane"));
const TerminalPane = lazy(() => import("./TerminalPane"));

interface TabContentProps {
  tab: PaneTab;
  visible: boolean;
  active: boolean;
}

/** Memoized, since the dock re-renders on every pointer move while a pane is dragged. */
export const TabContent = memo(function TabContent({ tab, visible, active }: TabContentProps) {
  if (isRestoring(tab)) return <RestoringPane tab={tab} />;
  if (tab.kind === "local") return <LocalTab tab={tab} visible={visible} active={active} />;
  if (tab.kind === "remote") return <RemoteTab tab={tab} visible={visible} active={active} />;
  if (tab.kind === "editor" || tab.kind === "terminal") {
    return (
      <Suspense fallback={<section className="pane" />}>
        {tab.kind === "editor" ? (
          <EditorPane tab={tab} visible={visible} active={active} />
        ) : (
          <TerminalPane tab={tab} visible={visible} active={active} />
        )}
      </Suspense>
    );
  }
  return <WelcomePanel tabId={tab.id} />;
});

function LocalTab({ tab, visible, active }: TabContentProps) {
  const t = useT();
  const path = tab.kind === "local" ? tab.path : undefined;
  const source = useMemo(() => startingAt(localSource, path), [path]);
  return (
    <FilePane
      tabId={tab.id}
      title={t("workspace.local")}
      icon={<HardDrive size={15} />}
      source={source}
      visible={visible}
      active={active}
    />
  );
}

function RemoteTab({ tab, visible, active }: TabContentProps) {
  const sessionId = tab.kind === "remote" ? tab.sessionId : "";
  const path = tab.kind === "remote" ? tab.path : undefined;
  const entry = useSessionStore((state) => state.sessions[sessionId]);
  // An unlocked vault is reached over its server's connection: lost and back with it.
  const server = useSessionStore((state) => serverEntry(state.sessions, sessionId));
  const base = entry ? remoteSource(entry.info, entry.profile) : null;
  const source = useMemo(() => base && startingAt(base, path), [base, path]);

  if (!entry || !server || !source) return <ClosedSession tabId={tab.id} />;
  return (
    <FilePane
      tabId={tab.id}
      title={entry.info.label}
      icon={<ConnectionIcon protocol={entry.info.protocol} size={15} />}
      source={source}
      sessionId={sessionId}
      visible={visible}
      active={active}
      overlay={
        server.status === "lost" && (
          <LostSession sessionId={server.info.id} reason={server.lostReason} />
        )
      }
    />
  );
}

function ClosedSession({ tabId }: { tabId: string }) {
  const t = useT();
  const closeTab = useLayoutStore((state) => state.closeTab);
  return (
    <section className="pane">
      <div className="pane-empty">
        <Unplug size={36} className="pane-empty-icon" />
        <p className="pane-empty-title">{t("workspace.thisSessionHasEnded")}</p>
        <button type="button" className="button" onClick={() => closeTab(tabId)}>
          {t("workspace.closeTab")}
        </button>
      </div>
    </section>
  );
}

/**
 * Covers a pane whose server is out of reach, and reconnects to that server. The session store
 * then shows the new session in the tabs that showed the lost one, and reloads the panes in its
 * unlocked vaults, so the overlay only has to leave.
 */
function LostSession({ sessionId, reason }: { sessionId: string; reason?: string }) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const automatic = useAutoReconnect(sessionId);

  const reconnect = async () => {
    automatic.stop();
    setBusy(true);
    setError(null);
    const underWay = await attemptUnderWay(sessionId)?.catch(() => null);
    const result = underWay ? { session: underWay } : await reconnectWithPrompts(sessionId);
    setBusy(false);
    if ("error" in result) setError(result.error.message);
  };

  const shownError = error ?? automatic.lastError;
  return (
    <div className="pane-overlay">
      <Unplug size={36} className="pane-empty-icon" />
      <p className="pane-empty-title">{t("workspace.connectionLost")}</p>
      {reason && <p className="pane-empty-detail">{reason}</p>}
      {automatic.active && (
        <p className="pane-empty-detail">
          {automatic.secondsLeft > 0
            ? `Reconnecting in ${automatic.secondsLeft} s`
            : "Reconnecting..."}
          {automatic.failures > 0 && ` (attempt ${automatic.failures + 1})`}
        </p>
      )}
      {shownError && <p className="form-error">{shownError}</p>}
      <div className="pane-overlay-actions">
        <button type="button" className="button button-primary" onClick={reconnect} disabled={busy}>
          <RotateCw size={14} className={busy ? "spin" : ""} />
          {busy ? "Reconnecting..." : automatic.active ? "Reconnect now" : "Reconnect"}
        </button>
        {automatic.active && (
          <button type="button" className="button" onClick={automatic.stop}>
            {t("common.stop")}
          </button>
        )}
      </div>
    </div>
  );
}
