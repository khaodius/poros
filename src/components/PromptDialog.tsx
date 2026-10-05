import { useEffect, useRef, useState, type FormEvent } from "react";
import { Dialog } from "./Dialog";

interface PromptDialogProps {
  title: string;
  label: string;
  initialValue?: string;
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
  selectLength,
  confirmLabel,
  onSubmit,
  onClose,
}: PromptDialogProps) {
  const [value, setValue] = useState(initialValue);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

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
      onClose();
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
            spellCheck={false}
            onChange={(event) => setValue(event.target.value)}
          />
        </label>
        {error && <p className="form-error">{error}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="button button-primary" disabled={busy || !value.trim()}>
            {confirmLabel}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
