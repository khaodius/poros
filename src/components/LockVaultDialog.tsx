import { useLatest } from "../hooks/useLatest";
import { transfers } from "../lib/ipc";
import { lockVault } from "../state/vaultActions";
import { Dialog } from "./Dialog";
import { useT } from "../i18n";
import { DrivesGoing } from "./DrivesGoing";

interface LockVaultDialogProps {
  /** The vault's session. */
  sessionId: string;
  name: string;
  pendingTransfers: number;
  /** Where the vault is mounted as a drive. */
  drives: string[];
  onClose: () => void;
}

/** Asks before locking a vault that transfers in the queue still copy into or out of, or that is
 * mounted as a drive. */
export function LockVaultDialog({
  sessionId,
  name,
  pendingTransfers,
  drives,
  onClose,
}: LockVaultDialogProps) {
  const t = useT();
  const latestOnClose = useLatest(onClose);
  const lock = async () => {
    if (pendingTransfers > 0) await transfers.pauseSession(sessionId).catch(() => 0);
    await lockVault(sessionId);
    latestOnClose.current();
  };

  return (
    <Dialog title={t("vault.lock", { name })} onClose={onClose}>
      <div className="confirm-body">
        {pendingTransfers > 0 && (
          <p>{t("vault.transfersForThisVaultStillQueued", { count: pendingTransfers })}</p>
        )}
        <DrivesGoing drives={drives} when="vaultLocks" />
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
          {pendingTransfers > 0 ? t("vault.pauseTransfersAndLock") : t("vault.unmountAndLock")}
        </button>
      </div>
    </Dialog>
  );
}
