import { useLatest } from "../hooks/useLatest";
import { transfers } from "../lib/ipc";
import { closeRemoteTab, familyOf } from "../state/tabActions";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";
import { DrivesGoing } from "./DrivesGoing";

interface CloseTabDialogProps {
  tabId: string;
  sessionId: string;
  label: string;
  pendingTransfers: number;
  /** How many of them are for the server's unlocked vaults. */
  vaultTransfers: number;
  /** Where the server and its vaults are mounted as drives. */
  drives: string[];
  onClose: () => void;
}

export function CloseTabDialog({
  tabId,
  sessionId,
  label,
  pendingTransfers,
  vaultTransfers,
  drives,
  onClose,
}: CloseTabDialogProps) {
  const t = useT();
  const latestOnClose = useLatest(onClose);
  const close = async (pauseTransfers: boolean) => {
    const [serverId, ...vaultIds] = familyOf(sessionId);
    // Closing the tab locks the server's vaults, and nothing is copied into a locked vault.
    const paused = pauseTransfers ? [serverId, ...vaultIds] : vaultTransfers > 0 ? vaultIds : [];
    await Promise.all(paused.map((id) => transfers.pauseSession(id).catch(() => 0)));
    closeRemoteTab(tabId, sessionId);
    latestOnClose.current();
  };

  return (
    <Dialog title={t("workspace.close", { label })} onClose={onClose}>
      <div className="confirm-body">
        {pendingTransfers > 0 && (
          <p>{t("workspace.transfersForThisServerStillQueued", { count: pendingTransfers })}</p>
        )}
        {vaultTransfers > 0 && <p>{t("workspace.transfersIntoOrOutOf")}</p>}
        <DrivesGoing drives={drives} when="tabCloses" />
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose}>
          {t("common.cancel")}
        </button>
        {pendingTransfers > 0 ? (
          <>
            <button type="button" className="button" onClick={() => void close(true)}>
              {t("workspace.pauseTransfersAndClose")}
            </button>
            <button
              type="button"
              data-autofocus
              className="button button-primary"
              onClick={() => void close(false)}
            >
              {t("workspace.keepTransferring")}
            </button>
          </>
        ) : (
          <button
            type="button"
            data-autofocus
            className="button button-primary"
            onClick={() => void close(false)}
          >
            {t("workspace.unmountAndClose")}
          </button>
        )}
      </div>
    </Dialog>
  );
}
