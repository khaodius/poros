import { AlertCircle, Info, X } from "lucide-react";
import { useToastStore } from "../state/toastStore";

export function Toasts() {
  const toasts = useToastStore((state) => state.toasts);
  const dismiss = useToastStore((state) => state.dismiss);
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast toast-${toast.tone}`}>
          {toast.tone === "error" ? <AlertCircle size={16} /> : <Info size={16} />}
          <span className="toast-message selectable">{toast.message}</span>
          <button
            type="button"
            className="icon-button"
            aria-label="Dismiss"
            onClick={() => dismiss(toast.id)}
          >
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
