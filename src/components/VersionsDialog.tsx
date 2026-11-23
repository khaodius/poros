import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { Download, FileDiff, FileText, History, RotateCcw, Trash2 } from "lucide-react";
import { formatSize, pluralize } from "../lib/format";
import { toAppError, versions } from "../lib/ipc";
import type { FileEntry, ObjectVersion, Versioning } from "../lib/types";
import {
  NO_VERSIONS,
  STATUS_LABELS,
  formatVersionTime,
  shortTag,
  versionActions,
  versionStatus,
  versioningNote,
  type VersionAction,
} from "../lib/versions";
import { useSettingsStore } from "../state/settingsStore";
import {
  deleteVersion,
  downloadVersion,
  openVersion,
  restoreVersion,
  useVersioningStore,
  versioningKey,
  type VersionedFile,
} from "../state/versionActions";
import { CompareDialog, type CompareOptions, type CompareSide } from "./CompareDialog";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./Dialog";

interface VersionsDialogProps {
  file: VersionedFile;
  /** The group of the pane it was opened from, where an opened version goes. */
  nearGroupId?: string;
  onClose: () => void;
}

/** What the dialog shows: the history, that there is none, or why it could not tell. */
type HistoryState =
  | { phase: "loading" }
  | { phase: "listed"; entries: ObjectVersion[] }
  | { phase: "none"; bucketKeepsNone: boolean }
  | { phase: "failed"; message: string };

type Asking = { action: "restore" | "delete"; version: ObjectVersion };

const ACTIONS: { action: Exclude<VersionAction, "delete">; label: string; icon: ReactNode }[] = [
  { action: "restore", label: "Restore this version", icon: <RotateCcw size={14} /> },
  { action: "download", label: "Download this version", icon: <Download size={14} /> },
  { action: "open", label: "Open in the editor", icon: <FileText size={14} /> },
  { action: "compare", label: "Compare with the current file", icon: <FileDiff size={14} /> },
];

const DIALOG_WIDTH = 820;

