import { useMemo, useState } from "react";
import type { OutcomeRow, OutcomeStatus } from "../lib/bulkRename";
import type { RenameReport } from "../state/bulkRename";
import { Dialog } from "./Dialog";
import { RenameTable, type RenameTableRow } from "./RenameTable";
import { useT, t } from "../i18n";

type OutcomeFilter = "all" | "renamed" | "notRenamed";
const FILTER_LABELS: Record<OutcomeFilter, string> = {
  get all() {
    return t("rename.all");
  },
  get renamed() {
    return t("rename.renamed");
  },
  get notRenamed() {
    return t("rename.notRenamed");
  },
};

const STATUS_LABELS: Record<OutcomeStatus, string> = {
  get renamed() {
    return t("rename.renamed");
  },
  get failed() {
    return t("rename.failed");
  },
  get stranded() {
    return t("rename.leftUnderAnotherName");
  },
  get left() {
    return t("rename.notRenamed");
  },
};

function tableRow(outcome: OutcomeRow): RenameTableRow {
  const label = STATUS_LABELS[outcome.status];
  return {
    name: outcome.from,
    newName: outcome.to,
    tone: outcome.status === "renamed" ? "plain" : outcome.status === "left" ? "dim" : "bad",
    status: outcome.message ? `${label}: ${outcome.message}` : label,
  };
}

interface RenameReportDialogProps {
  report: RenameReport;
  onClose: () => void;
}

/** A bulk rename that stopped part way: which items have their new name, and which do not. */
export function RenameReportDialog({ report, onClose }: RenameReportDialogProps) {
  const t = useT();
  const { outcomes, summary, folder } = report;
  const { failure, stranded } = summary;
  const [filter, setFilter] = useState<OutcomeFilter>("all");
  const renamedCount = summary.renamed.length;
  const counts: Record<OutcomeFilter, number> = {
    all: outcomes.length,
    renamed: renamedCount,
    notRenamed: outcomes.length - renamedCount,
  };
  const shown = useMemo(
    () =>
      filter === "all"
        ? outcomes
        : outcomes.filter((outcome) => (outcome.status === "renamed") === (filter === "renamed")),
    [outcomes, filter],
  );

  return (
    <Dialog
      title={t("rename.renamingStopped")}
      onClose={onClose}
      width={760}
      className="rename-report-dialog"
    >
      <div className="rename-report">
        <div className="confirm-body selectable">
          <p>
            {t.rich("rename.renamedOfItemsIn", {
              count: renamedCount,
              total: t("rename.items", { count: outcomes.length }),
              folder: <strong>{folder}</strong>,
            })}{" "}
            {stranded
              ? t("rename.theOthersKeepTheirOldNamesButOne")
              : t("rename.theOthersKeepTheirOldNames")}
          </p>
          {failure && (
            <p className="form-error">
              {t.rich("rename.couldNotBeRenamedTo", {
                from: <strong>{failure.from}</strong>,
                to: <strong>{failure.to}</strong>,
                message: failure.message,
              })}
            </p>
          )}
          {stranded && (
            <p className="form-error">
              {t.rich("rename.isNowNamedBecause", {
                from: <strong>{stranded.from}</strong>,
                temporary: <strong>{stranded.temporary}</strong>,
                message: stranded.message,
              })}
            </p>
          )}
        </div>
        <div className="toggle-group" role="group" aria-label={t("rename.rowsShown")}>
          {(Object.keys(FILTER_LABELS) as OutcomeFilter[]).map((value) => (
            <button
              key={value}
              type="button"
              aria-pressed={filter === value}
              className={`toggle ${filter === value ? "is-on" : ""}`}
              onClick={() => setFilter(value)}
            >
              {FILTER_LABELS[value]} {counts[value]}
            </button>
          ))}
        </div>
        <RenameTable
          label={t("rename.whatBecameOfEachRename")}
          count={shown.length}
          row={(index) => tableRow(shown[index])}
          empty={t("rename.noItems")}
        />
        <div className="dialog-actions">
          <button type="button" className="button button-primary" data-autofocus onClick={onClose}>
            {t("common.close")}
          </button>
        </div>
      </div>
    </Dialog>
  );
}
