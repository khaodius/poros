import { useState, type ReactNode } from "react";
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
import { jobsIn, VIEW_STATES, type TransferViewKind } from "../lib/transfers";
import { useTransferStore } from "../state/transferStore";
import { LogView } from "./LogView";
import { TransferView } from "./TransferView";

type PanelTab = TransferViewKind | "log";

const STORAGE_KEY = "poros.bottomTab";
const PANEL_TABS: PanelTab[] = ["queue", "failed", "completed", "log"];

function readTab(): PanelTab {
  try {
    const stored = localStorage.getItem(STORAGE_KEY) as PanelTab | null;
    return stored && PANEL_TABS.includes(stored) ? stored : "queue";
  } catch {
    return "queue";
  }
}

export function BottomPanel() {
  const [tab, setTab] = useState<PanelTab>(readTab);
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const counts = useTransferStore((state) => state.snapshot.stats.counts);
  const queuePaused = useTransferStore((state) => state.snapshot.stats.queuePaused);
  const selectedIds = [...selection];

  const switchTab = (next: PanelTab) => {
    setTab(next);
    setSelection(new Set());
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // The panel then opens on the queue next time.
    }
  };

  const queued = counts.queued + counts.running + counts.paused + counts.conflict;
  const labels: Record<PanelTab, ReactNode> = {
    queue: <TabLabel text="Queue" count={queued} />,
    failed: <TabLabel text="Failed" count={counts.failed} tone="danger" />,
    completed: <TabLabel text="Completed" count={counts.done + counts.skipped} />,
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
        <div className="panel-toolbar">
          {tab === "queue" && (
            <>
              <button
                type="button"
                className={`button button-small ${queuePaused ? "button-primary" : ""}`}
                onClick={() => void transfers.setPaused(!queuePaused)}
              >
                {queuePaused ? <Play size={13} /> : <Pause size={13} />}
                {queuePaused ? "Start queue" : "Pause queue"}
              </button>
              <span className="toolbar-divider" />
              <PanelButton
                label="Pause selected"
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.pause(selectedIds)}
              >
                <Pause size={14} />
              </PanelButton>
              <PanelButton
                label="Resume selected"
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.resume(selectedIds)}
              >
                <Play size={14} />
              </PanelButton>
              <PanelButton
                label="Move selected to the top"
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.move(selectedIds, true)}
              >
                <ArrowUpToLine size={14} />
              </PanelButton>
              <PanelButton
                label="Move selected to the bottom"
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.move(selectedIds, false)}
              >
                <ArrowDownToLine size={14} />
              </PanelButton>
              <PanelButton
                label="Remove selected"
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
                Retry all
              </button>
              <PanelButton
                label="Retry selected"
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.resume(selectedIds)}
              >
                <RotateCw size={14} />
              </PanelButton>
              <PanelButton
                label="Remove selected"
                disabled={selectedIds.length === 0}
                onClick={() => void transfers.remove(selectedIds)}
              >
                <Trash2 size={14} />
              </PanelButton>
              <PanelButton
                label="Clear failed transfers"
                disabled={counts.failed === 0}
                onClick={() => void transfers.clear([...VIEW_STATES.failed])}
              >
                <Eraser size={14} />
              </PanelButton>
            </>
          )}
          {tab === "completed" && (
            <PanelButton
              label="Clear finished transfers"
              disabled={counts.done + counts.skipped === 0}
              onClick={() => void transfers.clear([...VIEW_STATES.completed])}
            >
              <Eraser size={14} />
            </PanelButton>
          )}
        </div>
      </header>
      {tab === "log" ? (
        <LogView />
      ) : (
        <TransferView view={tab} selection={selection} onSelectionChange={setSelection} />
      )}
    </section>
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
