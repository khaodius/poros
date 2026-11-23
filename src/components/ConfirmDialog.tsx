import { useState, type ReactNode } from "react";
import { Dialog } from "./Dialog";

interface ConfirmDialogProps {
  title: string;
  children: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  /** Shown on the confirm button, which does nothing meanwhile, while what is asked is checked. */
  waiting?: string;
  /** There is nothing to confirm. */
  disabled?: boolean;
  onConfirm: () => Promise<void>;
  onClose: () => void;
}

export function ConfirmDialog({
  title,
  children,
  confirmLabel,
  danger,
  waiting,
  disabled,
  onConfirm,
  onClose,
}: ConfirmDialogProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = async () => {
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
      onClose();
    } catch (caught) {
      setError((caught as { message?: string }).message ?? String(caught));
      setBusy(false);
    }
  };

  return (
    <Dialog title={title} onClose={onClose} tone={danger ? "danger" : "default"}>
      <div className="confirm-body">{children}</div>
      {error && <p className="form-error">{error}</p>}
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose} disabled={busy}>
          Cancel
        </button>
        <button
          type="button"
          data-autofocus
          className={`button ${danger ? "button-danger" : "button-primary"}`}
          onClick={confirm}
          disabled={busy || disabled || waiting !== undefined}
        >
          {busy ? "Working..." : (waiting ?? confirmLabel)}
        </button>
      </div>
    </Dialog>
  );
}
