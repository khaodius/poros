import { useState, type FormEvent } from "react";
import { MIN_VAULT_PASSWORD_LENGTH } from "../lib/vaults";
import { Dialog } from "./Dialog";

interface NewVaultDialogProps {
  /** The folder on the server the vault's own folder is made in. */
  folder: string;
  /** Rejects with the backend's error, such as a name that is taken. */
  onCreate: (name: string, password: string) => Promise<void>;
  onClose: () => void;
}

/** Asks for the name and the password of a new Cryptomator vault. */
export function NewVaultDialog({ folder, onCreate, onClose }: NewVaultDialogProps) {
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [repeated, setRepeated] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const trimmedName = name.trim();
  const tooShort = Array.from(password).length < MIN_VAULT_PASSWORD_LENGTH;
  const differs = repeated !== password;
  const complete = trimmedName !== "" && !tooShort && !differs;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!complete || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onCreate(trimmedName, password);
      onClose();
    } catch (caught) {
      setError((caught as { message?: string }).message ?? String(caught));
      setBusy(false);
    }
  };

  return (
    <Dialog title="New vault" onClose={onClose}>
      <form onSubmit={submit} className="form">
        <label className="field">
          <span>Name</span>
          <input
            data-autofocus
            value={name}
            spellCheck={false}
            onChange={(event) => setName(event.target.value)}
          />
          <small className="field-hint">A new folder in {folder} holds the encrypted files.</small>
        </label>
        <label className="field">
          <span>Password</span>
          <input
            type="password"
            className={password !== "" && tooShort ? "has-error" : ""}
            value={password}
            autoComplete="new-password"
            onChange={(event) => setPassword(event.target.value)}
          />
          <small className={`field-hint ${password !== "" && tooShort ? "is-error" : ""}`}>
            At least {MIN_VAULT_PASSWORD_LENGTH} characters.
          </small>
        </label>
        <label className="field">
          <span>Repeat password</span>
          <input
            type="password"
            className={repeated !== "" && differs ? "has-error" : ""}
            value={repeated}
            autoComplete="new-password"
            onChange={(event) => setRepeated(event.target.value)}
          />
          {repeated !== "" && differs && (
            <small className="field-hint is-error">The passwords do not match.</small>
          )}
        </label>
        <p className="vault-warning">
          This password cannot be recovered. Without it, nobody can read the files in the vault.
        </p>
        <p className="field-hint">The vault also opens in the Cryptomator apps.</p>
        {error && <p className="form-error">{error}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="button button-primary" disabled={busy || !complete}>
            {busy ? "Creating..." : "Create vault"}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
