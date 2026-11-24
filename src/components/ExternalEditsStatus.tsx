import { useEffect, useRef, useState, type ReactNode } from "react";
import {
  FilePen,
  FolderOpen,
  Settings2,
  ShieldCheck,
  SquareArrowOutUpRight,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import {
  editStateLabel,
  editsSummary,
  needsAttention,
  sessionForLeftover,
  transferFraction,
} from "../lib/externalEditor";
import { formatDate, formatSize, formatTime } from "../lib/format";
import { external } from "../lib/ipc";
import type { DateFormat } from "../lib/settings";
import type { ExternalEdit, ExternalLeftover } from "../lib/types";
import { editAction, openProgramSettings, useExternalEditStore } from "../state/externalEditStore";
import { useSessionStore } from "../state/sessionStore";
import { useSettingsStore } from "../state/settingsStore";
import { ConfirmDialog } from "./ConfirmDialog";
import { FileIcon } from "./FileIcon";
import { ProgressBar } from "./ProgressBar";
import { useT } from "../i18n";

/** The status bar's word on files open in other programs, opening the list of them. */
export function ExternalEditsStatus() {
  const t = useT();
  const edits = useExternalEditStore((state) => state.edits);
  const leftovers = useExternalEditStore((state) => state.leftovers);
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const summary = editsSummary(edits, leftovers);
  if (!summary && !open) return null;

  const attention = edits.some(needsAttention);
  const moving = edits.some((edit) => edit.state === "downloading" || edit.state === "uploading");
  const close = () => {
    setOpen(false);
    triggerRef.current?.focus();
  };
  return (
    <span className="edits-status">
      <button
        ref={triggerRef}
        type="button"
        className={`status-item edits-status-button ${attention ? "needs-attention" : ""}`}
        aria-expanded={open}
        aria-haspopup="dialog"
        title={t("openWith.filesOpenInOtherPrograms")}
        onClick={() => setOpen(!open)}
      >
        <FilePen size={12} className={moving ? "is-moving" : ""} />
        {summary ?? "Not editing any files"}
      </button>
      {open && <EditsPopover edits={edits} leftovers={leftovers} onClose={close} />}
    </span>
  );
}

interface EditsPopoverProps {
  edits: ExternalEdit[];
  leftovers: ExternalLeftover[];
  onClose: () => void;
}

/** Something that loses saved changes, asked about first. */
export interface Confirmation {
  title: string;
  confirmLabel: string;
  body: ReactNode;
  action: () => Promise<void>;
}

function EditsPopover({ edits, leftovers, onClose }: EditsPopoverProps) {
  const t = useT();
  const popoverRef = useRef<HTMLElement>(null);
  const [confirming, setConfirming] = useState<Confirmation | null>(null);
  const uploadOnSave = useSettingsStore((state) => state.settings.editing.uploadOnSave);

  // A dialog opened from the list keeps it open behind it.
  const asking = confirming !== null;
  useEffect(() => {
    if (asking) return;
    const closeOnOutside = (event: MouseEvent) => {
      const target = event.target as Element;
      if (!popoverRef.current?.contains(target) && !target.closest?.(".edits-status")) onClose();
    };
    const closeOnKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", closeOnOutside, true);
    window.addEventListener("keydown", closeOnKey);
    return () => {
      window.removeEventListener("mousedown", closeOnOutside, true);
      window.removeEventListener("keydown", closeOnKey);
    };
  }, [asking, onClose]);

  useEffect(() => {
    popoverRef.current?.querySelector<HTMLElement>("button")?.focus({ preventScroll: true });
  }, []);

  const stop = (edit: ExternalEdit) => {
    if (edit.state === "watching" || edit.state === "downloading") {
      editAction(() => external.stop(edit.id));
      return;
    }
    setConfirming({
      title: t("openWith.stopEditing", { name: edit.name }),
      confirmLabel: t("openWith.stopAndDiscard"),
      body: (
        <p>
          Changes saved in {edit.program || "the program"} have not reached {edit.server}. Stopping
          deletes the working copy with them.
        </p>
      ),
      action: () => external.stop(edit.id),
    });
  };

  return (
    <section
      ref={popoverRef}
      className="edits-popover"
      role="dialog"
      aria-label={t("openWith.filesOpenInOtherPrograms")}
    >
      <header className="edits-popover-header">
        <h3>{t("openWith.openInOtherPrograms")}</h3>
        <button
          type="button"
          className="icon-button"
          title={t("openWith.settingsForOpeningFiles")}
          aria-label={t("openWith.settingsForOpeningFiles")}
          onClick={() => {
            onClose();
            openProgramSettings();
          }}
        >
          <Settings2 size={14} />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label={t("common.close")}
          onClick={onClose}
        >
          <X size={14} />
        </button>
      </header>
      {edits.length === 0 && leftovers.length === 0 ? (
        <p className="edits-empty">{t("openWith.noFilesAreOpenIn")}</p>
      ) : (
        <ul className="edits-list">
          {edits.map((edit) => (
            <EditRow key={edit.id} edit={edit} onStop={() => stop(edit)} />
          ))}
        </ul>
      )}
      {leftovers.length > 0 && <LeftoverList leftovers={leftovers} onConfirm={setConfirming} />}
      {edits.length > 0 && (
        <p className="edits-footnote">
          {uploadOnSave
            ? "Each save is uploaded as soon as the program has written it."
            : "Saves wait for you to upload them."}
        </p>
      )}
      {confirming && (
        <ConfirmDialog
          title={confirming.title}
          confirmLabel={confirming.confirmLabel}
          danger
          onConfirm={confirming.action}
          onClose={() => setConfirming(null)}
        >
          {confirming.body}
        </ConfirmDialog>
      )}
    </section>
  );
}

function EditRow({ edit, onStop }: { edit: ExternalEdit; onStop: () => void }) {
  const t = useT();
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const fraction = transferFraction(edit);
  const canUpload = ["changed", "failed", "waiting"].includes(edit.state) && !edit.blocked;
  const where = [edit.server, edit.program].filter(Boolean).join(" · ");
  const resolve = () => useExternalEditStore.setState({ resolving: edit.id });
  const pickEditor = () =>
    useExternalEditStore.setState({
      prompt: {
        kind: "executable",
        editId: edit.id,
        target: {
          side: "remote",
          sessionId: edit.sessionId,
          path: edit.remotePath,
          name: edit.name,
        },
      },
    });
  return (
    <li className={`edit-row state-${edit.state} ${needsAttention(edit) ? "needs-attention" : ""}`}>
      <FileIcon entry={{ name: edit.name, kind: "file" }} />
      <div className="edit-main">
        <div className="edit-title">
          <span className="edit-name" title={edit.remotePath}>
            {edit.name}
          </span>
          <span className="edit-state">{editStateLabel(edit)}</span>
        </div>
        <div className="edit-where" title={`${edit.server}: ${edit.remotePath}`}>
          {edit.inVault && <ShieldCheck size={11} aria-label={t("openWith.inAVault")} />}
          <span>{where}</span>
          {edit.lastUpload !== undefined && (
            <span className="edit-uploaded">
              {t("openWith.uploaded", {
                uploadTime: formatUploadTime(edit.lastUpload, dateFormat),
              })}
            </span>
          )}
        </div>
        {fraction !== null && (
          <div className="edit-progress">
            <ProgressBar
              fraction={fraction}
              label={
                edit.detail ??
                t("openWith.of", {
                  size: formatSize(edit.transferred),
                  size2: formatSize(edit.size),
                })
              }
              labelKind="bytes"
              labelInside={false}
            />
          </div>
        )}
        {edit.detail && fraction === null && <p className="edit-detail">{edit.detail}</p>}
        {(edit.state === "conflict" || edit.blocked) && (
          <div className="edit-question">
            {edit.state === "conflict" && (
              <button type="button" className="button button-small" onClick={resolve}>
                {t("openWith.decideWhatToUpload")}
              </button>
            )}
            {edit.blocked && (
              <button type="button" className="button button-small" onClick={pickEditor}>
                {t("openWith.openInAnEditor")}
              </button>
            )}
          </div>
        )}
      </div>
      <div className="edit-actions">
        <button
          type="button"
          className="icon-button"
          title={t("openWith.uploadNow")}
          aria-label={t("openWith.uploadNow2", { name: edit.name })}
          disabled={!canUpload}
          onClick={() => editAction(() => external.upload(edit.id))}
        >
          <Upload size={14} />
        </button>
        <button
          type="button"
          className="icon-button"
          title={t("openWith.openAgain")}
          aria-label={t("openWith.openAgain2", { name: edit.name })}
          disabled={edit.state === "downloading" || edit.blocked}
          onClick={() => editAction(() => external.reopen(edit.id, null))}
        >
          <SquareArrowOutUpRight size={14} />
        </button>
        <button
          type="button"
          className="icon-button"
          title={t("openWith.showTheWorkingCopysFolder")}
          aria-label={t("openWith.showTheFolderOf", { name: edit.name })}
          disabled={edit.state === "downloading"}
          onClick={() => editAction(() => external.reveal(edit.id))}
        >
          <FolderOpen size={14} />
        </button>
        <button
          type="button"
          className="icon-button"
          title={
            edit.state === "downloading"
              ? t("common.cancel")
              : t("openWith.stopEditingAndDeleteThe")
          }
          aria-label={t("openWith.stopEditing2", { name: edit.name })}
          onClick={onStop}
        >
          <X size={14} />
        </button>
      </div>
    </li>
  );
}

/** Today's uploads by the minute, older ones by the date. */
function formatUploadTime(epochMillis: number, dateFormat: DateFormat): string {
  const today = new Date().toDateString() === new Date(epochMillis).toDateString();
  const minute = formatTime(epochMillis).slice(0, "hh:mm".length);
  return today ? `at ${minute}` : formatDate(epochMillis / 1000, dateFormat);
}

interface LeftoverListProps {
  leftovers: ExternalLeftover[];
  onConfirm: (confirmation: Confirmation) => void;
}

export function LeftoverList({ leftovers, onConfirm }: LeftoverListProps) {
  const t = useT();
  const sessions = useSessionStore((state) => state.sessions);
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const connected = Object.values(sessions)
    .filter((entry) => entry.status === "connected")
    .map((entry) => entry.info);
  const refresh = () => void useExternalEditStore.getState().loadLeftovers();
  return (
    <>
      <h4 className="edits-section">{t("openWith.savedButNotUploadedWhen")}</h4>
      <ul className="edits-list">
        {leftovers.map((leftover) => {
          const session = sessionForLeftover(leftover, connected);
          const server = `${leftover.username}@${leftover.host}`;
          return (
            <li key={leftover.id} className="edit-row state-changed">
              <FileIcon entry={{ name: leftover.name, kind: "file" }} />
              <div className="edit-main">
                <div className="edit-title">
                  <span className="edit-name" title={leftover.remotePath}>
                    {leftover.name}
                  </span>
                  <span className="edit-state">{formatSize(leftover.size)}</span>
                </div>
                <div className="edit-where" title={`${leftover.server}: ${leftover.remotePath}`}>
                  {leftover.vaultRoot && (
                    <ShieldCheck size={11} aria-label={t("openWith.inAVault")} />
                  )}
                  <span>{leftover.server}</span>
                  {leftover.modified !== undefined && (
                    <span className="edit-uploaded">
                      {t("openWith.saved", {
                        date: formatDate(leftover.modified / 1000, dateFormat),
                      })}
                    </span>
                  )}
                </div>
                {leftover.vaultRoot && (
                  <p className="edit-detail">{t("openWith.thisCopyOfAVault")}</p>
                )}
                {!session && (
                  <p className="edit-detail">
                    Connect to {server}
                    {leftover.vaultRoot ? " and unlock its vault" : ""} to upload it.
                  </p>
                )}
              </div>
              <div className="edit-actions">
                <button
                  type="button"
                  className="icon-button"
                  title={
                    session
                      ? t("openWith.uploadTo", { label: session.label })
                      : t("openWith.connectToFirst", { server })
                  }
                  aria-label={t("openWith.upload2", { name: leftover.name })}
                  disabled={!session}
                  onClick={() =>
                    session &&
                    editAction(async () => {
                      await external.recover(leftover.id, session.id);
                      refresh();
                    })
                  }
                >
                  <Upload size={14} />
                </button>
                <button
                  type="button"
                  className="icon-button"
                  title={t("openWith.discardTheSavedChanges")}
                  aria-label={t("openWith.discard", { name: leftover.name })}
                  onClick={() =>
                    onConfirm({
                      title: t("openWith.discard2", { name: leftover.name }),
                      confirmLabel: t("common.discard"),
                      body: (
                        <p>
                          The changes saved to it never reached {leftover.server}. Discarding
                          {leftover.vaultRoot ? " wipes" : " deletes"} them.
                        </p>
                      ),
                      action: async () => {
                        await external.discardLeftover(leftover.id);
                        refresh();
                      },
                    })
                  }
                >
                  <Trash2 size={14} />
                </button>
                <button
                  type="button"
                  className="icon-button"
                  title={t("openWith.showTheWorkingCopysFolder")}
                  aria-label={t("openWith.showTheFolderOf", { name: leftover.name })}
                  onClick={() => editAction(() => external.revealLeftover(leftover.id))}
                >
                  <FolderOpen size={14} />
                </button>
              </div>
            </li>
          );
        })}
      </ul>
    </>
  );
}
