import { useMemo, useState } from "react";
import type { OutcomeRow, OutcomeStatus } from "../lib/bulkRename";
import { pluralize } from "../lib/format";
import type { RenameReport } from "../state/bulkRename";
import { Dialog } from "./Dialog";
import { RenameTable, type RenameTableRow } from "./RenameTable";

type OutcomeFilter = "all" | "renamed" | "notRenamed";
const FILTER_LABELS: Record<OutcomeFilter, string> = {
  all: "All",
  renamed: "Renamed",
  notRenamed: "Not renamed",
};

const STATUS_LABELS: Record<OutcomeStatus, string> = {
  renamed: "Renamed",
  failed: "Failed",
  stranded: "Left under another name",
  left: "Not renamed",
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
    <Dialog title="Renaming stopped" onClose={onClose} width={760} className="rename-report-dialog">
      <div className="rename-report">
        <div className="confirm-body selectable">
          <p>
            {renamedCount} of {pluralize(outcomes.length, "item")} in <strong>{folder}</strong>{" "}
            {renamedCount === 1 ? "has its" : "have their"} new name.
            {stranded
              ? " The others keep their old names, but for the one below."
              : " The others keep their old names."}
          </p>
          {failure && (
            <p className="form-error">
              <strong>{failure.from}</strong> could not be renamed to <strong>{failure.to}</strong>:{" "}
              {failure.message}
            </p>
          )}
          {stranded && (
            <p className="form-error">
              <strong>{stranded.from}</strong> is now named <strong>{stranded.temporary}</strong>,
              because it could not be given its name back: {stranded.message}. Rename it by hand.
            </p>
          )}
        </div>
        <div className="toggle-group" role="group" aria-label="Rows shown">
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
          label="What became of each rename"
          count={shown.length}
          row={(index) => tableRow(shown[index])}
          empty="No items."
        />
        <div className="dialog-actions">
          <button type="button" className="button button-primary" data-autofocus onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </Dialog>
  );
}
