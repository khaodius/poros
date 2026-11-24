import { Copy, Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useT } from "../i18n";

/** Minimize, maximize and close, for windows drawn without the system title bar. */
export function WindowControls({ maximized }: { maximized: boolean }) {
  const t = useT();
  const appWindow = getCurrentWindow();
  return (
    <div className="window-controls">
      <button
        type="button"
        className="window-control"
        title={t("workspace.minimize")}
        aria-label={t("workspace.minimize")}
        onClick={() => void appWindow.minimize()}
      >
        <Minus size={16} strokeWidth={1.5} />
      </button>
      <button
        type="button"
        className="window-control"
        title={maximized ? t("workspace.restore") : t("workspace.maximize")}
        aria-label={maximized ? t("workspace.restore") : t("workspace.maximize")}
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
        title={t("common.close")}
        aria-label={t("common.close")}
        onClick={() => void appWindow.close()}
      >
        <X size={17} strokeWidth={1.5} />
      </button>
    </div>
  );
}
