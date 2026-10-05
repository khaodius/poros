import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";

interface DialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
  tone?: "default" | "danger";
}

export function Dialog({ title, onClose, children, footer, width = 440, tone }: DialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    dialog.showModal();
    const autofocus = dialog.querySelector<HTMLElement>("[data-autofocus]");
    autofocus?.focus();
    if (autofocus instanceof HTMLInputElement) autofocus.select();
    return () => dialog.close();
  }, []);

  return (
    <dialog
      ref={dialogRef}
      className={`dialog ${tone === "danger" ? "dialog-danger" : ""}`}
      style={{ width }}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      <header className="dialog-header">
        <h2>{title}</h2>
        <button type="button" className="icon-button" aria-label="Close" onClick={onClose}>
          <X size={16} />
        </button>
      </header>
      <div className="dialog-body">{children}</div>
      {footer && <footer className="dialog-footer">{footer}</footer>}
    </dialog>
  );
}
