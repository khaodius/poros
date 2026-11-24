import { useState, type ReactNode } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  ArrowDownToLine,
  ArrowUpToLine,
  Eraser,
  Pause,
  Play,
  RotateCw,
  Trash2,
} from "lucide-react";
import { transfers } from "../lib/ipc";
import { MAX_WORKERS } from "../lib/settings";
import { jobsIn, showsPaused, VIEW_STATES, type TransferViewKind } from "../lib/transfers";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { useTransferStore } from "../state/transferStore";
import { ScheduleOverrideButton } from "./LimitScheduleStatus";
import { LogView } from "./LogView";
import { Stepper } from "./Stepper";
import { TransferView } from "./TransferView";
import { WhenDoneControl } from "./WhenDoneControl";
import { useT } from "../i18n";

type PanelTab = TransferViewKind | "log";

const STORAGE_KEY = "poros.bottomTab";
const FOLLOW_KEY = "poros.followCompleted";
const PANEL_TABS: PanelTab[] = ["queue", "failed", "completed", "log"];

function readTab(): PanelTab {
  try {
    const stored = localStorage.getItem(STORAGE_KEY) as PanelTab | null;
    return stored && PANEL_TABS.includes(stored) ? stored : "queue";
  } catch {
    return "queue";
  }
}

function readFollow(): boolean {
  try {
    return localStorage.getItem(FOLLOW_KEY) === "true";
  } catch {
    return false;
  }
}

