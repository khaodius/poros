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
import { formatDate, formatSize, pluralize } from "../lib/format";
import { external, onExternalEdits } from "../lib/ipc";
import type { ExternalEdit, ExternalEditNotice, ExternalResolution } from "../lib/types";
import {
  allowClose,
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
import { isMainWindow, requestCloseTab } from "../state/tabActions";
import { useToastStore } from "../state/toastStore";
import { CompareDialog } from "./CompareDialog";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./Dialog";
import { LeftoverList, type Confirmation } from "./ExternalEditsStatus";

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
          ? `Uploaded ${notice.name} as ${notice.renamedTo}`
          : `Uploaded ${notice.name}`,
      );
      return;
    case "uploadFailed":
      toast(
        "error",
        `Could not upload ${notice.name}: ${notice.message}. The saved file is kept; try again from the status bar.`,
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
      toast("error", `Could not open ${notice.name}: ${notice.message}`);
      return;
    case "ended":
      toast("error", notice.message);
  }
}

/** The server's copy changed since the file was opened: overwrite it, keep both, or not yet. */
function ConflictPrompt({ edit }: { edit: ExternalEdit }) {
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
        deleted ? `${edit.name} is gone from the server` : `${edit.name} changed on the server`
      }
      onClose={later}
      width={540}
    >
      <div className="confirm-body">
        <p>
          {deleted
            ? `Someone deleted it from ${edit.server} since you opened it.`
            : `Someone changed it on ${edit.server} since you opened it.`}{" "}
          Your save has not been uploaded yet.
        </p>
        {conflict && (
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
                <th>On the server</th>
                <td>{deleted ? "Deleted" : formatSize(conflict.serverSize ?? 0)}</td>
                <td>{deleted ? "" : formatDate(conflict.serverModified, dateFormat)}</td>
              </tr>
              <tr>
                <th>Your saved copy</th>
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
        {!deleted && (
          <p>Keep both uploads your copy beside the server's, under a name not taken there.</p>
        )}
      </div>
      <div className="dialog-actions">
        {!deleted && text && (
          <button
            type="button"
            className="button dialog-action-start"
            disabled={!overSftp}
            title={overSftp ? undefined : "Comparing reads the server's copy over SFTP"}
            onClick={() => setComparing(true)}
          >
            {overSftp ? "Compare" : "Compare (SFTP only)"}
          </button>
        )}
        <button type="button" className="button" onClick={() => answer("cancel")}>
          Don't upload
        </button>
        {!deleted && (
          <button type="button" className="button" onClick={() => answer("keepBoth")}>
            Keep both
          </button>
        )}
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          onClick={() => answer("overwrite")}
        >
          {deleted ? "Upload it again" : "Overwrite"}
        </button>
      </div>
      {comparing && (
        <CompareDialog
          left={{
            location: { kind: "remote", sessionId: edit.sessionId },
            path: edit.remotePath,
            name: edit.name,
            where: `${edit.server}, changed there`,
          }}
          right={{
            location: { kind: "local" },
            path: edit.localPath,
            name: edit.name,
            where: "Your saved copy",
          }}
          onClose={() => setComparing(false)}
        />
      )}
    </Dialog>
  );
}

/** Uploads wait to be asked for, and the file was saved. */
function UploadPrompt({ edit }: { edit: ExternalEdit }) {
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
    <Dialog title={`Upload ${edit.name}?`} onClose={done}>
      <div className="confirm-body">
        <p>
          {edit.name} was saved{edit.program ? ` in ${edit.program}` : ""}. Upload it to{" "}
          {edit.server} now?
        </p>
        <label className="check">
          <input
            type="checkbox"
            checked={always}
            onChange={(event) => setAlways(event.target.checked)}
          />
          Upload every save without asking
        </label>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={done}>
          Not now
        </button>
        <button type="button" data-autofocus className="button button-primary" onClick={upload}>
          Upload
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
            label: preferred ? programLabel(preferred) : "Default program",
            detail: preferred
              ? `Set up for files like this one`
              : `The program this system opens ${extension ?? "such"} files with`,
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
      label: "Built-in editor",
      detail: "Edit the text in a tab here",
      choice: { kind: "builtIn" },
    },
    {
      key: "choose",
      icon: <FolderOpen size={16} />,
      label: "Choose a program...",
      detail: "Any program on this computer",
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
      title={executable ? `${target.name} is a program` : `Open ${target.name}`}
      onClose={close}
      width={460}
    >
      <div className="confirm-body">
        {executable && (
          <p>
            {prompt.editId
              ? `${target.name} holds a program or a script, so it was not opened.`
              : `${target.name} is a program or a script.`}{" "}
            Poros does not run files it opens: pick an editor to look at it in.
          </p>
        )}
        <div className="open-choices" role="group" aria-label="Programs">
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
          Cancel
        </button>
      </div>
    </Dialog>
  );
}

