import { pluralize } from "../lib/format";
import { transfers } from "../lib/ipc";
import { closeRemoteTab } from "../state/tabActions";
import { Dialog } from "./Dialog";

interface CloseTabDialogProps {
  tabId: string;
  sessionId: string;
  label: string;
  pendingTransfers: number;
  onClose: () => void;
}

export function CloseTabDialog({
  tabId,
  sessionId,
  label,
  pendingTransfers,
  onClose,
}: CloseTabDialogProps) {
  const close = async (pauseTransfers: boolean) => {
    if (pauseTransfers) await transfers.pauseSession(sessionId).catch(() => 0);
    closeRemoteTab(tabId, sessionId);
    onClose();
  };

  return (
    <Dialog title={`Close ${label}?`} onClose={onClose}>
      <div className="confirm-body">
        <p>
          {pluralize(pendingTransfers, "transfer")} for this server{" "}
          {pendingTransfers === 1 ? "is" : "are"} still in the queue. They can keep running on their
          own connections after the tab closes, or wait paused until you resume them.
        </p>
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
