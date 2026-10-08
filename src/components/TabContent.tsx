import { memo, useMemo, useState } from "react";
import { HardDrive, RotateCw, Unplug } from "lucide-react";
import { localSource, remoteSource, startingAt } from "../lib/fileSource";
import type { PaneTab } from "../lib/layout";
import { reconnectWithPrompts } from "../state/connectFlow";
import { useLayoutStore } from "../state/layoutStore";
import { getPane } from "../state/paneRegistry";
import { useSessionStore } from "../state/sessionStore";
import { ConnectionIcon } from "./ConnectionIcon";
import { FilePane } from "./FilePane";
import { WelcomePanel } from "./WelcomePanel";

interface TabContentProps {
  tab: PaneTab;
  visible: boolean;
  active: boolean;
}

/** Memoized, since the dock re-renders on every pointer move while a pane is dragged. */
export const TabContent = memo(function TabContent({ tab, visible, active }: TabContentProps) {
  if (tab.kind === "local") return <LocalTab tab={tab} visible={visible} active={active} />;
  if (tab.kind === "remote") return <RemoteTab tab={tab} visible={visible} active={active} />;
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
      icon={<ConnectionIcon protocol={entry.info.protocol} size={15} />}
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

  const reconnect = async () => {
    setBusy(true);
    setError(null);
    const result = await reconnectWithPrompts(sessionId);
    setBusy(false);
    if ("error" in result) {
      setError(result.error.message);
      return;
    }
    const path = getPane(tabId)?.path() ?? undefined;
    replaceTab(tabId, { id: tabId, kind: "remote", sessionId: result.session.id, path });
  };

  return (
    <div className="pane-overlay">
      <Unplug size={36} className="pane-empty-icon" />
      <p className="pane-empty-title">Connection lost</p>
      {reason && <p className="pane-empty-detail">{reason}</p>}
      {error && <p className="form-error">{error}</p>}
      <button type="button" className="button button-primary" onClick={reconnect} disabled={busy}>
        <RotateCw size={14} className={busy ? "spin" : ""} />
        {busy ? "Reconnecting..." : "Reconnect"}
      </button>
    </div>
  );
}
