import { useEffect, useState, type FormEvent } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import { local, mounts } from "../lib/ipc";
import { connectionRoot, driveName, mountPointProblem, shortPath } from "../lib/mounts";
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
      <Dialog title="Mount as drive" onClose={onClose}>
        <p className="confirm-body">The connection has closed.</p>
        <div className="dialog-actions">
          <button type="button" className="button button-primary" onClick={onClose}>
            Close
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
      <Dialog title="Mount as drive" onClose={onClose}>
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
            Close
          </button>
        </div>
      </Dialog>
    );
  }

  const showProblem = touched && problem !== null;
  const rootLabel = info.vault ? "The vault's root" : "The server's root";
  return (
    <Dialog title={`Mount ${name} as a drive`} onClose={onClose} width={480}>
      <form onSubmit={submit} className="form">
        <label className="field">
          <span>{usesDriveLetters ? "Drive letter" : "Folder on this computer"}</span>
          <span className="input-with-button">
            <input
              data-autofocus
              value={mountPoint}
              spellCheck={false}
              placeholder={usesDriveLetters ? "P:" : "~/Poros/server"}
              className={showProblem ? "has-error" : ""}
              aria-invalid={showProblem}
              onChange={(event) => {
                setMountPoint(event.target.value);
                setTouched(true);
              }}
            />
            {!usesDriveLetters && (
              <button type="button" className="button" onClick={() => void browse()}>
                <FolderOpen size={14} /> Browse
              </button>
            )}
          </span>
          <small className={`field-hint ${showProblem ? "is-error" : ""}`}>
            {showProblem
              ? problem
              : usesDriveLetters
                ? "A free letter the drive is shown under."
                : "An empty folder, made if it is not there and removed again after."}
          </small>
        </label>

        {current ? (
          <div className="field">
            <span>Show on the drive</span>
            <div className="segmented" role="radiogroup" aria-label="Show on the drive">
              {(["current", "connection"] as const).map((choice) => (
                <button
                  key={choice}
                  type="button"
                  role="radio"
                  aria-checked={rootChoice === choice}
                  className={rootChoice === choice ? "is-selected" : ""}
                  onClick={() => setRootChoice(choice)}
                >
                  {choice === "current" ? "This folder" : rootLabel}
                </button>
              ))}
            </div>
            <small className="field-hint mount-root" title={remoteRoot}>
              {remoteRoot}
            </small>
          </div>
        ) : (
          <div className="field">
            <span>Show on the drive</span>
            <small className="field-hint mount-root" title={remoteRoot}>
              {rootLabel}: {remoteRoot}
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
            Read-only: programs can open files but not change them
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={openWhenMounted}
              onChange={(event) => setOpenWhenMounted(event.target.checked)}
            />
            Open in file manager when mounted
          </label>
        </div>

        <p className="field-hint">
          What programs save is uploaded when they close the file. The drive is unmounted when the
          tab closes{info.vault ? ", the vault locks" : ""} or Poros quits.
        </p>
        {lost && <p className="form-error">The connection is lost. Reconnect to mount it.</p>}
        {error && <p className="form-error">{error}</p>}
        <div className="dialog-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="button button-primary" disabled={!ready}>
            {busy ? "Mounting..." : "Mount"}
          </button>
        </div>
      </form>
    </Dialog>
  );
}