/** Every version and delete marker of a file, newest first, and what can be done with each. */
export function VersionsDialog({ file, nearGroupId, onClose }: VersionsDialogProps) {
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const [history, setHistory] = useState<HistoryState>({ phase: "loading" });
  const [versioning, setVersioning] = useState<Versioning>();
  const [reloads, setReloads] = useState(0);
  const [asking, setAsking] = useState<Asking | null>(null);
  const [comparing, setComparing] = useState<ObjectVersion | null>(null);
  const [notice, setNotice] = useState<{ tone: "info" | "error"; text: string } | null>(null);
  const { sessionId, path, name } = file;

  useEffect(() => {
    let current = true;
    const status = versions.status(sessionId, path).catch((): Versioning => "unknown");
    Promise.all([status, versions.list(sessionId, path)])
      .then(([found, entries]) => {
        if (!current) return;
        setVersioning(found);
        const key = versioningKey(sessionId, path);
        if (key) {
          useVersioningStore.setState((state) => ({ known: { ...state.known, [key]: found } }));
        }
        setHistory(
          found === "off" || entries.length === 0
            ? { phase: "none", bucketKeepsNone: found === "off" }
            : { phase: "listed", entries },
        );
      })
      .catch((caught) => {
        if (!current) return;
        const error = toAppError(caught);
        setHistory(
          error.kind === "unsupported"
            ? { phase: "none", bucketKeepsNone: true }
            : { phase: "failed", message: error.message },
        );
      });
    return () => {
      current = false;
    };
  }, [sessionId, path, reloads]);

  const when = (version: ObjectVersion) => formatVersionTime(version.modified, dateFormat);
  const entries = history.phase === "listed" ? history.entries : [];
  const showClass = entries.some((version) => version.storageClass);
  const showTag = entries.some((version) => version.etag);
  const note = versioningNote(versioning);

  const act = async (action: VersionAction, version: ObjectVersion) => {
    setNotice(null);
    try {
      if (action === "restore" || action === "delete") {
        setAsking({ action, version });
      } else if (action === "compare") {
        setComparing(version);
      } else if (action === "open") {
        await openVersion(file, version, dateFormat, nearGroupId);
        onClose();
      } else {
        const folder = await downloadVersion(file, version);
        if (folder) {
          setNotice({
            tone: "info",
            text: `Downloading the version from ${when(version)} to ${folder}. The transfer queue shows how it goes.`,
          });
        }
      }
    } catch (caught) {
      setNotice({ tone: "error", text: toAppError(caught).message });
    }
  };

  return (
    <Dialog
      title={`Versions of ${name}`}
      onClose={onClose}
      width={DIALOG_WIDTH}
      className="versions-dialog"
    >
      <p className="versions-path selectable" title={path}>
        {path}
      </p>
      {note && <p className="versions-note">{note}</p>}
      {history.phase === "loading" && <p className="versions-empty">Loading versions...</p>}
      {history.phase === "none" && (
        <p className="versions-empty">
          <History size={16} />
          {history.bucketKeepsNone
            ? `${NO_VERSIONS}. Turn on versioning for the bucket to keep the files it replaces and deletes.`
            : "No versions of this file were found."}
        </p>
      )}
      {history.phase === "failed" && <p className="form-error">{history.message}</p>}
      {history.phase === "listed" && (
        <div className="versions-table" role="table" aria-label={`Versions of ${name}`}>
          <div className="versions-row versions-heading" role="row">
            <span role="columnheader" className="versions-time">
              Saved
            </span>
            <span role="columnheader" className="align-end versions-size">
              Size
            </span>
            <span role="columnheader" className="versions-state">
              Status
            </span>
            {showClass && (
              <span role="columnheader" className="versions-detail">
                Storage class
              </span>
            )}
            {showTag && (
              <span role="columnheader" className="versions-detail versions-tag">
                ETag
              </span>
            )}
            <span role="columnheader" className="versions-actions">
              Actions
            </span>
          </div>
          {entries.map((version) => {
            const status = versionStatus(version);
            const offered = versionActions(version, entries, name);
            const deleteLabel =
              version.kind === "deleteMarker"
                ? "Remove this delete marker"
                : "Delete this version for good";
            return (
              <div
                key={version.versionId}
                className={[
                  "versions-row",
                  version.kind === "deleteMarker" && "is-marker",
                  version.latest && "is-latest",
                ]
                  .filter(Boolean)
                  .join(" ")}
                role="row"
                title={`Version ${version.versionId}`}
              >
                <span role="cell" className="versions-time">
                  {when(version)}
                </span>
                <span role="cell" className="align-end versions-size">
                  {version.kind === "version" ? formatSize(version.size) : ""}
                </span>
                <span role="cell" className="versions-state">
                  {status && (
                    <span className={`version-status is-${status}`}>{STATUS_LABELS[status]}</span>
                  )}
                </span>
                {showClass && (
                  <span role="cell" className="versions-detail">
                    {version.storageClass ?? ""}
                  </span>
                )}
                {showTag && (
                  <span role="cell" className="versions-detail versions-tag" title={version.etag}>
                    {version.etag ? shortTag(version.etag) : ""}
                  </span>
                )}
                <span role="cell" className="versions-actions">
                  {ACTIONS.map(({ action, label, icon }) =>
                    offered.has(action) ? (
                      <button
                        key={action}
                        type="button"
                        className="icon-button"
                        title={label}
                        aria-label={`${label}, ${when(version)}`}
                        onClick={() => void act(action, version)}
                      >
                        {icon}
                      </button>
                    ) : (
                      <span key={action} className="versions-action-gap" aria-hidden />
                    ),
                  )}
                  <button
                    type="button"
                    className="icon-button versions-delete"
                    title={deleteLabel}
                    aria-label={`${deleteLabel}, ${when(version)}`}
                    onClick={() => void act("delete", version)}
                  >
                    <Trash2 size={14} />
                  </button>
                </span>
              </div>
            );
          })}
        </div>
      )}
      {notice && (
        <p className={notice.tone === "error" ? "form-error" : "versions-notice"} role="status">
          {notice.text}
        </p>
      )}
      <div className="dialog-actions">
        {history.phase === "listed" && (
          <span className="versions-count">{pluralize(entries.length, "entry", "entries")}</span>
        )}
        <button type="button" className="button" data-autofocus onClick={onClose}>
          Close
        </button>
      </div>

      {asking?.action === "restore" && (
        <ConfirmDialog
          title="Restore version"
          confirmLabel="Restore"
          onClose={() => setAsking(null)}
          onConfirm={async () => {
            await restoreVersion(file, asking.version, dateFormat);
            setReloads((count) => count + 1);
          }}
        >
          <p>
            Make the version of <strong>{name}</strong> from {when(asking.version)} the current file
            again?
          </p>
          <p>
            {entries[0]?.kind === "deleteMarker"
              ? "This brings the deleted file back. The delete stays in the history."
              : "The current content stays in the history, so it can be restored in turn."}
          </p>
        </ConfirmDialog>
      )}

      {asking?.action === "delete" && (
        <ConfirmDialog
          title={asking.version.kind === "deleteMarker" ? "Remove delete marker" : "Delete version"}
          confirmLabel={asking.version.kind === "deleteMarker" ? "Remove marker" : "Delete version"}
          danger
          onClose={() => setAsking(null)}
          onConfirm={async () => {
            await deleteVersion(file, asking.version, dateFormat);
            setReloads((count) => count + 1);
          }}
        >
          <DeleteVersionSummary name={name} version={asking.version} when={when(asking.version)} />
        </ConfirmDialog>
      )}

      {comparing && (
        <VersionComparison
          file={file}
          version={comparing}
          when={when(comparing)}
          onClose={() => setComparing(null)}
        />
      )}
    </Dialog>
  );
}

