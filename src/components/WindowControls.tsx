import { Copy, Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Minimize, maximize and close, for windows drawn without the system title bar. */
export function WindowControls({ maximized }: { maximized: boolean }) {
  const appWindow = getCurrentWindow();
  return (
    <div className="window-controls">
      <button
        type="button"
        className="window-control"
        title="Minimize"
        aria-label="Minimize"
        onClick={() => void appWindow.minimize()}
      >
        <Minus size={16} strokeWidth={1.5} />
      </button>
      <button
        type="button"
        className="window-control"
        title={maximized ? "Restore" : "Maximize"}
        aria-label={maximized ? "Restore" : "Maximize"}
        onClick={() => void appWindow.toggleMaximize()}
      >
        {maximized ? (
          <Copy size={13} strokeWidth={1.5} className="window-control-restore" />
        ) : (
          <Square size={13} strokeWidth={1.5} />
        )}
      </button>
      <button
        type="button"
        className="window-control window-control-close"
        title="Close"
        aria-label="Close"
        onClick={() => void appWindow.close()}
      >
        <X size={17} strokeWidth={1.5} />
      </button>
    </div>
  );
}
