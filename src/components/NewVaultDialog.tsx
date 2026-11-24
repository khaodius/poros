import { useState, type FormEvent } from "react";
import { useLatest } from "../hooks/useLatest";
import { MIN_VAULT_PASSWORD_LENGTH } from "../lib/vaults";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

interface NewVaultDialogProps {
  /** The folder on the server the vault's own folder is made in. */
  folder: string;
  /** Rejects with the backend's error, such as a name that is taken. */
  onCreate: (name: string, password: string) => Promise<void>;
  onClose: () => void;
}

/** Asks for the name and the password of a new Cryptomator vault. */
export function NewVaultDialog({ folder, onCreate, onClose }: NewVaultDialogProps) {
  const t = useT();
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [repeated, setRepeated] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const latestOnClose = useLatest(onClose);

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
      latestOnClose.current();
    } catch (caught) {
      setError((caught as { message?: string }).message ?? String(caught));
      setBusy(false);
    }
  };

  return (
    <Dialog title={t("vault.newVault")} onClose={onClose}>
      <form onSubmit={submit} className="form">
        <label className="field">
          <span>{t("common.name")}</span>
          <input
            data-autofocus
            value={name}
            spellCheck={false}
            onChange={(event) => setName(event.target.value)}
          />
          <small className="field-hint">{t("vault.aNewFolderInHolds", { folder })}</small>
        </label>
        <label className="field">
          <span>{t("vault.password")}</span>
          <input
            type="password"
            className={password !== "" && tooShort ? "has-error" : ""}
            value={password}
            autoComplete="new-password"
            onChange={(event) => setPassword(event.target.value)}
          />
          <small className={`field-hint ${password !== "" && tooShort ? "is-error" : ""}`}>
            {t("vault.atLeastCharacters", { value: MIN_VAULT_PASSWORD_LENGTH })}
          </small>
        </label>
        <label className="field">
          <span>{t("vault.repeatPassword")}</span>
          <input
            type="password"
            className={repeated !== "" && differs ? "has-error" : ""}
            value={repeated}
            autoComplete="new-password"
            onChange={(event) => setRepeated(event.target.value)}
          />
          {repeated !== "" && differs && (
            <small className="field-hint is-error">{t("vault.thePasswordsDoNotMatch")}</small>
          )}
        </label>
        <p className="vault-warning">{t("vault.thisPasswordCannotBeRecovered")}</p>
        <p className="field-hint">{t("vault.theVaultAlsoOpensIn")}</p>
        {error && <p className="form-error">{error}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="button button-primary" disabled={busy || !complete}>
            {busy ? t("vault.creating") : t("vault.createVault")}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
