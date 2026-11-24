import { useEffect, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import { local, mounts } from "../lib/ipc";
import { useT } from "../i18n";
import {
  connectionRoot,
  DRIVE_LETTER_EXAMPLE,
  driveName,
  FOLDER_EXAMPLE,
  mountPointProblem,
  shortPath,
} from "../lib/mounts";
import type { MountSupport } from "../lib/types";
import { isLost } from "../lib/vaults";
import { mountDrive } from "../state/mountActions";
import { useMountStore } from "../state/mountStore";
import { useSessionStore } from "../state/sessionStore";
import { Dialog } from "./Dialog";

interface MountDialogProps {
  sessionId: string;
  /** The folder the pane shows, offered as the drive's root. */
  path: string | null;
  onClose: () => void;
}

type RootChoice = "current" | "connection";

/** Asks where to show a server, or an unlocked vault, as a drive on this computer. */
export function MountDialog({ sessionId, path, onClose }: MountDialogProps) {
  const t = useT();
  const info = useSessionStore((state) => state.sessions[sessionId]?.info);
  const lost = useSessionStore((state) => isLost(state.sessions, sessionId));
  const [support, setSupport] = useState<MountSupport | null>(
    () => useMountStore.getState().support,
  );
  const [home, setHome] = useState<string | null>(null);
  const [mountPoint, setMountPoint] = useState("");
  const [rootChoice, setRootChoice] = useState<RootChoice>("current");
  const [readOnly, setReadOnly] = useState(false);
  const [openWhenMounted, setOpenWhenMounted] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [touched, setTouched] = useState(false);

  const name = info ? driveName(info) : "";
  useEffect(() => {
    let current = true;
    void useMountStore
      .getState()
      .loadSupport()
      .then((found) => current && setSupport(found))
      .catch((caught: { message?: string }) => current && setError(caught.message ?? null));
    void Promise.all([local.home().catch(() => null), mounts.defaultPoint(name).catch(() => "")])
      .then(([homeFolder, suggested]) => {
        if (!current) return;
        setHome(homeFolder);
        // Kept if the user typed meanwhile.
        setMountPoint((typed) => typed || shortPath(suggested, homeFolder));
      })
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [name]);

  if (!info) {
    return (
      <Dialog title={t("drives.mountAsDrive")} onClose={onClose}>
        <p className="confirm-body">{t("drives.theConnectionHasClosed")}</p>
        <div className="dialog-actions">
          <button type="button" className="button button-primary" onClick={onClose}>
            {t("common.close")}
          </button>
        </div>
      </Dialog>
    );
  }

  const usesDriveLetters = support?.usesDriveLetters ?? false;
  const root = connectionRoot(info);
  const current = path && path !== root ? path : null;
  const remoteRoot = current && rootChoice === "current" ? current : root;
  const problem = mountPointProblem(mountPoint, usesDriveLetters);
  const missing = support?.missing;
  const ready = support !== null && !missing && !lost && problem === null && !busy;

  const browse = async () => {
    const chosen = await openFileDialog({ directory: true, multiple: false });
    if (typeof chosen === "string") {
      setMountPoint(shortPath(chosen, home));
      setTouched(true);
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setTouched(true);
    if (!ready) return;
    setBusy(true);
    setError(null);
    try {
      await mountDrive(
        { sessionId, mountPoint: mountPoint.trim(), remoteRoot, readOnly },
        openWhenMounted,
      );
      onClose();
    } catch (caught) {
      setError((caught as { message?: string }).message ?? String(caught));
      setBusy(false);
    }
  };

  if (missing) {
    return (
      <Dialog title={t("drives.mountAsDrive")} onClose={onClose}>
        <div className="confirm-body mount-missing">
          <p>{missing.problem}</p>
          <p>{missing.help}</p>
          {missing.link && (
            <p>
              <a href={missing.link} target="_blank" rel="noreferrer">
                {missing.link}
              </a>
            </p>
          )}
        </div>
        <div className="dialog-actions">
          <button type="button" data-autofocus className="button button-primary" onClick={onClose}>
            {t("common.close")}
          </button>
        </div>
      </Dialog>
    );
  }

  const showProblem = touched && problem !== null;
  const rootLabel = info.vault ? t("drives.theVaultsRoot") : t("drives.theServersRoot");
  return (
    <Dialog title={t("drives.mountNameAsADrive", { name })} onClose={onClose} width={480}>
      <form onSubmit={submit} className="form">
        <label className="field">
          <span>
            {usesDriveLetters ? t("drives.driveLetter") : t("drives.folderOnThisComputer")}
          </span>
          <span className="input-with-button">
            <input
              data-autofocus
              value={mountPoint}
              spellCheck={false}
              placeholder={usesDriveLetters ? DRIVE_LETTER_EXAMPLE : FOLDER_EXAMPLE}
              className={showProblem ? "has-error" : ""}
              aria-invalid={showProblem}
              onChange={(event) => {
                setMountPoint(event.target.value);
                setTouched(true);
              }}
            />
            {!usesDriveLetters && (
              <button type="button" className="button" onClick={() => void browse()}>
                <FolderOpen size={14} /> {t("common.browse")}
              </button>
            )}
          </span>
          <small className={`field-hint ${showProblem ? "is-error" : ""}`}>
            {showProblem
              ? problem
              : usesDriveLetters
                ? t("drives.aFreeLetterTheDrive")
                : t("drives.anEmptyFolderMadeIf")}
          </small>
        </label>

        {current ? (
          <div className="field">
            <span>{t("drives.showOnTheDrive")}</span>
            <div className="segmented" role="radiogroup" aria-label={t("drives.showOnTheDrive")}>
              {(["current", "connection"] as const).map((choice) => (
                <button
                  key={choice}
                  type="button"
                  role="radio"
                  aria-checked={rootChoice === choice}
                  className={rootChoice === choice ? "is-selected" : ""}
                  onClick={() => setRootChoice(choice)}
                >
                  {choice === "current" ? t("drives.thisFolder") : rootLabel}
                </button>
              ))}
            </div>
            <small className="field-hint mount-root" title={remoteRoot}>
              {remoteRoot}
            </small>
          </div>
        ) : (
          <div className="field">
            <span>{t("drives.showOnTheDrive")}</span>
            <small className="field-hint mount-root" title={remoteRoot}>
              {t("drives.rootAt", { root: rootLabel, path: remoteRoot })}
            </small>
          </div>
        )}

        <div className="checks">
          <label className="check">
            <input
              type="checkbox"
              checked={readOnly}
              onChange={(event) => setReadOnly(event.target.checked)}
            />
            {t("drives.readOnlyProgramsCanOpen")}
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={openWhenMounted}
              onChange={(event) => setOpenWhenMounted(event.target.checked)}
            />
            {t("drives.openInFileManagerWhen")}
          </label>
        </div>

        <p className="field-hint">
          {info.vault ? t("drives.whatProgramsSaveVault") : t("drives.whatProgramsSaveServer")}
        </p>
        {lost && <p className="form-error">{t("drives.theConnectionIsLostReconnect")}</p>}
        {error && <p className="form-error">{error}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            {t("common.cancel")}
          </button>
          <button type="submit" className="button button-primary" disabled={!ready}>
            {busy ? t("drives.mounting") : t("drives.mount")}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
