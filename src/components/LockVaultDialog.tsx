import { useLatest } from "../hooks/useLatest";
import { pluralize } from "../lib/format";
import { transfers } from "../lib/ipc";
import { lockVault } from "../state/vaultActions";
import { Dialog } from "./Dialog";
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
  const latestOnClose = useLatest(onClose);
  const lock = async () => {
    if (pendingTransfers > 0) await transfers.pauseSession(sessionId).catch(() => 0);
    await lockVault(sessionId);
    latestOnClose.current();
  };

  return (
    <Dialog title={`Lock ${name}?`} onClose={onClose}>
      <div className="confirm-body">
        {pendingTransfers > 0 && (
          <p>
            {pluralize(pendingTransfers, "transfer")} for this vault{" "}
            {pendingTransfers === 1 ? "is" : "are"} still in the queue. Nothing can be copied into
            or out of a locked vault, so {pendingTransfers === 1 ? "it waits" : "they wait"} paused
            until you unlock the vault again and resume {pendingTransfers === 1 ? "it" : "them"}.
          </p>
        )}
        <DrivesGoing drives={drives} when="the vault locks" />
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose}>
          Cancel
        </button>
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          onClick={() => void lock()}
        >
          {pendingTransfers > 0 ? "Pause transfers and lock" : "Unmount and lock"}
        </button>
      </div>
    </Dialog>
  );
}
