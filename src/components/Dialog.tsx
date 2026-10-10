import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";
import { rememberFocus } from "../lib/focus";
import { dragWindowFromBackdrop } from "../lib/windowDrag";

interface DialogProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
  tone?: "default" | "danger";
  className?: string;
}

export function Dialog({
  title,
  onClose,
  children,
  footer,
  width = 440,
  tone,
  className,
}: DialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    const returnFocus = rememberFocus();
    dialog.showModal();
    const autofocus = dialog.querySelector<HTMLElement>("[data-autofocus]");
    autofocus?.focus();
    if (autofocus instanceof HTMLInputElement) autofocus.select();
    return () => {
      // Off the page it no longer shows. Closing it then would make WebKit focus what had focus
      // as it opened, even in a tab group the user has left since.
      if (dialog.isConnected) dialog.close();
      returnFocus(dialog);
    };
  }, []);

  return (
    <dialog
      ref={dialogRef}
      className={["dialog", tone === "danger" && "dialog-danger", className]
        .filter(Boolean)
        .join(" ")}
      style={{ width }}
      onMouseDown={dragWindowFromBackdrop}
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
