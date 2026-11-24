import type { CSSProperties } from "react";
import { AlertCircle, CircleArrowUp, CircleCheck, X } from "lucide-react";
import { formatSize, formatVersion } from "../lib/format";
import { progressReach } from "../lib/progressBars";
import {
  hideUpdateNotice,
  percentDownloaded,
  restartToUpdate,
  startUpdate,
  useRestartWarnings,
  useUpdateStore,
  type InstallStage,
} from "../state/updateStore";
import { t, useT } from "../i18n";

/** Shows how installing an update goes, in the window's bottom-right corner, then offers to restart. */
export function UpdateNotice() {
  const t = useT();
  const target = useUpdateStore((state) => state.target);
  const install = useUpdateStore((state) => state.install);
  const open = useUpdateStore((state) => state.noticeOpen);
  if (!target || !install || !open) return null;

  const version = formatVersion(target.version);
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
            aria-label={failed ? t("common.dismiss") : t("common.hide")}
            title={failed ? undefined : t("updates.hideTheStatusBarShows")}
            onClick={hideUpdateNotice}
          >
            <X size={14} />
          </button>
        )}
      </header>
      <NoticeBody install={install} onRetry={() => void startUpdate(target)} />
    </section>
  );
}

function title(install: InstallStage, version: string): string {
  switch (install.stage) {
    case "downloading":
      return t("updates.downloadingPoros", { version });
    case "installing":
      return t("updates.installingPoros", { version });
    case "ready":
      return t("updates.porosIsReady", { version });
    case "restarting":
      return t("updates.restartingPoros");
    case "failed":
      return t("updates.couldNotInstallPoros", { version });
  }
}

function NoticeBody({ install, onRetry }: { install: InstallStage; onRetry: () => void }) {
  const t = useT();
  switch (install.stage) {
    case "downloading": {
      const percent = percentDownloaded(install.received, install.total);
      return (
        <div className="update-notice-progress">
          <ProgressBar percent={percent} />
          <span className="update-notice-text">
            {install.total
              ? t("updates.of", {
                  received: formatSize(install.received),
                  total: formatSize(install.total),
                })
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
              {t("common.dismiss")}
            </button>
            <button type="button" className="button button-primary" onClick={onRetry}>
              {t("common.tryAgain")}
            </button>
          </div>
        </>
      );
  }
}

function ReadyBody() {
  const t = useT();
  const warnings = useRestartWarnings();
  return (
    <>
      <p className="update-notice-message">{t("updates.restartPorosToSwitchTo")}</p>
      {warnings.map((warning) => (
        <p key={warning} className="update-notice-message warning-text">
          {warning}
        </p>
      ))}
      <div className="update-notice-actions">
        <button type="button" className="button" onClick={hideUpdateNotice}>
          {t("updates.later")}
        </button>
        <button
          type="button"
          className="button button-primary"
          onClick={() => void restartToUpdate()}
        >
          {t("updates.restartNow")}
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
      style={percent === null ? undefined : (progressReach(percent / 100) as CSSProperties)}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent ?? undefined}
    >
      <span className="progress-fill" />
    </span>
  );
}
