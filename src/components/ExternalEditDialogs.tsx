import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AppWindow, FilePen, FolderOpen, SquareArrowOutUpRight } from "lucide-react";
import { useLastUsedWindow } from "../hooks/useLastUsedWindow";
import {
  editStateLabel,
  editsOfSessions,
  looksLikeText,
  programFor,
  programLabel,
} from "../lib/externalEditor";
import { formatDate, formatSize } from "../lib/format";
import { external, onExternalEdits } from "../lib/ipc";
import type { ExternalEdit, ExternalEditNotice, ExternalResolution } from "../lib/types";
import {
  answerClosing,
  editAction,
  exitNow,
  openFileWith,
  openHeldBackWith,
  useExternalEditStore,
  type ClosePrompt,
  type OpenChoice,
  type OpenPrompt,
} from "../state/externalEditStore";
import { forgetListingOf } from "../state/listings";
import { useSessionStore } from "../state/sessionStore";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { isMainWindow } from "../state/tabActions";
import { useToastStore } from "../state/toastStore";
import { CompareDialog } from "./CompareDialog";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./Dialog";
import { LeftoverList, type Confirmation } from "./ExternalEditsStatus";
import { midSentence, t, useT } from "../i18n";

/** Keeps the list of files open in other programs, and asks what the window in use has to ask. */
export function ExternalEditsHost() {
  const asking = useLastUsedWindow();
  const askingRef = useRef(asking);
  useEffect(() => {
    askingRef.current = asking;
  }, [asking]);
  const resolving = useExternalEditStore((state) => state.resolving);
  const askingUpload = useExternalEditStore((state) => state.askingUpload);
  const prompt = useExternalEditStore((state) => state.prompt);
  const showingLeftovers = useExternalEditStore((state) => state.showingLeftovers);
  const closing = useExternalEditStore((state) => state.closing);
  const exiting = useExternalEditStore((state) => state.exiting);
  const edits = useExternalEditStore((state) => state.edits);
  const leftoverCount = useExternalEditStore((state) => state.leftovers.length);

  useEffect(() => {
    const store = useExternalEditStore.getState();
    const subscription = onExternalEdits((update) => {
      useExternalEditStore.getState().apply(update);
      const { notice } = update;
      if (notice?.kind === "uploaded") {
        // Its folder no longer lists the file as the cache remembers it. Only the window that
        // holds the edit's session knows the cache it is in, and tells every window.
        const edit = update.edits.find((candidate) => candidate.id === notice.editId);
        if (edit) forgetListingOf(edit.sessionId, edit.remotePath);
      }
      // Every window hears it; the one in use tells the user.
      if (notice && askingRef.current) tell(notice);
    });
    void store.load();
    void store.loadLeftovers().then(() => {
      // Asked once, as Poros starts.
      if (isMainWindow() && useExternalEditStore.getState().leftovers.length > 0) {
        useExternalEditStore.setState({ showingLeftovers: true });
      }
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, []);

  const conflicted = edits.find((edit) => edit.id === resolving && edit.state === "conflict");
  const uploadAsked = edits.find((edit) => edit.id === askingUpload[0]);
  return (
    <>
      {conflicted && <ConflictPrompt edit={conflicted} />}
      {!conflicted && uploadAsked && <UploadPrompt edit={uploadAsked} />}
      {prompt && <OpenQuestion prompt={prompt} />}
      {showingLeftovers && leftoverCount > 0 && <LeftoversDialog />}
      {closing && <CloseTabPrompt closing={closing} />}
      {exiting && <ExitPrompt editIds={exiting} />}
    </>
  );
}

/** Tells the user what happened, in the window they use. */
function tell(notice: ExternalEditNotice): void {
  const toast = useToastStore.getState().show;
  const store = useExternalEditStore;
  switch (notice.kind) {
    case "uploaded":
      toast(
        "info",
        notice.renamedTo
          ? t("openWith.uploadedAs", { name: notice.name, renamedTo: notice.renamedTo })
          : t("openWith.uploadedName", { name: notice.name }),
      );
      return;
    case "uploadFailed":
      toast(
        "error",
        t("openWith.couldNotUploadKept", { name: notice.name, message: notice.message }),
      );
      return;
    case "conflict":
      if (!store.getState().resolving) store.setState({ resolving: notice.editId });
      return;
    case "saved": {
      const { askingUpload } = store.getState();
      if (!askingUpload.includes(notice.editId)) {
        store.setState({ askingUpload: [...askingUpload, notice.editId] });
      }
      return;
    }
    case "blocked": {
      const edit = store.getState().edits.find((candidate) => candidate.id === notice.editId);
      if (!edit) return;
      store.setState({
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
      return;
    }
    case "openFailed":
      toast("error", t("openWith.couldNotOpen", { name: notice.name, message: notice.message }));
      return;
    case "ended":
      toast("error", notice.message);
  }
}

/** The server's copy changed since the file was opened: overwrite it, keep both, or not yet. */
function ConflictPrompt({ edit }: { edit: ExternalEdit }) {
  const t = useT();
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const overSftp = useSessionStore(
    (state) => state.sessions[edit.sessionId]?.info.protocol === "sftp",
  );
  const [comparing, setComparing] = useState(false);
  const conflict = edit.conflict;
  const later = () => useExternalEditStore.setState({ resolving: null });
  const answer = (resolution: ExternalResolution) => {
    later();
    editAction(() => external.resolve(edit.id, resolution));
  };
  const deleted = conflict?.deleted ?? false;
  const text = looksLikeText(edit.name);

  return (
    <Dialog
      title={
        deleted
          ? t("openWith.isGoneFromTheServer", { name: edit.name })
          : t("openWith.changedOnTheServer", { name: edit.name })
      }
      onClose={later}
      width={540}
    >
      <div className="confirm-body">
        <p>
          {deleted
            ? t("openWith.someoneDeletedIt", { server: edit.server })
            : t("openWith.someoneChangedIt", { server: edit.server })}
        </p>
        {conflict && (
          <table className="conflict-table">
            <thead>
              <tr>
                <th />
                <th>{t("common.size")}</th>
                <th>{t("openWith.modified")}</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <th>{t("openWith.onTheServer")}</th>
                <td>{deleted ? t("openWith.deleted") : formatSize(conflict.serverSize ?? 0)}</td>
                <td>{deleted ? "" : formatDate(conflict.serverModified, dateFormat)}</td>
              </tr>
              <tr>
                <th>{t("openWith.yourSavedCopy")}</th>
                <td>{formatSize(conflict.localSize)}</td>
                <td>
                  {formatDate(
                    conflict.localModified === null ? null : conflict.localModified / 1000,
                    dateFormat,
                  )}
                </td>
              </tr>
            </tbody>
          </table>
        )}
        {!deleted && <p>{t("openWith.keepBothUploadsYourCopy")}</p>}
      </div>
      <div className="dialog-actions">
        {!deleted && text && (
          <button
            type="button"
            className="button dialog-action-start"
            disabled={!overSftp}
            title={overSftp ? undefined : t("openWith.comparingReadsTheServersCopy")}
            onClick={() => setComparing(true)}
          >
            {overSftp ? t("openWith.compare") : t("openWith.compareSftpOnly")}
          </button>
        )}
        <button type="button" className="button" onClick={() => answer("cancel")}>
          {t("openWith.dontUpload")}
        </button>
        {deleted ? (
          <button
            type="button"
            data-autofocus
            className="button button-primary"
            onClick={() => answer("overwrite")}
          >
            {t("openWith.uploadItAgain")}
          </button>
        ) : (
          <>
            <button type="button" className="button" onClick={() => answer("overwrite")}>
              {t("common.overwrite")}
            </button>
            {/* Enter loses neither copy. */}
            <button
              type="button"
              data-autofocus
              className="button button-primary"
              onClick={() => answer("keepBoth")}
            >
              {t("openWith.keepBoth")}
            </button>
          </>
        )}
      </div>
      {comparing && (
        <CompareDialog
          left={{
            location: { kind: "remote", sessionId: edit.sessionId },
            path: edit.remotePath,
            name: edit.name,
            where: t("openWith.serverChangedThere", { server: edit.server }),
          }}
          right={{
            location: { kind: "local" },
            path: edit.localPath,
            name: edit.name,
            where: t("openWith.yourSavedCopy"),
          }}
          onClose={() => setComparing(false)}
        />
      )}
    </Dialog>
  );
}

/** Uploads wait to be asked for, and the file was saved. */
function UploadPrompt({ edit }: { edit: ExternalEdit }) {
  const t = useT();
  const [always, setAlways] = useState(false);
  const done = () =>
    useExternalEditStore.setState(({ askingUpload }) => ({
      askingUpload: askingUpload.filter((id) => id !== edit.id),
    }));
  const upload = () => {
    done();
    if (always) void saveSettingsSection("editing", { uploadOnSave: true });
    editAction(() => external.upload(edit.id));
  };
  return (
    <Dialog title={t("openWith.upload", { name: edit.name })} onClose={done}>
      <div className="confirm-body">
        <p>
          {edit.program
            ? t("openWith.savedInProgramUploadNow", {
                name: edit.name,
                program: edit.program,
                server: edit.server,
              })
            : t("openWith.savedUploadNow", { name: edit.name, server: edit.server })}
        </p>
        <label className="check">
          <input
            type="checkbox"
            checked={always}
            onChange={(event) => setAlways(event.target.checked)}
          />
          {t("openWith.uploadEverySaveWithoutAsking")}
        </label>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={done}>
          {t("openWith.notNow")}
        </button>
        <button type="button" data-autofocus className="button button-primary" onClick={upload}>
          {t("common.upload")}
        </button>
      </div>
    </Dialog>
  );
}

interface OpenOption {
  key: string;
  icon: ReactNode;
  label: string;
  detail: string;
  choice: OpenChoice | "choose";
}

/** Which program opens a file: asked on double-click, and before an editor opens a program. */
function OpenQuestion({ prompt }: { prompt: OpenPrompt }) {
  const t = useT();
  const programs = useSettingsStore((state) => state.settings.editing.programs);
  const { target } = prompt;
  const executable = prompt.kind === "executable";
  const close = () => useExternalEditStore.setState({ prompt: null });
  const preferred = programFor(target.name, programs);
  const extension = target.name.includes(".")
    ? target.name.slice(target.name.lastIndexOf("."))
    : null;

  const options: OpenOption[] = [
    ...(executable
      ? []
      : [
          {
            key: "open",
            icon: <SquareArrowOutUpRight size={16} />,
            label: preferred ? programLabel(preferred) : t("openWith.defaultProgram"),
            detail: preferred
              ? t("openWith.setUpForFilesLike")
              : extension
                ? t("openWith.theProgramThisSystemOpensFilesWith", { extension })
                : t("openWith.theProgramThisSystemOpensSuchFilesWith"),
            choice: preferred
              ? ({ kind: "program", id: preferred.id } as const)
              : ({ kind: "default" } as const),
          },
        ]),
    ...programs
      .filter((program) => executable || program !== preferred)
      .map((program) => ({
        key: program.id,
        icon: <AppWindow size={16} />,
        label: programLabel(program),
        detail: program.command,
        choice: { kind: "program", id: program.id } as const,
      })),
    {
      key: "builtIn",
      icon: <FilePen size={16} />,
      label: t("openWith.builtInEditor"),
      detail: t("openWith.editTheTextInA"),
      choice: { kind: "builtIn" },
    },
    {
      key: "choose",
      icon: <FolderOpen size={16} />,
      label: t("openWith.chooseAProgram"),
      detail: t("openWith.anyProgramOnThisComputer"),
      choice: "choose",
    },
  ];

  const pick = (choice: OpenChoice | "choose") => {
    close();
    if (prompt.kind === "executable" && prompt.editId) {
      const editId = prompt.editId;
      if (choice !== "choose" && choice.kind === "builtIn") {
        // The built-in editor reads the server's file itself.
        editAction(() => external.stop(editId));
        void openFileWith(target, choice);
      } else {
        void openHeldBackWith(editId, choice);
      }
      return;
    }
    void openFileWith(target, choice, {
      confirmedExecutable: executable,
      nearGroupId: prompt.kind === "choose" ? prompt.nearGroupId : undefined,
    });
  };

  return (
    <Dialog
      title={
        executable
          ? t("openWith.isAProgram", { name: target.name })
          : t("openWith.open", { name: target.name })
      }
      onClose={close}
      width={460}
    >
      <div className="confirm-body">
        {executable && (
          <p>
            {prompt.editId
              ? t("openWith.holdsAProgramNotOpened", { name: target.name })
              : t("openWith.isAProgramOrAScript", { name: target.name })}
          </p>
        )}
        <div className="open-choices" role="group" aria-label={t("openWith.programs")}>
          {options.map((option, index) => (
            <button
              key={option.key}
              type="button"
              className="open-choice"
              data-autofocus={index === 0 ? true : undefined}
              onClick={() => pick(option.choice)}
            >
              {option.icon}
              <span className="open-choice-text">
                <span className="open-choice-label">{option.label}</span>
                <span className="open-choice-detail">{option.detail}</span>
              </span>
            </button>
          ))}
        </div>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={close}>
          {t("common.cancel")}
        </button>
      </div>
    </Dialog>
  );
}

/** Files an earlier run left saved but never uploaded, offered back as Poros starts. */
function LeftoversDialog() {
  const t = useT();
  const leftovers = useExternalEditStore((state) => state.leftovers);
  const [confirming, setConfirming] = useState<Confirmation | null>(null);
  const close = () => useExternalEditStore.setState({ showingLeftovers: false });
  return (
    <Dialog title={t("openWith.filesNotUploadedLastTime")} onClose={close} width={560}>
      <div className="confirm-body">
        <p>
          {t("openWith.whenPorosLastClosed", { count: leftovers.length })}
          {leftovers.some((leftover) => leftover.vaultRoot) &&
            ` ${t("openWith.copiesFromVaultsStayUnencrypted")}`}
        </p>
        <div className="edits-dialog-list">
          <LeftoverList leftovers={leftovers} onConfirm={setConfirming} />
        </div>
      </div>
      <div className="dialog-actions">
        <button type="button" data-autofocus className="button button-primary" onClick={close}>
          {t("openWith.later")}
        </button>
      </div>
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
    </Dialog>
  );
}

function EditNames({ edits }: { edits: ExternalEdit[] }) {
  return (
    <ul className="unsaved-files">
      {edits.map((edit) => (
        <li key={edit.id}>
          {edit.name} <span className="edit-names-state">{midSentence(editStateLabel(edit))}</span>
          {edit.detail && <span className="edit-names-state">: {edit.detail}</span>}
        </li>
      ))}
    </ul>
  );
}

/** A server tab closes, or a vault locks, while files from it are open in other programs. */
function CloseTabPrompt({ closing }: { closing: ClosePrompt }) {
  const t = useT();
  const allEdits = useExternalEditStore((state) => state.edits);
  const edits = useMemo(
    () => editsOfSessions(allEdits, closing.sessionIds),
    [allEdits, closing.sessionIds],
  );
  const [busy, setBusy] = useState(false);
  const [stuck, setStuck] = useState<ExternalEdit[] | null>(null);
  const locking = closing.kind === "lockVault";
  const inVault = edits.filter((edit) => edit.inVault);
  const cancel = () => answerClosing(false);
  const close = () => answerClosing(true);

  /** Uploads what waits, of the vault's files or all, and stops editing those when asked. */
  const settle = async (stopEditing: boolean) => {
    const settling = stopEditing ? edits : inVault;
    setBusy(true);
    const remaining =
      settling.length > 0
        ? await external.flush(settling.map((edit) => edit.id)).catch(() => settling)
        : [];
    if (stopEditing) {
      const clean = settling.filter((edit) => !remaining.some((left) => left.id === edit.id));
      await Promise.all(clean.map((edit) => external.stop(edit.id).catch(() => undefined)));
    }
    setBusy(false);
    if (remaining.length > 0) setStuck(remaining);
    else close();
  };

  return (
    <Dialog
      title={
        locking
          ? t("openWith.lock", { label: closing.label })
          : t("openWith.close", { label: closing.label })
      }
      onClose={cancel}
      width={500}
    >
      <div className="confirm-body">
        <p>
          {locking
            ? t("openWith.filesFromThisVaultOpen", { count: edits.length })
            : t("openWith.filesFromThisServerOpen", { count: edits.length })}
        </p>
        <EditNames edits={stuck ?? edits} />
        {stuck ? (
          <p className="warning-text">
            {locking
              ? t("openWith.couldNotBeUploadedLockingWipes", { count: stuck.length })
              : stuck.some((edit) => edit.inVault)
                ? t("openWith.couldNotBeUploadedClosingWipes", { count: stuck.length })
                : t("openWith.couldNotBeUploadedClosingLeaves", { count: stuck.length })}
          </p>
        ) : (
          <>
            {inVault.length < edits.length && <p>{t("openWith.theirSavesKeepBeingUploaded")}</p>}
            {inVault.length > 0 && (
              <p>
                {locking
                  ? t("openWith.lockingEndsEditing", { count: inVault.length })
                  : t("openWith.closingLocksVaultEndsEditing", { count: inVault.length })}
              </p>
            )}
          </>
        )}
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={cancel} disabled={busy}>
          {t("common.cancel")}
        </button>
        {stuck ? (
          <button type="button" className="button button-danger" onClick={close}>
            {locking ? t("openWith.lockAnyway") : t("openWith.closeAnyway")}
          </button>
        ) : (
          <>
            {!locking && (
              <button
                type="button"
                className="button"
                disabled={busy}
                onClick={() => void settle(true)}
              >
                {t("openWith.stopEditingAndClose")}
              </button>
            )}
            <button
              type="button"
              data-autofocus
              className="button button-primary"
              disabled={busy}
              onClick={() => void settle(false)}
            >
              {busy
                ? t("openWith.uploading2")
                : locking
                  ? t("openWith.uploadAndLock")
                  : inVault.length > 0
                    ? t("openWith.uploadAndClose")
                    : t("openWith.keepEditing")}
            </button>
          </>
        )}
      </div>
    </Dialog>
  );
}

/** Poros is quitting while files hold saves their servers do not have yet. */
function ExitPrompt({ editIds }: { editIds: string[] }) {
  const t = useT();
  const allEdits = useExternalEditStore((state) => state.edits);
  const unsaved = useMemo(
    () =>
      allEdits.filter(
        (edit) =>
          editIds.includes(edit.id) && edit.state !== "watching" && edit.state !== "downloading",
      ),
    [allEdits, editIds],
  );
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const cancel = () => useExternalEditStore.setState({ exiting: null });

  const uploadAndQuit = async () => {
    setBusy(true);
    const remaining = await external.flush(unsaved.map((edit) => edit.id)).catch(() => unsaved);
    setBusy(false);
    if (remaining.length === 0) exitNow();
    else setFailed(true);
  };

  const inVault = unsaved.some((edit) => edit.inVault);
  return (
    <Dialog title={t("openWith.uploadSavedFilesBeforeQuitting")} onClose={cancel} width={500}>
      <div className="confirm-body">
        <p>{t("openWith.savedButNotReachedTheServer", { count: unsaved.length })}</p>
        <EditNames edits={unsaved} />
        {failed && <p className="warning-text">{t("openWith.someCouldNotBeUploaded")}</p>}
        <p>
          {t("openWith.quittingWithoutUploadingKeepsThem")}
          {inVault && ` ${t("openWith.workingCopiesOfVaultFilesAreWiped")}`}
        </p>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={cancel} disabled={busy}>
          {t("common.cancel")}
        </button>
        <button type="button" className="button button-danger" onClick={exitNow}>
          {t("openWith.quitWithoutUploading")}
        </button>
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          disabled={busy}
          onClick={() => void uploadAndQuit()}
        >
          {busy
            ? t("openWith.uploading2")
            : failed
              ? t("common.tryAgain")
              : t("openWith.uploadAndQuit")}
        </button>
      </div>
    </Dialog>
  );
}
