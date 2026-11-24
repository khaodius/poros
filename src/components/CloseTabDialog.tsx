import { useLatest } from "../hooks/useLatest";
import { pluralize } from "../lib/format";
import { transfers } from "../lib/ipc";
import { closeRemoteTab, familyOf } from "../state/tabActions";
import { Dialog } from "./Dialog";

interface CloseTabDialogProps {
  tabId: string;
  sessionId: string;
  label: string;
  pendingTransfers: number;
  /** How many of them are for the server's unlocked vaults. */
  vaultTransfers: number;
  onClose: () => void;
}

export function CloseTabDialog({
  tabId,
  sessionId,
  label,
  pendingTransfers,
  vaultTransfers,
  onClose,
}: CloseTabDialogProps) {
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
    <Dialog title={`Close ${label}?`} onClose={onClose}>
      <div className="confirm-body">
        <p>
          {pluralize(pendingTransfers, "transfer")} for this server{" "}
          {pendingTransfers === 1 ? "is" : "are"} still in the queue. They can keep running on their
          own connections after the tab closes, or wait paused until you resume them.
        </p>
        {vaultTransfers > 0 && (
          <p>
            Transfers into or out of an unlocked vault are paused either way: closing the tab locks
            the vault. Resume them once the vault is unlocked again.
          </p>
        )}
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={onClose}>
          Cancel
        </button>
        <button type="button" className="button" onClick={() => void close(true)}>
          Pause transfers and close
        </button>
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          onClick={() => void close(false)}
        >
          Keep transferring
        </button>
      </div>
    </Dialog>
  );
}
