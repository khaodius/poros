import { useRef, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Plus } from "lucide-react";
import {
  EMPTY_DRAFT,
  authChoicesFor,
  canConnect,
  connectionDetail,
  draftFromSaved,
  draftSecret,
  profileFromDraft,
  savedFromDraft,
  withField,
  withProtocol,
  type AuthChoice,
  type ConnectDraft,
} from "../lib/connectDraft";
import { toAppError } from "../lib/ipc";
import { PROTOCOLS, isCloud, isFtp, protocolInfo } from "../lib/protocols";
import type { Protocol } from "../lib/types";
import { connectInTab } from "../state/connectActions";
import { byRecentUse, useSavedConnections } from "../state/savedConnectionsStore";
import { CloudSignIn } from "./CloudSignIn";
import { ConnectionIcon } from "./ConnectionIcon";
import { useSettingsStore } from "../state/settingsStore";
import { Dialog } from "./Dialog";

const AUTH_LABELS: Record<AuthChoice, string> = {
  password: "Password",
  publicKey: "Key file",
  agent: "SSH agent",
  oauth: "Browser sign-in",
};

interface ConnectDialogProps {
  initialDraft: ConnectDraft;
  targetTabId?: string;
  initialError?: string;
  onClose: () => void;
}

export function ConnectDialog({
  initialDraft,
  targetTabId,
  initialError,
  onClose,
}: ConnectDialogProps) {
  const connections = useSavedConnections((state) => state.connections);
  const saveConnection = useSavedConnections((state) => state.save);
  const removeConnection = useSavedConnections((state) => state.remove);
  const [draft, setDraft] = useState(initialDraft);
  const [busy, setBusy] = useState<"saving" | "connecting" | null>(null);
  const [error, setError] = useState<string | null>(initialError ?? null);
  const [needsPassphrase, setNeedsPassphrase] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const passphraseRef = useRef<HTMLInputElement>(null);
  const proxy = useSettingsStore((state) => state.settings.connection.proxy);
  const proxyEnabled = proxy.kind !== "none" && proxy.host.trim() !== "";
  // Only SSH connections can carry a tunnel.
  const jumpHosts = connections
    .filter((connection) => connection.protocol === "sftp" && connection.id !== draft.savedId)
    .sort((first, second) => first.name.localeCompare(second.name));
  const jumpHostMissing =
    draft.jumpConnectionId !== "" &&
    !jumpHosts.some((connection) => connection.id === draft.jumpConnectionId);

  const update = <K extends keyof ConnectDraft>(key: K, value: ConnectDraft[K]) =>
    setDraft((current) => withField(current, key, value));

  const select = (next: ConnectDraft) => {
    setDraft(next);
    setError(null);
    setNeedsPassphrase(false);
    setConfirmingDelete(false);
  };

  const changeProtocol = (protocol: Protocol) => {
    setDraft((current) => withProtocol(current, protocol));
    setError(null);
    setNeedsPassphrase(false);
  };

  const sftp = draft.protocol === "sftp";
  const cloud = isCloud(draft.protocol);
  const ftp = isFtp(draft.protocol);
  const authChoices = authChoicesFor(draft.protocol);
  const ready = canConnect(draft);
  const canSave = cloud ? ready : draft.host.trim() !== "";
  const account =
    draft.signedIn?.account ?? (draft.hasSavedSecret && draft.username ? draft.username : null);
  const secretLabel = draft.authChoice === "publicKey" ? "passphrase" : "password";
  const namePlaceholder = cloud
    ? `${protocolInfo(draft.protocol).label}${account ? ` (${account})` : ""}`
    : draft.host.trim() || "Shown in saved connections";
  const folderPlaceholder = cloud
    ? "Top folder of the drive"
    : ftp
      ? "Login folder"
      : "Home folder";

  const browseForKey = async () => {
    // No extension filter: OpenSSH keys such as `id_ed25519` have none.
    const selected = await openFileDialog({
      title: "Select private key",
      multiple: false,
      directory: false,
    });
    if (typeof selected === "string") update("keyPath", selected);
  };

  /** Stores the draft as a saved connection; returns it with its id. */
  const store = async (): Promise<ConnectDraft> => {
    const secret = draftSecret(draft);
    const grant = draft.signedIn?.grantId ?? null;
    const saved = await saveConnection(savedFromDraft(draft), secret || null, grant);
    const stored = {
      ...draft,
      savedId: saved.id,
      name: saved.name,
      username: saved.username,
      hasSavedSecret: saved.saveSecret && (draft.hasSavedSecret || secret !== "" || grant !== null),
    };
    setDraft(stored);
    return stored;
  };

  const saveOnly = async () => {
    if (!canSave || busy) return;
    setBusy("saving");
    setError(null);
    try {
      await store();
    } catch (caught) {
      setError(toAppError(caught).message);
    }
    setBusy(null);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!ready || busy) return;
    setBusy("connecting");
    setError(null);
    let connectDraft = draft;
    if (draft.saveConnection) {
      try {
        connectDraft = await store();
      } catch (caught) {
        setError(toAppError(caught).message);
        setBusy(null);
        return;
      }
    }
    const result = await connectInTab(profileFromDraft(connectDraft), targetTabId);
    setBusy(null);
    if (!("error" in result)) {
      onClose();
      return;
    }
    setError(result.error.message);
    if (result.error.kind === "passphraseRequired") {
      setNeedsPassphrase(true);
      requestAnimationFrame(() => passphraseRef.current?.select());
    }
  };

  const deleteSelected = async () => {
    if (!draft.savedId) return;
    if (!confirmingDelete) {
      setConfirmingDelete(true);
      return;
    }
    try {
      await removeConnection(draft.savedId);
      select(EMPTY_DRAFT);
    } catch (caught) {
      setError(toAppError(caught).message);
    }
  };

  return (
    <Dialog title="Connect" onClose={onClose} width={800}>
      <div className="site-manager">
        <aside className="site-list">
          <div className="site-list-header">
            <span>Saved connections</span>
            <button
              type="button"
              className="icon-button"
              title="New connection"
              aria-label="New connection"
              onClick={() => select(EMPTY_DRAFT)}
            >
              <Plus size={15} />
            </button>
          </div>
          {connections.length === 0 ? (
            <p className="site-list-empty">Connections you save appear here.</p>
          ) : (
            <ul>
              {byRecentUse(connections).map((connection) => (
                <li key={connection.id}>
                  <button
                    type="button"
                    className={`site-item ${draft.savedId === connection.id ? "is-selected" : ""}`}
                    onClick={() => select(draftFromSaved(connection))}
                    onDoubleClick={() => select(draftFromSaved(connection))}
                  >
                    <ConnectionIcon protocol={connection.protocol} size={14} />
                    <span className="site-item-text">
                      <span className="site-item-name">{connection.name}</span>
                      <span className="site-item-detail">{connectionDetail(connection)}</span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>

        <form className="form site-form" onSubmit={submit}>
          <div className="form-row">
            <label className="field protocol">
              <span>Protocol</span>
              <select
                value={draft.protocol}
                onChange={(event) => changeProtocol(event.target.value as Protocol)}
              >
                {PROTOCOLS.map((protocol) => (
                  <option key={protocol.value} value={protocol.value}>
                    {protocol.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="field grow">
              <span>Name</span>
              <input
                value={draft.name}
                placeholder={namePlaceholder}
                spellCheck={false}
                onChange={(event) => update("name", event.target.value)}
              />
            </label>
          </div>

          {cloud ? (
            <div className="field">
              <span>Account</span>
              <CloudSignIn
                key={draft.protocol}
                protocol={draft.protocol}
                account={account}
                onSignedIn={(signedIn) =>
                  setDraft((current) => ({ ...current, signedIn, username: signedIn.account }))
                }
                onError={setError}
              />
            </div>
          ) : (
            <>
              <div className="form-row">
                <label className="field grow">
                  <span>Host</span>
                  <input
                    data-autofocus
                    value={draft.host}
                    placeholder="example.com"
                    spellCheck={false}
                    autoCapitalize="off"
                    onChange={(event) => update("host", event.target.value)}
                  />
                </label>
                <label className="field port">
                  <span>Port</span>
                  <input
                    value={draft.port}
                    inputMode="numeric"
                    onChange={(event) => update("port", event.target.value.replace(/\D/g, ""))}
                  />
                </label>
              </div>

              <label className="field">
                <span>Username</span>
                <input
                  value={draft.username}
                  placeholder={ftp ? "anonymous" : ""}
                  spellCheck={false}
                  autoCapitalize="off"
                  onChange={(event) => update("username", event.target.value)}
                />
              </label>
            </>
          )}

          {authChoices.length > 1 && (
            <div className="field">
              <span>Authentication</span>
              <div className="segmented" role="radiogroup">
                {authChoices.map((choice) => (
                  <button
                    key={choice}
                    type="button"
                    role="radio"
                    aria-checked={draft.authChoice === choice}
                    className={draft.authChoice === choice ? "is-selected" : ""}
                    onClick={() => update("authChoice", choice)}
                  >
                    {AUTH_LABELS[choice]}
                  </button>
                ))}
              </div>
            </div>
          )}

          {draft.authChoice === "password" && (
            <label className="field">
              <span>Password</span>
              <input
                type="password"
                value={draft.password}
                placeholder={draft.hasSavedSecret ? "Saved in the system keychain" : ""}
                onChange={(event) => update("password", event.target.value)}
              />
            </label>
          )}

          {draft.authChoice === "publicKey" && (
            <>
              <div className="field">
                <span>Private key file</span>
                <div className="input-with-button">
                  <input
                    value={draft.keyPath}
                    placeholder="~/.ssh/id_ed25519 or C:\keys\server.ppk"
                    spellCheck={false}
                    onChange={(event) => update("keyPath", event.target.value)}
                  />
                  <button type="button" className="button" onClick={browseForKey}>
                    <FolderOpen size={14} /> Browse
                  </button>
                </div>
                <small className="field-hint">
                  OpenSSH, PEM, PKCS#8 and PuTTY .ppk (v2 and v3) keys are supported.
                </small>
              </div>
              <label className="field">
                <span>Passphrase {needsPassphrase ? "" : "(if the key has one)"}</span>
                <input
                  ref={passphraseRef}
                  type="password"
                  value={draft.passphrase}
                  placeholder={draft.hasSavedSecret ? "Saved in the system keychain" : ""}
                  className={needsPassphrase ? "has-error" : ""}
                  onChange={(event) => update("passphrase", event.target.value)}
                />
              </label>
            </>
          )}

          {draft.authChoice === "agent" && (
            <p className="field-hint">
              Uses keys loaded in your SSH agent: <code>SSH_AUTH_SOCK</code> on Linux and macOS, the
              OpenSSH Authentication Agent service or Pageant on Windows.
            </p>
          )}

          <label className="field">
            <span>Remote folder (optional)</span>
            <input
              value={draft.initialPath}
              placeholder={folderPlaceholder}
              spellCheck={false}
              onChange={(event) => update("initialPath", event.target.value)}
            />
          </label>

          {sftp && (
            <label className="field">
              <span>Jump host</span>
              <select
                value={draft.jumpConnectionId}
                disabled={jumpHosts.length === 0 && !jumpHostMissing}
                onChange={(event) => update("jumpConnectionId", event.target.value)}
              >
                <option value="">None, connect straight to the server</option>
                {jumpHostMissing && (
                  <option value={draft.jumpConnectionId}>A deleted connection</option>
                )}
                {jumpHosts.map((connection) => (
                  <option key={connection.id} value={connection.id}>
                    {connection.name}
                  </option>
                ))}
              </select>
              <small className="field-hint">
                Logs in to this saved connection first and reaches the server through it, like
                ProxyJump in OpenSSH.
              </small>
            </label>
          )}

          <div className="checks">
            <label className="check">
              <input
                type="checkbox"
                checked={draft.saveConnection}
                onChange={(event) => update("saveConnection", event.target.checked)}
              />
              {cloud ? "Save this connection and its sign-in" : "Save this connection"}
            </label>
            {!cloud && (
              <label className={`check ${draft.authChoice === "agent" ? "is-disabled" : ""}`}>
                <input
                  type="checkbox"
                  checked={draft.authChoice !== "agent" && draft.saveSecret}
                  disabled={draft.authChoice === "agent"}
                  onChange={(event) => update("saveSecret", event.target.checked)}
                />
                Remember the {secretLabel} in the system keychain
              </label>
            )}
            {ftp && (
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.ftpActive}
                  onChange={(event) => update("ftpActive", event.target.checked)}
                />
                Active mode: the server opens data connections to this computer
              </label>
            )}
            {sftp && proxyEnabled && !draft.jumpConnectionId && (
              <label className="check">
                <input
                  type="checkbox"
                  checked={draft.bypassProxy}
                  onChange={(event) => update("bypassProxy", event.target.checked)}
                />
                Connect without the proxy
              </label>
            )}
          </div>

          {error && <p className="form-error">{error}</p>}

          <div className="dialog-actions">
            {draft.savedId && (
              <button
                type="button"
                className={`button ${confirmingDelete ? "button-danger" : ""} push-left`}
                onClick={() => void deleteSelected()}
              >
                {confirmingDelete ? "Delete for good" : "Delete"}
              </button>
            )}
            <button type="button" className="button" onClick={onClose}>
              Cancel
            </button>
            <button
              type="button"
              className="button"
              disabled={!canSave || busy !== null}
              onClick={() => void saveOnly()}
            >
              {busy === "saving" ? "Saving..." : "Save"}
            </button>
            <button
              type="submit"
              className="button button-primary"
              disabled={!ready || busy !== null}
            >
              {busy === "connecting" ? "Connecting..." : "Connect"}
            </button>
          </div>
        </form>
      </div>
    </Dialog>
  );
}
