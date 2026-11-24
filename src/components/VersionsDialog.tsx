import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { Download, FileDiff, FileText, History, RotateCcw, Trash2 } from "lucide-react";
import { formatSize } from "../lib/format";
import { toAppError, versions } from "../lib/ipc";
import type { DirListing, FileEntry, ObjectVersion, Versioning } from "../lib/types";
import {
  STATUS_LABELS,
  afterDeleting,
  entriesStillDeleted,
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
import { t, useT } from "../i18n";

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
  {
    action: "restore",
    get label() {
      return t("versions.restoreThisVersion");
    },
    icon: <RotateCcw size={14} />,
  },
  {
    action: "download",
    get label() {
      return t("versions.downloadThisVersion");
    },
    icon: <Download size={14} />,
  },
  {
    action: "open",
    get label() {
      return t("versions.openInTheEditor");
    },
    icon: <FileText size={14} />,
  },
  {
    action: "compare",
    get label() {
      return t("versions.compareWithTheCurrentFile");
    },
    icon: <FileDiff size={14} />,
  },
];

const DIALOG_WIDTH = 820;

/** Every version and delete marker of a file, newest first, and what can be done with each. */
export function VersionsDialog({ file, nearGroupId, onClose }: VersionsDialogProps) {
  const t = useT();
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
            text: t("versions.downloadingTheVersionFromTo", { when: when(version), folder }),
          });
        }
      }
    } catch (caught) {
      setNotice({ tone: "error", text: toAppError(caught).message });
    }
  };

  return (
    <Dialog
      title={t("versions.versionsOf", { name })}
      onClose={onClose}
      width={DIALOG_WIDTH}
      className="versions-dialog"
    >
      <p className="versions-path selectable" title={path}>
        {path}
      </p>
      {note && <p className="versions-note">{note}</p>}
      {history.phase === "loading" && (
        <p className="versions-empty">{t("versions.loadingVersions")}</p>
      )}
      {history.phase === "none" && (
        <p className="versions-empty">
          <History size={16} />
          {history.bucketKeepsNone
            ? t("versions.turnOnVersioningForThe")
            : t("versions.noVersionsOfThisFile")}
        </p>
      )}
      {history.phase === "failed" && <p className="form-error">{history.message}</p>}
      {history.phase === "listed" && (
        <div
          className="versions-table"
          role="table"
          aria-label={t("versions.versionsOf", { name })}
        >
          <div className="versions-row versions-heading" role="row">
            <span role="columnheader" className="versions-time">
              {t("versions.saved")}
            </span>
            <span role="columnheader" className="align-end versions-size">
              {t("common.size")}
            </span>
            <span role="columnheader" className="versions-state">
              {t("versions.status")}
            </span>
            {showClass && (
              <span role="columnheader" className="versions-detail">
                {t("versions.storageClass")}
              </span>
            )}
            {showTag && (
              <span role="columnheader" className="versions-detail versions-tag">
                {t("versions.etag")}
              </span>
            )}
            <span role="columnheader" className="versions-actions">
              {t("versions.actions")}
            </span>
          </div>
          {entries.map((version) => {
            const status = versionStatus(version);
            const offered = versionActions(version, entries, name);
            const deleteLabel =
              version.kind === "deleteMarker"
                ? t("versions.removeThisDeleteMarker")
                : t("versions.deleteThisVersionForGood");
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
                title={t("versions.version", { versionId: version.versionId })}
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
          <span className="versions-count">{t("versions.entries", { count: entries.length })}</span>
        )}
        <button type="button" className="button" data-autofocus onClick={onClose}>
          {t("common.close")}
        </button>
      </div>

      {asking?.action === "restore" && (
        <ConfirmDialog
          title={t("versions.restoreVersion")}
          confirmLabel={t("versions.restore")}
          onClose={() => setAsking(null)}
          onConfirm={async () => {
            await restoreVersion(file, asking.version, dateFormat);
            setReloads((count) => count + 1);
          }}
        >
          <p>
            {t.rich("versions.makeTheVersionOf", {
              name: <strong>{name}</strong>,
              when: when(asking.version),
            })}
          </p>
          <p>
            {entries[0]?.kind === "deleteMarker"
              ? t("versions.thisBringsTheDeletedFile")
              : t("versions.theCurrentContentStaysIn")}
          </p>
        </ConfirmDialog>
      )}

      {asking?.action === "delete" && (
        <ConfirmDialog
          title={
            asking.version.kind === "deleteMarker"
              ? t("versions.removeDeleteMarker")
              : t("versions.deleteVersion")
          }
          confirmLabel={
            asking.version.kind === "deleteMarker"
              ? t("versions.removeMarker")
              : t("versions.deleteVersion")
          }
          danger
          onClose={() => setAsking(null)}
          onConfirm={async () => {
            await deleteVersion(file, asking.version, dateFormat);
            setReloads((count) => count + 1);
          }}
        >
          <DeleteVersionSummary
            name={name}
            version={asking.version}
            history={entries}
            when={when(asking.version)}
          />
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
  history,
  when,
}: {
  name: string;
  version: ObjectVersion;
  history: readonly ObjectVersion[];
  when: string;
}) {
  const t = useT();
  const marker = version.kind === "deleteMarker";
  const outcome = afterDeleting(version, history);
  return (
    <>
      <p>
        {t.rich(marker ? "versions.removeTheDeleteMarkerOf" : "versions.deleteTheVersionOf", {
          name: <strong>{name}</strong>,
          when,
        })}
      </p>
      {outcome && <p>{outcome}</p>}
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
  const t = useT();
  const { sessionId, path, name } = file;
  const timeKnown = version.modified !== null;
  const [left, right] = useMemo((): [CompareSide, CompareSide] => {
    const location = { kind: "remote" as const, sessionId };
    const where = timeKnown
      ? t("versions.versionFrom", { when })
      : t("versions.versionFromUnknownTime");
    return [
      { location, path, name, where },
      { location, path, name, where: t("versions.currentVersion") },
    ];
  }, [sessionId, path, name, when, timeKnown, t]);
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
  /** The deleted entries picked, as the listing had them then. */
  chosen: readonly FileEntry[];
  /** The folder's listing on show now. */
  listing: DirListing | null;
  onConfirm: (entries: FileEntry[]) => Promise<void>;
  onClose: () => void;
}

/**
 * Asks before deleted files and folders go for good, with every version they still have. It
 * acts only on those the folder still lists as deleted: one written again since is left alone.
 */
export function PurgeDeletedDialog({
  chosen,
  listing,
  onConfirm,
  onClose,
}: PurgeDeletedDialogProps) {
  const t = useT();
  const entries = useMemo(() => entriesStillDeleted(chosen, listing), [chosen, listing]);
  const folders = entries.some((entry) => entry.kind === "dir");
  return (
    <ConfirmDialog
      title={t("versions.deleteForGood")}
      confirmLabel={t("versions.deleteForGood")}
      danger
      disabled={entries.length === 0}
      onClose={onClose}
      onConfirm={() => onConfirm(entries)}
    >
      {entries.length === 0 ? (
        <p>
          {chosen.length === 1
            ? t("versions.isNoLongerDeleted", { name: chosen[0].name })
            : t("versions.theseItemsAreNoLongerDeleted")}
        </p>
      ) : (
        <p>
          {t.rich("versions.removeWithEveryVersion", {
            what:
              entries.length === 1 ? (
                <strong>{entries[0].name}</strong>
              ) : (
                t("versions.deletedItems", { count: entries.length })
              ),
          })}
        </p>
      )}
      {folders && <p className="warning-text">{t("versions.everythingDeletedUnderAFolder")}</p>}
      {entries.length > 1 && (
        <ul className="delete-preview">
          {entries.slice(0, 5).map((entry) => (
            <li key={entry.path}>{entry.name}</li>
          ))}
          {entries.length > 5 && <li>{t("versions.andMore", { count: entries.length - 5 })}</li>}
        </ul>
      )}
    </ConfirmDialog>
  );
}
