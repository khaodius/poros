import { useEffect, useMemo, useState } from "react";
import { formatDate, formatSize } from "../lib/format";
import { transfers } from "../lib/ipc";
import { jobsIn } from "../lib/transfers";
import type { ExistsAction } from "../lib/types";
import { useSettingsStore } from "../state/settingsStore";
import { useTransferStore } from "../state/transferStore";
import { Dialog } from "./Dialog";

const ACTIONS: { action: ExistsAction; label: string }[] = [
  { action: "overwrite", label: "Overwrite" },
  { action: "overwriteIfNewer", label: "Overwrite if newer" },
  { action: "resume", label: "Resume" },
  { action: "rename", label: "Keep both" },
];

/** Hook for whether this window has the user's attention, so only one window asks. */
function useWindowFocused(): boolean {
  const [focused, setFocused] = useState(() => document.hasFocus());
  useEffect(() => {
    const onFocus = () => setFocused(true);
    const onBlur = () => setFocused(false);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);
  return focused;
}

export function ConflictDialog() {
  const snapshot = useTransferStore((state) => state.snapshot);
  const conflicts = useMemo(() => jobsIn(snapshot, ["conflict"]), [snapshot]);
  const focused = useWindowFocused();
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const [applyToAll, setApplyToAll] = useState(false);
  const [answeredId, setAnsweredId] = useState<number | null>(null);
  const job = conflicts.find((candidate) => candidate.id !== answeredId);
  if (!job?.conflict || !focused) return null;
  const info = job.conflict;
  const others = conflicts.length - 1;

  const answer = (action: ExistsAction) => {
    setAnsweredId(job.id);
    void transfers.resolve(job.id, action, applyToAll);
  };

  return (
    <Dialog title="File already exists" onClose={() => answer("skip")} width={520}>
      <div className="confirm-body">
        <p>
          <strong>{job.name}</strong> already exists in {job.targetDirectory}.
        </p>
        <table className="conflict-table">
          <thead>
            <tr>
              <th />
              <th>Size</th>
              <th>Modified</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th>{job.direction === "upload" ? "Local file" : "Server file"}</th>
              <td>{formatSize(info.sourceSize)}</td>
              <td>{formatDate(info.sourceModified, dateFormat)}</td>
            </tr>
            <tr>
              <th>Existing file</th>
              <td>{formatSize(info.targetSize)}</td>
              <td>{formatDate(info.targetModified, dateFormat)}</td>
            </tr>
          </tbody>
        </table>
        <label className="check">
          <input
            type="checkbox"
            checked={applyToAll}
            onChange={(event) => setApplyToAll(event.target.checked)}
          />
          {others > 0
            ? `Do the same for the ${others} other ${others === 1 ? "conflict" : "conflicts"} and any that follow`
            : "Do the same for any further conflicts in these transfers"}
        </label>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={() => answer("skip")}>
          Skip
        </button>
        {ACTIONS.map(({ action, label }) => (
          <button
            key={action}
            type="button"
            data-autofocus={action === "overwrite" ? true : undefined}
            className={`button ${action === "overwrite" ? "button-primary" : ""}`}
            disabled={action === "resume" && info.targetSize >= info.sourceSize}
            onClick={() => answer(action)}
          >
            {label}
          </button>
        ))}
      </div>
    </Dialog>
  );
}
