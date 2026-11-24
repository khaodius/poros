import { useMemo, useState } from "react";
import { useLastUsedWindow } from "../hooks/useLastUsedWindow";
import { formatDate, formatSize } from "../lib/format";
import { transfers } from "../lib/ipc";
import { jobsIn } from "../lib/transfers";
import type { ExistsAction } from "../lib/types";
import { useSettingsStore } from "../state/settingsStore";
import { useTransferStore } from "../state/transferStore";
import { Dialog } from "./Dialog";
import { useT, t } from "../i18n";

const ACTIONS: { action: ExistsAction; label: string }[] = [
  {
    action: "overwrite",
    get label() {
      return t("common.overwrite");
    },
  },
  {
    action: "overwriteIfNewer",
    get label() {
      return t("dialogs.overwriteIfNewer");
    },
  },
  {
    action: "resume",
    get label() {
      return t("common.resume");
    },
  },
  {
    action: "rename",
    get label() {
      return t("dialogs.keepBoth");
    },
  },
];

export function ConflictDialog() {
  const t = useT();
  const snapshot = useTransferStore((state) => state.snapshot);
  const conflicts = useMemo(() => jobsIn(snapshot, ["conflict"]), [snapshot]);
  // One window asks.
  const asking = useLastUsedWindow();
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const [applyToAll, setApplyToAll] = useState(false);
  const [answeredId, setAnsweredId] = useState<number | null>(null);
  const job = conflicts.find((candidate) => candidate.id !== answeredId);
  if (!job?.conflict || !asking) return null;
  const info = job.conflict;
  const others = conflicts.length - 1;

  const answer = (action: ExistsAction) => {
    setAnsweredId(job.id);
    void transfers.resolve(job.id, action, applyToAll);
  };

  return (
    <Dialog title={t("dialogs.fileAlreadyExists")} onClose={() => answer("skip")} width={520}>
      <div className="confirm-body">
        <p>
          {t.rich("dialogs.alreadyExistsIn", {
            name: <strong>{job.name}</strong>,
            folder: job.targetDirectory,
          })}
        </p>
        <table className="conflict-table">
          <thead>
            <tr>
              <th />
              <th>{t("common.size")}</th>
              <th>{t("dialogs.modified")}</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th>
                {job.direction === "upload" ? t("dialogs.localFile") : t("dialogs.serverFile")}
              </th>
              <td>{formatSize(info.sourceSize)}</td>
              <td>{formatDate(info.sourceModified, dateFormat)}</td>
            </tr>
            <tr>
              <th>{t("dialogs.existingFile")}</th>
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
            ? t("dialogs.doTheSameForOthers", { count: others })
            : t("dialogs.doTheSameForAny")}
        </label>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={() => answer("skip")}>
          {t("common.skip")}
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
