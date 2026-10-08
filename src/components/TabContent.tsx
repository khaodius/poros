import { lazy, memo, Suspense, useMemo, useState } from "react";
import { HardDrive, RotateCw, Server, Unplug } from "lucide-react";
import { localSource, remoteSource, startingAt } from "../lib/fileSource";
import type { PaneTab } from "../lib/layout";
import { attemptUnderWay, useAutoReconnect } from "../hooks/useAutoReconnect";
import type { SessionInfo } from "../lib/types";
import { reconnectWithPrompts } from "../state/connectFlow";
import { useLayoutStore } from "../state/layoutStore";
import { getPane } from "../state/paneRegistry";
import { useSessionStore } from "../state/sessionStore";
import { FilePane } from "./FilePane";
import { WelcomePanel } from "./WelcomePanel";

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
  const path = tab.kind === "local" ? tab.path : undefined;
  const source = useMemo(() => startingAt(localSource, path), [path]);
  return (
    <FilePane
      tabId={tab.id}
      title="Local"
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
  const base = entry ? remoteSource(entry.info) : null;
  const source = useMemo(() => base && startingAt(base, path), [base, path]);

  if (!entry || !source) return <ClosedSession tabId={tab.id} />;
  return (
    <FilePane
      tabId={tab.id}
      title={entry.info.label}
      icon={<Server size={15} />}
      source={source}
      sessionId={sessionId}
      visible={visible}
      active={active}
      overlay={
        entry.status === "lost" && (
          <LostSession tabId={tab.id} sessionId={sessionId} reason={entry.lostReason} />
        )
      }
    />
  );
}

function ClosedSession({ tabId }: { tabId: string }) {
  const closeTab = useLayoutStore((state) => state.closeTab);
  return (
    <section className="pane">
      <div className="pane-empty">
        <Unplug size={36} className="pane-empty-icon" />
        <p className="pane-empty-title">This session has ended</p>
        <button type="button" className="button" onClick={() => closeTab(tabId)}>
          Close tab
        </button>
      </div>
    </section>
  );
}

function LostSession({
  tabId,
  sessionId,
  reason,
}: {
  tabId: string;
  sessionId: string;
  reason?: string;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const replaceTab = useLayoutStore((state) => state.replaceTab);
  const showSession = (session: SessionInfo) => {
    const path = getPane(tabId)?.path() ?? undefined;
    replaceTab(tabId, { id: tabId, kind: "remote", sessionId: session.id, path });
  };
  const automatic = useAutoReconnect(sessionId, showSession);

  const reconnect = async () => {
    automatic.stop();
    setBusy(true);
    setError(null);
    const underWay = await attemptUnderWay(sessionId)?.catch(() => null);
    const result = underWay ? { session: underWay } : await reconnectWithPrompts(sessionId);
    setBusy(false);
    if ("error" in result) {
      setError(result.error.message);
      return;
    }
    showSession(result.session);
  };

  const shownError = error ?? automatic.lastError;
  return (
    <div className="pane-overlay">
      <Unplug size={36} className="pane-empty-icon" />
      <p className="pane-empty-title">Connection lost</p>
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
            Stop
          </button>
        )}
      </div>
    </div>
  );
}
