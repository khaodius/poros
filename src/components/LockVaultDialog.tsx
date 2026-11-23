import { pluralize } from "../lib/format";
import { transfers } from "../lib/ipc";
import { lockVault } from "../state/vaultActions";
import { Dialog } from "./Dialog";

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
  const lock = async () => {
    await transfers.pauseSession(sessionId).catch(() => 0);
    await lockVault(sessionId);
    onClose();
  };

  return (
    <Dialog title={`Lock ${name}?`} onClose={onClose}>
      <div className="confirm-body">
        <p>
          {pluralize(pendingTransfers, "transfer")} for this vault{" "}
          {pendingTransfers === 1 ? "is" : "are"} still in the queue. Nothing can be copied into or
          out of a locked vault, so {pendingTransfers === 1 ? "it waits" : "they wait"} paused until
          you unlock the vault again and resume {pendingTransfers === 1 ? "it" : "them"}.
        </p>
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
          Pause transfers and lock
        </button>
      </div>
    </Dialog>
  );
}
