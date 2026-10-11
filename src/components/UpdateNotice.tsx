import { AlertCircle, CircleArrowUp, CircleCheck, X } from "lucide-react";
import { formatSize, formatVersion } from "../lib/format";
import {
  hideUpdateNotice,
  percentDownloaded,
  restartToUpdate,
  startUpdate,
  useRestartWarnings,
  useUpdateStore,
  type InstallStage,
} from "../state/updateStore";

/** Shows how installing an update goes, in the window's bottom-right corner, then offers to restart. */
export function UpdateNotice() {
  const update = useUpdateStore((state) => state.update);
  const install = useUpdateStore((state) => state.install);
  const open = useUpdateStore((state) => state.noticeOpen);
  if (!update || !install || !open) return null;

  const version = formatVersion(update.version);
  const failed = install.stage === "failed";
  return (
    <section className={`update-notice ${failed ? "update-notice-failed" : ""}`}>
      <header className="update-notice-header">
        {failed ? (
          <AlertCircle size={16} />
        ) : install.stage === "ready" ? (
          <CircleCheck size={16} />
        ) : (
          <CircleArrowUp size={16} />
        )}
        <h3 className="update-notice-title" aria-live="polite">
          {title(install, version)}
        </h3>
        {install.stage !== "restarting" && (
          <button
            type="button"
            className="icon-button"
            aria-label={failed ? "Dismiss" : "Hide"}
            title={failed ? undefined : "Hide; the status bar shows it again"}
            onClick={hideUpdateNotice}
          >
            <X size={14} />
          </button>
        )}
      </header>
      <NoticeBody install={install} />
    </section>
  );
}

function title(install: InstallStage, version: string): string {
  switch (install.stage) {
    case "downloading":
      return `Downloading Poros ${version}`;
    case "installing":
      return `Installing Poros ${version}`;
    case "ready":
      return `Poros ${version} is ready`;
    case "restarting":
      return "Restarting Poros";
    case "failed":
      return "Could not update Poros";
  }
}

function NoticeBody({ install }: { install: InstallStage }) {
  switch (install.stage) {
    case "downloading": {
      const percent = percentDownloaded(install.received, install.total);
      return (
        <div className="update-notice-progress">
          <ProgressBar percent={percent} />
          <span className="update-notice-text">
            {install.total
              ? `${formatSize(install.received)} of ${formatSize(install.total)}`
              : formatSize(install.received)}
            {percent !== null && <span>{percent}%</span>}
          </span>
        </div>
      );
    }
    case "installing":
    case "restarting":
      return (
        <div className="update-notice-progress">
          <ProgressBar percent={null} />
        </div>
      );
    case "ready":
      return <ReadyBody />;
    case "failed":
      return (
        <>
          <p className="update-notice-message selectable">{install.message}</p>
          <div className="update-notice-actions">
            <button type="button" className="button" onClick={hideUpdateNotice}>
              Dismiss
            </button>
            <button
              type="button"
              className="button button-primary"
              onClick={() => void startUpdate()}
            >
              Try again
            </button>
          </div>
        </>
      );
  }
}

function ReadyBody() {
  const warnings = useRestartWarnings();
  return (
    <>
      <p className="update-notice-message">
        Restart Poros to finish updating, or the update takes effect the next time you open it.
      </p>
      {warnings.map((warning) => (
        <p key={warning} className="update-notice-message warning-text">
          {warning}
        </p>
      ))}
      <div className="update-notice-actions">
        <button type="button" className="button" onClick={hideUpdateNotice}>
          Later
        </button>
        <button
          type="button"
          className="button button-primary"
          onClick={() => void restartToUpdate()}
        >
          Restart now
        </button>
      </div>
    </>
  );
}

/** A null `percent` draws a bar that keeps moving, for work of unknown length. */
function ProgressBar({ percent }: { percent: number | null }) {
  return (
    <span
      className={`progress-track ${percent === null ? "is-indeterminate" : ""}`}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent ?? undefined}
    >
      <span
        className="progress-fill"
        style={percent === null ? undefined : { width: `${percent}%` }}
      />
    </span>
  );
}
