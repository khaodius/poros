import { Fragment, useMemo } from "react";
import { formatSize, formatVersion, pluralize } from "../lib/format";
import { parseReleaseNotes, type NoteSpan } from "../lib/releaseNotes";
import { useUnsavedStore } from "../state/editorRegistry";
import { useSettingsStore } from "../state/settingsStore";
import { useTransferStore } from "../state/transferStore";
import {
  installUpdate,
  skipUpdate,
  useUpdateStore,
  type InstallProgress,
} from "../state/updateStore";
import { Dialog } from "./Dialog";

export function UpdateDialog({ onClose }: { onClose: () => void }) {
  const update = useUpdateStore((state) => state.update);
  const progress = useUpdateStore((state) => state.progress);
  const installError = useUpdateStore((state) => state.installError);
  const pendingTransfers = useTransferStore(
    ({ snapshot }) => snapshot.stats.counts.queued + snapshot.stats.counts.running,
  );
  const keepQueue = useSettingsStore((state) => state.settings.transfers.keepQueue);
  const unsavedFiles = useUnsavedStore((state) => Object.keys(state.tabs).length);
  const notes = useMemo(() => parseReleaseNotes(update?.body ?? ""), [update]);
  if (!update) return null;

  const busy = progress !== null;
  const skip = async () => {
    await skipUpdate();
    onClose();
  };

  return (
    <Dialog title="Update available" onClose={busy ? () => undefined : onClose} width={560}>
      <div className="confirm-body">
        <p>
          Poros <strong>{formatVersion(update.version)}</strong> is available. You have{" "}
          {formatVersion(update.currentVersion)}.
        </p>
        {notes.length > 0 && (
          <div className="release-notes selectable">
            {notes.map((block, index) => {
              switch (block.kind) {
                case "heading":
                  return (
                    <h4 key={index}>
                      <Spans spans={block.spans} />
                    </h4>
                  );
                case "paragraph":
                  return (
                    <p key={index}>
                      <Spans spans={block.spans} />
                    </p>
                  );
                case "list":
                  return (
                    <ul key={index}>
                      {block.items.map((item, itemIndex) => (
                        <li key={itemIndex}>
                          <Spans spans={item} />
                        </li>
                      ))}
                    </ul>
                  );
              }
            })}
          </div>
        )}
        {pendingTransfers > 0 && (
          <p className="warning-text">
            {pluralize(pendingTransfers, "transfer")} in the queue will stop when Poros restarts
            {keepQueue ? " and come back paused." : "."}
          </p>
        )}
        {unsavedFiles > 0 && (
          <p className="warning-text">
            Unsaved changes in {pluralize(unsavedFiles, "file")} will be lost when Poros restarts.
          </p>
        )}
      </div>
      {progress && <InstallStatus progress={progress} />}
      {installError && (
        <div className="form-error update-error">Could not install the update: {installError}</div>
      )}
      <div className="dialog-actions">
        <button
          type="button"
          className="button dialog-action-start"
          disabled={busy}
          onClick={() => void skip()}
        >
          Skip this version
        </button>
        <button type="button" className="button" disabled={busy} onClick={onClose}>
          Later
        </button>
        <button
          type="button"
          data-autofocus
          className="button button-primary"
          disabled={busy}
          onClick={() => void installUpdate()}
        >
          {busy ? "Installing..." : "Install and restart"}
        </button>
      </div>
    </Dialog>
  );
}

function Spans({ spans }: { spans: NoteSpan[] }) {
  return spans.map((span, index) => (
    <Fragment key={index}>
      {span.kind === "strong" ? (
        <strong>{span.text}</strong>
      ) : span.kind === "code" ? (
        <code>{span.text}</code>
      ) : (
        span.text
      )}
    </Fragment>
  ));
}

function InstallStatus({ progress }: { progress: InstallProgress }) {
  const { received, total, installing } = progress;
  const fraction = installing ? 1 : total ? Math.min(1, received / total) : 0;
  const text = installing
    ? "Installing"
    : total
      ? `Downloading ${formatSize(received)} of ${formatSize(total)}`
      : `Downloading ${formatSize(received)}`;
  return (
    <div className="update-progress" role="status">
      <span className="update-progress-text">{text}</span>
      <span className="progress-track">
        <span className="progress-fill" style={{ width: `${fraction * 100}%` }} />
      </span>
    </div>
  );
}
