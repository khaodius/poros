import { useEffect, useRef, useState, type FormEvent } from "react";
import { useLatest } from "../hooks/useLatest";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

interface PromptDialogProps {
  title: string;
  label: string;
  initialValue?: string;
  /** Faint example text shown while the box is empty. */
  placeholder?: string;
  /** Characters preselected from the start, e.g. a file name without its extension. */
  selectLength?: number;
  confirmLabel: string;
  onSubmit: (value: string) => Promise<void>;
  onClose: () => void;
}

export function PromptDialog({
  title,
  label,
  initialValue = "",
  placeholder,
  selectLength,
  confirmLabel,
  onSubmit,
  onClose,
}: PromptDialogProps) {
  const t = useT();
  const [value, setValue] = useState(initialValue);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const latestOnClose = useLatest(onClose);

  useEffect(() => {
    const input = inputRef.current;
    if (input && selectLength !== undefined) {
      requestAnimationFrame(() => input.setSelectionRange(0, selectLength));
    }
  }, [selectLength]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const trimmed = value.trim();
    if (!trimmed) return;
    setBusy(true);
    setError(null);
    try {
      await onSubmit(trimmed);
      latestOnClose.current();
    } catch (caught) {
      setError((caught as { message?: string }).message ?? String(caught));
      setBusy(false);
    }
  };

  return (
    <Dialog title={title} onClose={onClose}>
      <form onSubmit={submit} className="form">
        <label className="field">
          <span>{label}</span>
          <input
            ref={inputRef}
            data-autofocus
            value={value}
            placeholder={placeholder}
            spellCheck={false}
            onChange={(event) => setValue(event.target.value)}
          />
        </label>
        {error && <p className="form-error">{error}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="button button-primary" disabled={busy || !value.trim()}>
            {confirmLabel}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
