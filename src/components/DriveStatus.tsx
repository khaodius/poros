import { useEffect, useRef, useState } from "react";
import { FolderOpen, HardDrive, RotateCw } from "lucide-react";
import { formatSize, pluralize } from "../lib/format";
import { local } from "../lib/ipc";
import { driveActivity, indicatorSummary, shortPath, uploadText } from "../lib/mounts";
import { baseName } from "../lib/path";
import type { DriveView, UnsentFile } from "../lib/types";
import {
  discardUnsent,
  openDrive,
  retryUploads,
  saveUnsentCopy,
  unmountDrive,
} from "../state/mountActions";
import { useMountStore, watchMounts } from "../state/mountStore";

/** The mounted drives in the status bar, with a popover to open and unmount each. */
export function DriveStatus() {
  const drives = useMountStore((state) => state.drives);
  const unsent = useMountStore((state) => state.unsent);
  const [open, setOpen] = useState(false);
  const [home, setHome] = useState<string | null>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const shown = drives.length > 0 || unsent.length > 0;

  useEffect(() => watchMounts(), []);

  useEffect(() => {
    void local
      .home()
      .then(setHome)
      .catch(() => undefined);
  }, []);

  // The popover goes with the last drive and the last kept file.
  if (open && !shown) setOpen(false);

  useEffect(() => {
    if (!open) return;
    popoverRef.current?.querySelector<HTMLElement>("button")?.focus();
    const dismiss = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!popoverRef.current?.contains(target) && !buttonRef.current?.contains(target)) {
        setOpen(false);
      }
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.stopPropagation();
      setOpen(false);
      buttonRef.current?.focus();
    };
    document.addEventListener("pointerdown", dismiss, true);
    document.addEventListener("keydown", escape, true);
    return () => {
      document.removeEventListener("pointerdown", dismiss, true);
      document.removeEventListener("keydown", escape, true);
    };
  }, [open]);

  if (!shown) return null;
  const summary =
    drives.length > 0
      ? indicatorSummary(drives)
      : { text: `${pluralize(unsent.length, "file")} not uploaded`, tone: "failed" as const };

  return (
    <span className="drive-status">
      <button
        ref={buttonRef}
        type="button"
        className={`status-item drive-indicator is-${summary.tone}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        title="Drives mounted from Poros"
        onClick={() => setOpen((wasOpen) => !wasOpen)}
      >
        <HardDrive size={12} />
        {summary.text}
      </button>
      {open && (
        <div ref={popoverRef} className="drive-popover" role="dialog" aria-label="Mounted drives">
          {drives.length > 0 && (
            <div className="drive-list">
              {drives.map((drive) => (
                <DriveCard key={drive.id} drive={drive} home={home} />
              ))}
            </div>
          )}
          {unsent.length > 0 && <KeptFiles files={unsent} />}
        </div>
      )}
    </span>
  );
}

function DriveCard({ drive, home }: { drive: DriveView; home: string | null }) {
  const failed = drive.uploads.some((upload) => upload.error);
  return (
    <section className="drive-card" aria-label={`${drive.label} at ${drive.mountPoint}`}>
      <div className="drive-card-head">
        <HardDrive size={14} className="drive-card-icon" />
        <span className="drive-card-label" title={drive.label}>
          {drive.label}
        </span>
        {drive.readOnly && <span className="drive-badge">Read-only</span>}
      </div>
      <dl className="drive-card-facts">
        <dt>At</dt>
        <dd title={drive.mountPoint}>{shortPath(drive.mountPoint, home)}</dd>
        <dt>Shows</dt>
        <dd title={drive.remoteRoot}>{drive.remoteRoot}</dd>
        <dt>Activity</dt>
        <dd>{driveActivity(drive)}</dd>
      </dl>
      {drive.uploads.length > 0 && (
        <ul className="drive-uploads" aria-label="Uploads">
          {drive.uploads.map((upload) => (
            <li key={upload.path} className={upload.error ? "is-failed" : ""}>
              <span className="drive-upload-name" title={upload.path}>
                {upload.path}
              </span>
              <span className="drive-upload-state" title={upload.error}>
                {uploadText(upload)}
              </span>
            </li>
          ))}
        </ul>
      )}
      <div className="drive-card-actions">
        <button
          type="button"
          className="button button-small"
          onClick={() => void openDrive(drive)}
        >
          <FolderOpen size={13} /> Open
        </button>
        {failed && (
          <button
            type="button"
            className="button button-small"
            onClick={() => void retryUploads(drive)}
          >
            <RotateCw size={13} /> Try again
          </button>
        )}
        <span className="drive-card-spacer" />
        <button
          type="button"
          className="button button-small"
          title="Refused while programs have files open on the drive or uploads are pending"
          onClick={() => void unmountDrive(drive, false)}
        >
          Unmount
        </button>
        <button
          type="button"
          className="button button-small drive-force"
          title="Unmounts at once: programs lose the files they have open, and what is not uploaded yet is kept on this computer"
          onClick={() => void unmountDrive(drive, true)}
        >
          Force unmount
        </button>
      </div>
    </section>
  );
}

/** Files whose drive went before they reached the server, kept on this computer. */
function KeptFiles({ files }: { files: UnsentFile[] }) {
  /** The file whose discarding waits for a second click. */
  const [confirming, setConfirming] = useState<string | null>(null);
  return (
    <section className="drive-kept" aria-label="Not uploaded">
      <h3>Not uploaded, kept on this computer</h3>
      <ul>
        {files.map((file) => (
          <li key={file.path}>
            <span className="drive-kept-text">
              <span className="drive-kept-name" title={file.remotePath}>
                {baseName(file.remotePath)}
              </span>
              <small title={file.error ?? file.remotePath}>
                {file.connection}, {formatSize(file.size)}
                {file.error ? `: ${file.error}` : ""}
              </small>
            </span>
            <button
              type="button"
              className="button button-small"
              onClick={() => void saveUnsentCopy(file)}
            >
              Save a copy...
            </button>
            {confirming === file.path ? (
              <button
                type="button"
                className="button button-small button-danger"
                onClick={() => void discardUnsent(file)}
                onBlur={() => setConfirming(null)}
                autoFocus
              >
                Discard for good
              </button>
            ) : (
              <button
                type="button"
                className="button button-small"
                onClick={() => setConfirming(file.path)}
              >
                Discard
              </button>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