function DeleteVersionSummary({
  name,
  version,
  when,
}: {
  name: string;
  version: ObjectVersion;
  when: string;
}) {
  const marker = version.kind === "deleteMarker";
  return (
    <>
      <p>
        {marker ? "Remove the delete marker" : "Delete the version"} of <strong>{name}</strong> from{" "}
        {when} for good? This cannot be undone.
      </p>
      {version.latest && (
        <p>
          {marker
            ? "The file comes back as its newest version."
            : "The version before it becomes the current file."}
        </p>
      )}
    </>
  );
}

/** One version side by side with the file as it is now. */
function VersionComparison({
  file,
  version,
  when,
  onClose,
}: {
  file: VersionedFile;
  version: ObjectVersion;
  when: string;
  onClose: () => void;
}) {
  const { sessionId, path, name } = file;
  const [left, right] = useMemo((): [CompareSide, CompareSide] => {
    const location = { kind: "remote" as const, sessionId };
    return [
      { location, path, name, where: `Version from ${when}` },
      { location, path, name, where: "Current version" },
    ];
  }, [sessionId, path, name, when]);
  const compare = useCallback(
    (operationId: string, options: CompareOptions) =>
      versions.compare({
        operationId,
        sessionId,
        path,
        versionId: version.versionId,
        ...options,
      }),
    [sessionId, path, version.versionId],
  );
  return <CompareDialog left={left} right={right} compare={compare} onClose={onClose} />;
}

interface PurgeDeletedDialogProps {
  entries: FileEntry[];
  onConfirm: () => Promise<void>;
  onClose: () => void;
}

/** Asks before deleted files and folders go for good, with every version they still have. */
export function PurgeDeletedDialog({ entries, onConfirm, onClose }: PurgeDeletedDialogProps) {
  const folders = entries.some((entry) => entry.kind === "dir");
  return (
    <ConfirmDialog
      title="Delete for good"
      confirmLabel="Delete for good"
      danger
      onClose={onClose}
      onConfirm={onConfirm}
    >
      <p>
        Remove{" "}
        {entries.length === 1 ? (
          <strong>{entries[0].name}</strong>
        ) : (
          pluralize(entries.length, "deleted item")
        )}{" "}
        and every version kept of it for good? This cannot be undone.
      </p>
      {folders && (
        <p className="warning-text">
          Everything deleted under a folder goes too. Files still in it are not touched.
        </p>
      )}
      {entries.length > 1 && (
        <ul className="delete-preview">
          {entries.slice(0, 5).map((entry) => (
            <li key={entry.path}>{entry.name}</li>
          ))}
          {entries.length > 5 && <li>and {entries.length - 5} more</li>}
        </ul>
      )}
    </ConfirmDialog>
  );
}
