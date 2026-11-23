import { useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { deleteFraction, deleteProgressText, formatCount } from "../lib/deletion";
import { formatSize, pluralize } from "../lib/format";
import { fileOperations, onFileOperation } from "../lib/ipc";
import { baseName } from "../lib/path";
import type { NameClash } from "../lib/types";
import { restoreProgressText } from "../lib/versions";
import { closeRenameReport, useBulkRenameStore } from "../state/bulkRename";
import {
  useOperationStore,
  type NameClashQuestion,
  type RunningOperation,
} from "../state/operationStore";
import { Dialog } from "./Dialog";
import { RenameReportDialog } from "./RenameReportDialog";

/** The moves and copies running in this window, and the question when names are taken. */
export function FileOperationsHost() {
  const update = useOperationStore((state) => state.update);
  const question = useOperationStore((state) => state.question);
  const renameReport = useBulkRenameStore((state) => state.report);

  useEffect(() => {
    const subscription = onFileOperation(update);
    return () => void subscription.then((unlisten) => unlisten());
  }, [update]);

  return (
    <>
      <OperationTray />
      {question && <NameClashDialog question={question} />}
      {renameReport && <RenameReportDialog report={renameReport} onClose={closeRenameReport} />}
    </>
  );
}

function OperationTray() {
  const running = useOperationStore((state) => state.running);
  const [cancelling, setCancelling] = useState<ReadonlySet<string>>(new Set());
  const shown = running.filter((operation) => operation.shown);
  if (shown.length === 0) return null;

  const cancel = (id: string) => {
    setCancelling((current) => new Set(current).add(id));
    void fileOperations.cancel(id);
  };

  return (
    <div className="operation-tray" role="status" aria-live="polite">
      {shown.map((operation) => (
        <div
          key={operation.id}
          className="operation"
          title={operation.kind === "extract" ? undefined : operation.current}
        >
          <LoaderCircle size={16} className="spin" />
          <div className="operation-text">
            <span className="operation-title">{operation.title}</span>
            {operation.kind === "delete" || operation.kind === "restore" ? (
              <DeleteDetail operation={operation} />
            ) : operation.kind === "extract" ? (
              <ExtractDetail operation={operation} />
            ) : operation.kind === "rename" ? (
              <RenameDetail operation={operation} />
            ) : (
              <span className="operation-detail">
                {pluralize(operation.files, "file")} done, {formatSize(operation.bytes)}
              </span>
            )}
          </div>
          <button
            type="button"
            className="button button-small"
            disabled={cancelling.has(operation.id)}
            onClick={() => cancel(operation.id)}
          >
            {cancelling.has(operation.id) ? "Cancelling..." : "Cancel"}
          </button>
        </div>
      ))}
    </div>
  );
}

/** Which program unpacks the archive, under a bar that moves until it is done. */
function ExtractDetail({ operation }: { operation: RunningOperation }) {
  return (
    <>
      <span className="operation-detail">{operation.current ?? "Unpacking"}</span>
      <span className="progress-track is-indeterminate" role="progressbar">
        <span className="progress-fill" />
      </span>
    </>
  );
}

/** How many items have their new name, and the one being renamed. */
function RenameDetail({ operation }: { operation: RunningOperation }) {
  const { files, total, current } = operation;
  const fraction = total ? Math.min(1, files / total) : null;
  return (
    <>
      <span className="operation-detail">
        {total ? `${formatCount(files)} of ${formatCount(total)} renamed` : "Starting"}
      </span>
      <span
        className={`progress-track ${fraction === null ? "is-indeterminate" : ""}`}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={fraction === null ? undefined : Math.round(fraction * 100)}
      >
        <span
          className="progress-fill"
          style={fraction === null ? undefined : { width: `${fraction * 100}%` }}
        />
      </span>
      {current && (
        <span className="operation-path" title={current}>
          <bdi>{current}</bdi>
        </span>
      )}
    </>
  );
}

/** How many entries are gone or back, where the operation is, and the latest that failed. */
function DeleteDetail({ operation }: { operation: RunningOperation }) {
  const fraction = deleteFraction(operation);
  const { current, failed, lastFailure } = operation;
  return (
    <>
      <span className="operation-detail">
        {operation.kind === "restore"
          ? restoreProgressText(operation)
          : deleteProgressText(operation)}
        {failed ? `, ${formatCount(failed)} failed` : ""}
      </span>
      <span
        className={`progress-track ${fraction === null ? "is-indeterminate" : ""}`}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={fraction === null ? undefined : Math.round(fraction * 100)}
      >
        <span
          className="progress-fill"
          style={fraction === null ? undefined : { width: `${fraction * 100}%` }}
        />
      </span>
      {current && (
        <span className="operation-path" title={current}>
          <bdi>{current}</bdi>
        </span>
      )}
      {lastFailure && (
        <span className="operation-failure" title={`${lastFailure.path}: ${lastFailure.message}`}>
          {baseName(lastFailure.path)}: {lastFailure.message}
        </span>
      )}
    </>
  );
}

const CHOICES: { choice: NameClash; label: string }[] = [
  { choice: "skip", label: "Skip" },
  { choice: "keepBoth", label: "Keep both" },
  { choice: "replace", label: "Replace" },
];
const PREVIEW_NAMES = 5;

function NameClashDialog({ question }: { question: NameClashQuestion }) {
  const { names, total, folder, mode, answer } = question;
  const preview = names.slice(0, PREVIEW_NAMES);
  return (
    <Dialog title="Names already taken" onClose={() => answer(null)} width={480}>
      <div className="confirm-body">
        {names.length === 1 ? (
          <p>
            <strong>{names[0]}</strong> is already in <strong>{folder}</strong>.
          </p>
        ) : (
          <>
            <p>
              {names.length === total
                ? `All ${total} items`
                : `${names.length} of the ${total} items`}{" "}
              {mode === "move" ? "being moved" : "being copied"} have names already in{" "}
              <strong>{folder}</strong>.
            </p>
            <ul className="delete-preview">
              {preview.map((name) => (
                <li key={name}>{name}</li>
              ))}
              {names.length > preview.length && <li>and {names.length - preview.length} more</li>}
            </ul>
          </>
        )}
        <p>
          Replace overwrites files and merges folders. Keep both gives the new{" "}
          {names.length === 1 ? "item a name" : "items names"} like <em>name (2)</em>.
        </p>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button push-left" onClick={() => answer(null)}>
          Cancel
        </button>
        {CHOICES.map(({ choice, label }) => (
          <button
            key={choice}
            type="button"
            data-autofocus={choice === "replace" ? true : undefined}
            className={`button ${choice === "replace" ? "button-primary" : ""}`}
            onClick={() => answer(choice)}
          >
            {label}
          </button>
        ))}
      </div>
    </Dialog>
  );
}
