import { useState } from "react";
import { HardDrive, KeyRound, Lock, Plus, Server, UserRound } from "lucide-react";
import { EMPTY_DRAFT } from "../lib/connectDraft";
import type { SavedConnection } from "../lib/types";
import { connectSaved } from "../state/connectActions";
import { useLayoutStore } from "../state/layoutStore";
import { byRecentUse, useSavedConnections } from "../state/savedConnectionsStore";
import { useUiStore } from "../state/uiStore";
import { PorosLogo } from "./PorosLogo";

const AUTH_ICONS = { password: Lock, publicKey: KeyRound, agent: UserRound };

export function WelcomePanel({ tabId }: { tabId: string }) {
  const connections = useSavedConnections((state) => state.connections);
  const replaceTab = useLayoutStore((state) => state.replaceTab);
  const openDialog = useUiStore((state) => state.open);
  const [connectingId, setConnectingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const connect = async (connection: SavedConnection) => {
    if (connectingId) return;
    setConnectingId(connection.id);
    setError(null);
    const failure = await connectSaved(connection, tabId);
    setConnectingId(null);
    if (failure) setError(failure.message);
  };

  return (
    <section className="pane welcome">
      <div className="welcome-body">
        <PorosLogo size={44} className="welcome-logo" />
        <h2 className="welcome-title">New tab</h2>
        <div className="welcome-actions">
          <button
            type="button"
            className="button button-primary"
            onClick={() => openDialog({ kind: "connect", draft: EMPTY_DRAFT, targetTabId: tabId })}
          >
            <Plus size={14} />
            Connect to server
          </button>
          <button
            type="button"
            className="button"
            onClick={() => replaceTab(tabId, { id: tabId, kind: "local" })}
          >
            <HardDrive size={14} />
            Local files
          </button>
        </div>

        <div className="welcome-saved">
          <h3>Saved connections</h3>
          {error && <p className="form-error">{error}</p>}
          {connections.length === 0 ? (
            <p className="welcome-hint">
              Servers you save from the connect dialog, or from the bookmark button of a server tab,
              appear here.
            </p>
          ) : (
            <ul className="saved-list">
              {byRecentUse(connections).map((connection) => {
                const AuthIcon = AUTH_ICONS[connection.authType];
                const connecting = connectingId === connection.id;
                return (
                  <li key={connection.id}>
                    <button
                      type="button"
                      className="saved-item"
                      disabled={connectingId !== null}
                      onClick={() => void connect(connection)}
                    >
                      <Server size={16} className="saved-item-icon" />
                      <span className="saved-item-text">
                        <span className="saved-item-name">{connection.name}</span>
                        <span className="saved-item-detail">
                          {connection.username}@{connection.host}
                          {connection.port === 22 ? "" : `:${connection.port}`}
                        </span>
                      </span>
                      {connecting ? (
                        <span className="saved-item-status">Connecting...</span>
                      ) : (
                        <AuthIcon size={14} className="saved-item-auth" />
                      )}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>
    </section>
  );
}