/** Files an earlier run left saved but never uploaded, offered back as Poros starts. */
function LeftoversDialog() {
  const leftovers = useExternalEditStore((state) => state.leftovers);
  const [confirming, setConfirming] = useState<Confirmation | null>(null);
  const close = () => useExternalEditStore.setState({ showingLeftovers: false });
  return (
    <Dialog title="Files not uploaded last time" onClose={close} width={560}>
      <div className="confirm-body">
        <p>
          When Poros last closed, {leftovers.length === 1 ? "this file was" : "these files were"}{" "}
          saved in another program but not uploaded. Upload once its server is connected, or discard
          the changes. The status bar keeps the list.
        </p>
        <div className="edits-dialog-list">
          <LeftoverList leftovers={leftovers} onConfirm={setConfirming} />
        </div>
      </div>
      <div className="dialog-actions">
        <button type="button" data-autofocus className="button button-primary" onClick={close}>
          Later
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
          {edit.name} <span className="edit-names-state">{editStateLabel(edit).toLowerCase()}</span>
          {edit.detail && <span className="edit-names-state">: {edit.detail}</span>}
        </li>
      ))}
    </ul>
  );
}

/** A server tab closes while files from its server are open in other programs. */
function CloseTabPrompt({ closing }: { closing: ClosePrompt }) {
  const allEdits = useExternalEditStore((state) => state.edits);
  const edits = useMemo(
    () => editsOfSessions(allEdits, closing.sessionIds),
    [allEdits, closing.sessionIds],
  );
  const [busy, setBusy] = useState(false);
  const [stuck, setStuck] = useState<ExternalEdit[] | null>(null);
  const inVault = edits.filter((edit) => edit.inVault);
  const cancel = () => useExternalEditStore.setState({ closing: null });
  const close = () => {
    cancel();
    allowClose(closing.tabId);
    void requestCloseTab(closing.tabId);
  };

  /** Uploads what waits, of the vault's files or all, and stops editing those when asked. */
  const settle = async (stopEditing: boolean) => {
    const settling = stopEditing ? edits : inVault;
    setBusy(true);
    const remaining =
      settling.length > 0 ? await external.flush(settling.map((edit) => edit.id)) : [];
    if (stopEditing) {
      const clean = settling.filter((edit) => !remaining.some((left) => left.id === edit.id));
      await Promise.all(clean.map((edit) => external.stop(edit.id).catch(() => undefined)));
    }
    setBusy(false);
    if (remaining.length > 0) setStuck(remaining);
    else close();
  };

  return (
    <Dialog title={`Close ${closing.label}?`} onClose={cancel} width={500}>
      <div className="confirm-body">
        <p>
          {pluralize(edits.length, "file")} from this server {edits.length === 1 ? "is" : "are"}{" "}
          open in other programs:
        </p>
        <EditNames edits={stuck ?? edits} />
        {stuck ? (
          <p className="warning-text">
            {stuck.length === 1 ? "This one" : "These"} could not be uploaded.{" "}
            {stuck.some((edit) => edit.inVault)
              ? "Closing anyway wipes the changes in vault files."
              : "Closing anyway leaves them in the status bar's list, to upload later."}
          </p>
        ) : (
          <>
            {inVault.length < edits.length && (
              <p>Their saves keep being uploaded after the tab closes.</p>
            )}
            {inVault.length > 0 && (
              <p>
                Closing the tab locks its vault, which ends editing{" "}
                {inVault.length === 1 ? "the file" : "the files"} from it and wipes the working
                copies. What was saved is uploaded first.
              </p>
            )}
          </>
        )}
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={cancel} disabled={busy}>
          Cancel
        </button>
        {stuck ? (
          <button type="button" className="button button-danger" onClick={close}>
            Close anyway
          </button>
        ) : (
          <>
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() => void settle(true)}
            >
              Stop editing and close
            </button>
            <button
              type="button"
              data-autofocus
              className="button button-primary"
              disabled={busy}
              onClick={() => void settle(false)}
            >
              {busy ? "Uploading..." : inVault.length > 0 ? "Upload and close" : "Keep editing"}
            </button>
          </>
        )}
      </div>
    </Dialog>
  );
}

/** Poros is quitting while files hold saves their servers do not have yet. */
function ExitPrompt({ editIds }: { editIds: string[] }) {
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
    <Dialog title="Upload saved files before quitting?" onClose={cancel} width={500}>
      <div className="confirm-body">
        <p>
          {unsaved.length === 1 ? "This file was" : "These files were"} saved in other programs but{" "}
          {unsaved.length === 1 ? "has" : "have"} not reached the server:
        </p>
        <EditNames edits={unsaved} />
        {failed && <p className="warning-text">Some could not be uploaded.</p>}
        <p>
          Quitting without uploading keeps them, and Poros offers to upload them when it starts
          again.
          {inVault && " Working copies of vault files are wiped."}
        </p>
      </div>
      <div className="dialog-actions">
        <button type="button" className="button" onClick={cancel} disabled={busy}>
          Cancel
        </button>
        <button type="button" className="button button-danger" onClick={exitNow}>
          Quit without uploading
        </button>
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          disabled={busy}
          onClick={() => void uploadAndQuit()}
        >
          {busy ? "Uploading..." : failed ? "Try again" : "Upload and quit"}
        </button>
      </div>
    </Dialog>
  );
}
