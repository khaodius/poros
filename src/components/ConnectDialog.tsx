import { useRef, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Plus, Server } from "lucide-react";
import {
  EMPTY_DRAFT,
  draftFromSaved,
  draftSecret,
  profileFromDraft,
  savedFromDraft,
  type AuthChoice,
  type ConnectDraft,
} from "../lib/connectDraft";
import { toAppError } from "../lib/ipc";
import { connectInTab } from "../state/connectActions";
import { byRecentUse, useSavedConnections } from "../state/savedConnectionsStore";
import { Dialog } from "./Dialog";

const AUTH_CHOICES: { value: AuthChoice; label: string }[] = [
  { value: "password", label: "Password" },
  { value: "publicKey", label: "Key file" },
  { value: "agent", label: "SSH agent" },
];

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

  const update = <K extends keyof ConnectDraft>(key: K, value: ConnectDraft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  const select = (next: ConnectDraft) => {
    setDraft(next);
    setError(null);
    setNeedsPassphrase(false);
    setConfirmingDelete(false);
  };

  const hasHost = draft.host.trim() !== "";
  const canConnect =
    hasHost &&
    draft.username.trim() !== "" &&
    (draft.authChoice !== "publicKey" || draft.keyPath.trim() !== "");
  const secretLabel = draft.authChoice === "publicKey" ? "passphrase" : "password";

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
    const saved = await saveConnection(savedFromDraft(draft), secret || null);
    const stored = {
      ...draft,
      savedId: saved.id,
      name: saved.name,
      hasSavedSecret: saved.saveSecret && (draft.hasSavedSecret || secret !== ""),
    };
    setDraft(stored);
    return stored;
  };

  const saveOnly = async () => {
    if (!hasHost || busy) return;
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
    if (!canConnect || busy) return;
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
    <Dialog title="Connect to server" onClose={onClose} width={800}>
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
                    <Server size={14} />
                    <span className="site-item-text">
                      <span className="site-item-name">{connection.name}</span>
                      <span className="site-item-detail">
                        {connection.username}@{connection.host}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </aside>

        <form className="form site-form" onSubmit={submit}>
          <label className="field">
            <span>Name</span>
            <input
              value={draft.name}
              placeholder={draft.host.trim() || "Shown in saved connections"}
              spellCheck={false}
              onChange={(event) => update("name", event.target.value)}
            />
          </label>
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
              spellCheck={false}
              autoCapitalize="off"
              onChange={(event) => update("username", event.target.value)}
            />
          </label>

          <div className="field">
            <span>Authentication</span>
            <div className="segmented" role="radiogroup">
              {AUTH_CHOICES.map((choice) => (
                <button
                  key={choice.value}
                  type="button"
                  role="radio"
                  aria-checked={draft.authChoice === choice.value}
                  className={draft.authChoice === choice.value ? "is-selected" : ""}
                  onClick={() => update("authChoice", choice.value)}
                >
                  {choice.label}
                </button>
              ))}
            </div>
          </div>

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
              placeholder="Home folder"
              spellCheck={false}
              onChange={(event) => update("initialPath", event.target.value)}
            />
          </label>

          <div className="checks">
            <label className="check">
              <input
                type="checkbox"
                checked={draft.saveConnection}
                onChange={(event) => update("saveConnection", event.target.checked)}
              />
              Save this connection
            </label>
            <label className={`check ${draft.authChoice === "agent" ? "is-disabled" : ""}`}>
              <input
                type="checkbox"
                checked={draft.authChoice !== "agent" && draft.saveSecret}
                disabled={draft.authChoice === "agent"}
                onChange={(event) => update("saveSecret", event.target.checked)}
              />
              Remember the {secretLabel} in the system keychain
            </label>
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
              disabled={!hasHost || busy !== null}
              onClick={() => void saveOnly()}
            >
              {busy === "saving" ? "Saving..." : "Save"}
            </button>
            <button
              type="submit"
              className="button button-primary"
              disabled={!canConnect || busy !== null}
            >
              {busy === "connecting" ? "Connecting..." : "Connect"}
            </button>
          </div>
        </form>
      </div>
    </Dialog>
  );
}
