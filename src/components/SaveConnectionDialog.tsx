import { useState, type FormEvent } from "react";
import { profileSecret, savedFromProfile } from "../lib/connectDraft";
import { toAppError } from "../lib/ipc";
import { useSavedConnections } from "../state/savedConnectionsStore";
import { useSessionStore } from "../state/sessionStore";
import { Dialog } from "./Dialog";

export function SaveConnectionDialog({
  sessionId,
  onClose,
}: {
  sessionId: string;
  onClose: () => void;
}) {
  const entry = useSessionStore((state) => state.sessions[sessionId]);
  const linkSaved = useSessionStore((state) => state.linkSaved);
  const save = useSavedConnections((state) => state.save);
  const [name, setName] = useState(entry?.info.label ?? "");
  const [saveSecret, setSaveSecret] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const profile = entry?.profile;
  const secret = profile ? profileSecret(profile) : "";
  const secretLabel = profile?.auth.type === "publicKey" ? "passphrase" : "password";

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!profile || busy) return;
    setBusy(true);
    setError(null);
    try {
      const saved = await save(
        savedFromProfile(profile, name, saveSecret && secret !== ""),
        saveSecret ? secret : null,
      );
      linkSaved(sessionId, saved.id);
      onClose();
    } catch (caught) {
      setError(toAppError(caught).message);
      setBusy(false);
    }
  };

  return (
    <Dialog title="Save connection" onClose={onClose}>
      <form className="form" onSubmit={submit}>
        <label className="field">
          <span>Name</span>
          <input
            data-autofocus
            value={name}
            spellCheck={false}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        {secret !== "" && (
          <label className="check">
            <input
              type="checkbox"
              checked={saveSecret}
              onChange={(event) => setSaveSecret(event.target.checked)}
            />
            Remember the {secretLabel} in the system keychain
          </label>
        )}
        {error && <p className="form-error">{error}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="button button-primary" disabled={busy || !profile}>
            Save
          </button>
        </div>
      </form>
    </Dialog>
  );
}
