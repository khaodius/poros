import { useRef, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import { profileFromDraft, type AuthChoice, type ConnectDraft } from "../lib/connectDraft";
import { connectWithPrompts } from "../state/connectFlow";
import { Dialog } from "./Dialog";

const AUTH_CHOICES: { value: AuthChoice; label: string }[] = [
  { value: "password", label: "Password" },
  { value: "publicKey", label: "Key file" },
  { value: "agent", label: "SSH agent" },
];

interface ConnectDialogProps {
  initialDraft: ConnectDraft;
  onClose: () => void;
}

export function ConnectDialog({ initialDraft, onClose }: ConnectDialogProps) {
  const [draft, setDraft] = useState(initialDraft);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [needsPassphrase, setNeedsPassphrase] = useState(false);
  const passphraseRef = useRef<HTMLInputElement>(null);

  const update = <K extends keyof ConnectDraft>(key: K, value: ConnectDraft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  const canSubmit =
    draft.host.trim() !== "" &&
    draft.username.trim() !== "" &&
    (draft.authChoice !== "publicKey" || draft.keyPath.trim() !== "");

  const browseForKey = async () => {
    // No extension filter: OpenSSH keys such as `id_ed25519` have none.
    const selected = await openFileDialog({
      title: "Select private key",
      multiple: false,
      directory: false,
    });
    if (typeof selected === "string") update("keyPath", selected);
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!canSubmit || busy) return;
    setBusy(true);
    setError(null);
    const failure = await connectWithPrompts(profileFromDraft(draft));
    setBusy(false);
    if (!failure) {
      onClose();
      return;
    }
    setError(failure.message);
    if (failure.kind === "passphraseRequired") {
      setNeedsPassphrase(true);
      requestAnimationFrame(() => passphraseRef.current?.select());
    }
  };

  return (
    <Dialog title="Connect to server" onClose={onClose} width={500}>
      <form className="form" onSubmit={submit}>
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

        {error && <p className="form-error">{error}</p>}

        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="button button-primary" disabled={!canSubmit || busy}>
            {busy ? "Connecting..." : "Connect"}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
