import { useEffect, useMemo, useRef, useState } from "react";
import { HardDrive, LogOut, RotateCw, Server } from "lucide-react";
import { ConnectDialog } from "./components/ConnectDialog";
import { FilePane } from "./components/FilePane";
import { HostKeyDialog } from "./components/HostKeyDialog";
import { LogPanel } from "./components/LogPanel";
import { QuickConnectBar } from "./components/QuickConnectBar";
import { SplitView } from "./components/SplitView";
import { Toasts } from "./components/Toasts";
import { EMPTY_DRAFT, type ConnectDraft } from "./lib/connectDraft";
import { localSource, remoteSource } from "./lib/fileSource";
import { formatVersion } from "./lib/format";
import { onLog, onSessionClosed } from "./lib/ipc";
import { connectWithPrompts } from "./state/connectFlow";
import { useConnectionStore } from "./state/connectionStore";
import { useLogStore } from "./state/logStore";
import { useToastStore } from "./state/toastStore";

type PaneSide = "local" | "remote";

const APP_VERSION = formatVersion(__APP_VERSION__);

export function App() {
  const [activePane, setActivePane] = useState<PaneSide>("local");
  const [connectDraft, setConnectDraft] = useState<ConnectDraft | null>(null);
  const session = useConnectionStore((state) => state.session);
  const status = useConnectionStore((state) => state.status);
  const [activatedSessionId, setActivatedSessionId] = useState<string | null>(null);
  if (session && session.id !== activatedSessionId) {
    setActivatedSessionId(session.id);
    setActivePane("remote");
  }
  const paneElements = useRef<Record<PaneSide, HTMLDivElement | null>>({
    local: null,
    remote: null,
  });
  const remote = useMemo(() => (session ? remoteSource(session) : null), [session]);

  useEffect(() => {
    const appendLog = useLogStore.getState().append;
    const markLost = useConnectionStore.getState().markLost;
    const subscriptions = [
      onLog(appendLog),
      onSessionClosed(({ sessionId, reason }) => markLost(sessionId, reason)),
    ];
    return () => {
      for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    const suppressNativeMenu = (event: MouseEvent) => {
      const target = event.target as HTMLElement;
      if (!target.closest("input, textarea, .selectable")) event.preventDefault();
    };
    window.addEventListener("contextmenu", suppressNativeMenu);
    return () => window.removeEventListener("contextmenu", suppressNativeMenu);
  }, []);

  const switchPane = (from: PaneSide) => {
    const target: PaneSide = from === "local" ? "remote" : "local";
    const list = paneElements.current[target]?.querySelector<HTMLElement>(".file-list-body");
    if (list) {
      list.focus();
      setActivePane(target);
    }
  };

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <img src="/poros.svg" alt="" width={20} height={20} />
          <span>Poros</span>
        </div>
        <QuickConnectBar onOpenDialog={setConnectDraft} />
        <SessionControls />
      </header>

      <main className="workspace">
        <SplitView
          direction="vertical"
          storageKey="poros.split.log"
          defaultRatio={0.78}
          minRatio={0.4}
          maxRatio={0.92}
          first={
            <SplitView
              direction="horizontal"
              storageKey="poros.split.panes"
              defaultRatio={0.5}
              first={
                <div
                  className="pane-slot"
                  ref={(element) => void (paneElements.current.local = element)}
                >
                  <FilePane
                    title="Local"
                    icon={<HardDrive size={15} />}
                    source={localSource}
                    active={activePane === "local"}
                    onActivate={() => setActivePane("local")}
                    onSwitchPane={() => switchPane("local")}
                  />
                </div>
              }
              second={
                <div
                  className="pane-slot"
                  ref={(element) => void (paneElements.current.remote = element)}
                >
                  {remote && status !== "lost" ? (
                    <FilePane
                      key={remote.key}
                      title={remote.label}
                      icon={<Server size={15} />}
                      source={remote}
                      active={activePane === "remote"}
                      onActivate={() => setActivePane("remote")}
                      onSwitchPane={() => switchPane("remote")}
                    />
                  ) : (
                    <RemotePlaceholder onConnect={() => setConnectDraft(EMPTY_DRAFT)} />
                  )}
                </div>
              }
            />
          }
          second={<LogPanel />}
        />
      </main>

      <StatusBar />

      {connectDraft && (
        <ConnectDialog initialDraft={connectDraft} onClose={() => setConnectDraft(null)} />
      )}
      <HostKeyDialog />
      <Toasts />
    </div>
  );
}

function SessionControls() {
  const status = useConnectionStore((state) => state.status);
  const disconnect = useConnectionStore((state) => state.disconnect);
  if (status !== "connected" && status !== "lost") return null;
  return (
    <button type="button" className="button" onClick={() => void disconnect()}>
      <LogOut size={14} />
      Disconnect
    </button>
  );
}

function RemotePlaceholder({ onConnect }: { onConnect: () => void }) {
  const status = useConnectionStore((state) => state.status);
  const lostReason = useConnectionStore((state) => state.lostReason);
  const lastProfile = useConnectionStore((state) => state.lastProfile);
  const cancelConnect = useConnectionStore((state) => state.cancelConnect);
  const showToast = useToastStore((state) => state.show);

  const reconnect = async () => {
    if (!lastProfile) return;
    const failure = await connectWithPrompts(lastProfile);
    if (failure) showToast("error", failure.message);
  };

  return (
    <section className="pane">
      <header className="pane-header">
        <div className="pane-title">
          <Server size={15} />
          <span>Remote</span>
        </div>
      </header>
      <div className="pane-empty">
        {status === "connecting" && (
          <>
            <p className="pane-empty-detail">Connecting...</p>
            <button type="button" className="button" onClick={cancelConnect}>
              Cancel
            </button>
          </>
        )}
        {status === "disconnected" && (
          <>
            <Server size={40} className="pane-empty-icon" />
            <p className="pane-empty-title">Not connected</p>
            <p className="pane-empty-detail">
              Enter a host in the bar above, or open the full connection dialog for key files and
              SSH agents.
            </p>
            <button type="button" className="button button-primary" onClick={onConnect}>
              Connect to server
            </button>
          </>
        )}
        {status === "lost" && (
          <>
            <p className="pane-empty-title">Connection lost</p>
            {lostReason && <p className="pane-empty-detail">{lostReason}</p>}
            <button type="button" className="button button-primary" onClick={reconnect}>
              <RotateCw size={14} />
              Reconnect
            </button>
          </>
        )}
      </div>
    </section>
  );
}

function StatusBar() {
  const status = useConnectionStore((state) => state.status);
  const session = useConnectionStore((state) => state.session);
  const text = {
    disconnected: "Not connected",
    connecting: "Connecting...",
    connected: session ? `Connected to ${session.label}` : "Connected",
    lost: "Connection lost",
  }[status];
  return (
    <footer className="statusbar">
      <span className="status-item">
        <span className={`status-dot status-${status}`} />
        {text}
      </span>
      <span className="status-item status-version">Poros {APP_VERSION}</span>
    </footer>
  );
}
