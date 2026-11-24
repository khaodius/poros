import { useLatest } from "../hooks/useLatest";
import { transfers } from "../lib/ipc";
import { lockVault } from "../state/vaultActions";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";

interface LockVaultDialogProps {
  /** The vault's session. */
  sessionId: string;
  name: string;
  pendingTransfers: number;
  onClose: () => void;
}

/** Asks before locking a vault that transfers in the queue still copy into or out of. */
export function LockVaultDialog({
  sessionId,
  name,
  pendingTransfers,
  onClose,
}: LockVaultDialogProps) {
  const t = useT();
  const latestOnClose = useLatest(onClose);
  const lock = async () => {
    await transfers.pauseSession(sessionId).catch(() => 0);
    await lockVault(sessionId);
    latestOnClose.current();
  };

  return (
    <Dialog title={t("vault.lock", { name })} onClose={onClose}>
      <div className="confirm-body">
        <p>{t("vault.transfersForThisVaultStillQueued", { count: pendingTransfers })}</p>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          onClick={() => void lock()}
        >
          {t("vault.pauseTransfersAndLock")}
        </button>
      </div>
    </Dialog>
  );
}