export function BottomPanel() {
  const t = useT();
  const [tab, setTab] = useState<PanelTab>(readTab);
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const [followCompleted, setFollowCompleted] = useState(readFollow);
  const counts = useTransferStore((state) => state.snapshot.stats.counts);
  // A queue that waits for Start while scheduled files go is shown going, so Pause can stop them.
  const queuePaused = useTransferStore((state) => showsPaused(state.snapshot.stats));
  // Only the rows on show: a job selected in the queue that has finished since is not.
  const selectedIds = useTransferStore(
    useShallow((state) =>
      tab === "log"
        ? []
        : jobsIn(state.snapshot, VIEW_STATES[tab])
            .filter((job) => selection.has(job.id))
            .map((job) => job.id),
    ),
  );

  const switchTab = (next: PanelTab) => {
    setTab(next);
    setSelection(new Set());
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // The panel then opens on the queue next time.
    }
  };

  const toggleFollow = () => {
    const next = !followCompleted;
    setFollowCompleted(next);
    try {
      localStorage.setItem(FOLLOW_KEY, String(next));
    } catch {
      // The list then opens without following next time.
    }
  };

  const queued = counts.queued + counts.running + counts.paused + counts.conflict;
  const labels: Record<PanelTab, ReactNode> = {
    queue: <TabLabel text={t("workspace.queue")} count={queued} />,
    failed: <TabLabel text={t("workspace.failed")} count={counts.failed} tone="danger" />,
    completed: <TabLabel text={t("workspace.completed")} count={counts.done + counts.skipped} />,
    log: "Log",
  };

  return (
    <section className="bottom-panel">
      <header className="bottom-panel-header">
        <div className="tabs" role="tablist">
          {PANEL_TABS.map((panelTab) => (
            <button
              key={panelTab}
              type="button"
              role="tab"
              aria-selected={tab === panelTab}
              className={`tab ${tab === panelTab ? "is-selected" : ""}`}
              onClick={() => switchTab(panelTab)}
            >
              {labels[panelTab]}
            </button>
          ))}
        </div>
        <WorkerControl />
        <WhenDoneControl />
        <div className="panel-toolbar">
          {tab === "queue" && (
            <>
              <button
                type="button"
                className={`button button-small ${queuePaused ? "button-primary" : ""}`}
                title={queuePaused ? t("workspace.startQueue") : t("workspace.pauseQueue")}
                onClick={() => void transfers.setPaused(!queuePaused)}
              >
                {queuePaused ? <Play size={13} /> : <Pause size={13} />}
                <span className="narrow-hidden">
                  {queuePaused ? t("workspace.startQueue") : t("workspace.pauseQueue")}
                </span>
              </button>
              <ScheduleOverrideButton />
              <span className="toolbar-divider" />
              <PanelButton
                label={t("workspace.pauseSelected")}
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.pause(selectedIds)}
              >
                <Pause size={14} />
              </PanelButton>
              <PanelButton
                label={t("workspace.resumeSelected")}
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.resume(selectedIds)}
              >
                <Play size={14} />
              </PanelButton>
              <PanelButton
                label={t("workspace.moveSelectedToTheTop")}
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.move(selectedIds, true)}
              >
                <ArrowUpToLine size={14} />
              </PanelButton>
              <PanelButton
                label={t("workspace.moveSelectedToTheBottom")}
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.move(selectedIds, false)}
              >
                <ArrowDownToLine size={14} />
              </PanelButton>
              <PanelButton
                label={t("workspace.removeSelected")}
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.remove(selectedIds)}
              >
                <Trash2 size={14} />
              </PanelButton>
            </>
          )}
          {tab === "failed" && (
            <>
              <button
                type="button"
                className="button button-small"
                disabled={counts.failed === 0}
                onClick={() =>
                  void transfers.resume(
                    jobsIn(useTransferStore.getState().snapshot, VIEW_STATES.failed).map(
                      (job) => job.id,
                    ),
                  )
                }
              >
                <RotateCw size={13} />
                {t("workspace.retryAll")}
              </button>
              <PanelButton
                label={t("workspace.retrySelected")}
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.resume(selectedIds)}
              >
                <RotateCw size={14} />
              </PanelButton>
              <PanelButton
                label={t("workspace.removeSelected")}
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.remove(selectedIds)}
              >
                <Trash2 size={14} />
              </PanelButton>
              <PanelButton
                label={t("workspace.clearFailedTransfers")}
                disabled={counts.failed === 0}
                onClick={() => void transfers.clear([...VIEW_STATES.failed])}
              >
                <Eraser size={14} />
              </PanelButton>
            </>
          )}
          {tab === "completed" && (
            <>
              <button
                type="button"
                className={`button button-small ${followCompleted ? "button-primary" : ""}`}
                title={t("workspace.keepEachTransferInView")}
                aria-pressed={followCompleted}
                onClick={toggleFollow}
              >
                <ArrowDownToLine size={13} />
                <span className="narrow-hidden">{t("workspace.follow")}</span>
              </button>
              <span className="toolbar-divider" />
              <PanelButton
                label={t("workspace.clearFinishedTransfers")}
                disabled={counts.done + counts.skipped === 0}
                onClick={() => void transfers.clear([...VIEW_STATES.completed])}
              >
                <Eraser size={14} />
              </PanelButton>
            </>
          )}
        </div>
      </header>
      {tab === "log" ? (
        <LogView />
      ) : (
        <TransferView
          view={tab}
          selection={selection}
          onSelectionChange={setSelection}
          follow={tab === "completed" && followCompleted}
        />
      )}
    </section>
  );
}

/** Changes how many transfers run at once; running workers above a lower count finish their
 * current file first. */
function WorkerControl() {
  const t = useT();
  const workers = useSettingsStore((state) => state.settings.transfers.workers);
  return (
    <div className="worker-control" title={t("workspace.simultaneousTransfers")}>
      <span className="narrow-hidden">{t("workspace.workers")}</span>
      <Stepper
        value={workers}
        min={1}
        max={MAX_WORKERS}
        label={t("workspace.workers")}
        onChange={(next) => void saveSettingsSection("transfers", { workers: next })}
      />
    </div>
  );
}

function TabLabel({ text, count, tone }: { text: string; count: number; tone?: "danger" }) {
  return (
    <>
      {text}
      {count > 0 && <span className={`tab-count ${tone ? `tab-count-${tone}` : ""}`}>{count}</span>}
    </>
  );
}

interface PanelButtonProps {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}

function PanelButton({ label, disabled, onClick, children }: PanelButtonProps) {
  return (
    <button
      type="button"
      className="icon-button"
      title={label}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
