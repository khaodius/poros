import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { LoaderCircle, LockKeyhole, LockOpen } from "lucide-react";
import { isTextEntryTarget } from "../lib/focus";
import { toAppError } from "../lib/ipc";

interface VaultUnlockCardProps {
  /** The name of the vault's folder. */
  name: string;
  /** The pane is the one the user works in, so the password field takes the keyboard. */
  active: boolean;
  /** Rejects with the backend's error, such as a wrong password. */
  onUnlock: (password: string) => Promise<void>;
  onShowEncrypted: () => void;
  /** Keys the card has no use for, so the pane's shortcuts for going elsewhere still work. */
  onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
}

/**
 * Stands where a pane lists its files while the folder on show is a locked Cryptomator vault:
 * asks for the vault's password, to open the vault in that same pane.
 */
export function VaultUnlockCard({
  name,
  active,
  onUnlock,
  onShowEncrypted,
  onKeyDown,
}: VaultUnlockCardProps) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!active) return;
    // Once the click that brought the pane forward has moved focus, which it does after this
    // runs. A dialog or menu that is open keeps the keyboard, and so does a field being typed in.
    const timer = window.setTimeout(() => {
      const taken =
        isTextEntryTarget(document.activeElement) ||
        document.querySelector("dialog[open], .context-menu") !== null;
      if (!taken) inputRef.current?.focus({ preventScroll: true });
    });
    return () => window.clearTimeout(timer);
  }, [active]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (busy || password === "") return;
    setBusy(true);
    setError(null);
    try {
      await onUnlock(password);
    } catch (caught) {
      const failure = toAppError(caught);
      setError(failure.kind === "wrongPassword" ? "Wrong password" : failure.message);
      inputRef.current?.focus({ preventScroll: true });
      inputRef.current?.select();
    }
    setBusy(false);
  };

  return (
    <div className="vault-locked" onKeyDown={onKeyDown}>
      <form className="vault-card" onSubmit={submit}>
        <span className="vault-card-icon">
          <LockKeyhole size={22} />
        </span>
        <p className="pane-empty-title">Encrypted vault</p>
        <p className="vault-card-detail">
          <strong>{name}</strong> is a Cryptomator vault. Enter its password to open it here.
        </p>
        <div className="vault-card-unlock">
          <input
            ref={inputRef}
            type="password"
            className={`vault-password ${error ? "has-error" : ""}`}
            value={password}
            placeholder="Password"
            aria-label="Vault password"
            autoComplete="off"
            spellCheck={false}
            // Read only rather than disabled while unlocking, so it keeps the keyboard.
            readOnly={busy}
            onChange={(event) => setPassword(event.target.value)}
          />
          <button type="submit" className="button button-primary" disabled={busy || !password}>
            {busy ? <LoaderCircle size={14} className="spin" /> : <LockOpen size={14} />}
            {busy ? "Unlocking..." : "Unlock"}
          </button>
        </div>
        {error && <p className="form-error">{error}</p>}
        <button type="button" className="link-button vault-card-quiet" onClick={onShowEncrypted}>
          Show encrypted files
        </button>
      </form>
    </div>
  );
}
