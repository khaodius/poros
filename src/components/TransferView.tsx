import {
  memo,
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  ArrowDown,
  ArrowDownToLine,
  ArrowLeftRight,
  ArrowUp,
  ArrowUpToLine,
  ClipboardCopy,
  Folder,
  Pause,
  Play,
  RotateCw,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { useSelectionBox, type RowRange } from "../hooks/useSelectionBox";
import { formatSize, formatSpeed } from "../lib/format";
import { transfers } from "../lib/ipc";
import { jobProgress, jobsIn, VIEW_STATES, type TransferViewKind } from "../lib/transfers";
import type { Direction, JobSnapshot } from "../lib/types";
import { useTransferStore } from "../state/transferStore";
import { ContextMenu, type MenuItem } from "./ContextMenu";

const ROW_HEIGHT = 28;

const DIRECTION_ICONS: Record<Direction, LucideIcon> = {
  upload: ArrowUp,
  download: ArrowDown,
  relay: ArrowLeftRight,
};

const EMPTY_MESSAGES: Record<TransferViewKind, string> = {
  queue: "Drag files between tabs, or use Upload and Download in a file's menu.",
  failed: "Transfers that fail after their retries end up here.",
  completed: "Finished transfers are listed here.",
};

function statusText(job: JobSnapshot): string {
  switch (job.state) {
    case "queued":
      if (job.reconnecting) return "Connection lost, waiting for the server";
      return job.attempts > 0 ? `Waiting to retry (attempt ${job.attempts + 1})` : "Queued";
    case "running":
      if (job.kind === "folder") return "Listing folder";
      if (job.deltaBytes !== undefined) {
        return `Updating with rsync, ${formatSize(job.deltaBytes)} sent`;
      }
      if (job.direct) return "Copying directly between the servers";
      return job.connections > 1
        ? `Transferring on ${job.connections} connections`
        : "Transferring";
    case "paused":
      return "Paused";
    case "conflict":
      return "Already exists, waiting for your choice";
    case "done":
      if (job.direct) return "Done, copied directly between the servers";
      return job.deltaBytes === undefined
        ? "Done"
        : `Done with rsync, sent ${formatSize(job.deltaBytes)} of ${formatSize(job.size)}`;
    case "skipped":
      return job.error ? `Skipped: ${job.error}` : "Skipped";
    case "failed":
      return `Failed: ${job.error ?? "unknown error"}`;
  }
}

function useViewJobs(view: TransferViewKind): JobSnapshot[] {
  const snapshot = useTransferStore((state) => state.snapshot);
  return useMemo(() => jobsIn(snapshot, VIEW_STATES[view]), [snapshot, view]);
}

interface TransferViewProps {
  view: TransferViewKind;
  selection: ReadonlySet<number>;
  onSelectionChange: (selection: Set<number>) => void;
  /** Keeps the transfer that joined the list last in view, so each one is seen as it finishes. */
  follow?: boolean;
}

export function TransferView({
  view,
  selection,
  onSelectionChange,
  follow = false,
}: TransferViewProps) {
  const jobs = useViewJobs(view);
  const jobsRef = useRef(jobs);
  useLayoutEffect(() => {
    jobsRef.current = jobs;
  });
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const [anchor, setAnchor] = useState<number | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const virtualizer = useVirtualizer({
    count: jobs.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });
  const selectionBox = useSelectionBox(scrollRef, contentRef, {
    rowHeight: ROW_HEIGHT,
    rowCount: jobs.length,
    rowsInset: 0,
  });

  // The jobs listed when the list last followed, to tell the ones that have joined it since.
  const followedIds = useRef<ReadonlySet<number> | null>(null);
  useLayoutEffect(() => {
    if (!follow) {
      followedIds.current = null;
      return;
    }
    const before = followedIds.current;
    followedIds.current = new Set(jobs.map((job) => job.id));
    if (!before) {
      if (jobs.length > 0) virtualizer.scrollToIndex(jobs.length - 1, { align: "end" });
      return;
    }
    for (let index = jobs.length - 1; index >= 0; index--) {
      if (!before.has(jobs[index].id)) {
        virtualizer.scrollToIndex(index, { align: "auto" });
        return;
      }
    }
  }, [follow, jobs, virtualizer]);

  const select = (event: MouseEvent, job: JobSnapshot) => {
    if (event.ctrlKey || event.metaKey) {
      const next = new Set(selection);
      if (!next.delete(job.id)) next.add(job.id);
      onSelectionChange(next);
      setAnchor(job.id);
    } else if (event.shiftKey && anchor !== null) {
      const ids = jobs.map((entry) => entry.id);
      const [from, to] = [ids.indexOf(anchor), ids.indexOf(job.id)].sort((a, b) => a - b);
      onSelectionChange(new Set(from < 0 ? [job.id] : ids.slice(from, to + 1)));
    } else {
      onSelectionChange(new Set([job.id]));
      setAnchor(job.id);
    }
  };

  // Dragging from a row, or from below the rows, selects every row from the one pressed to the
  // one under the pointer, scrolling the list while the pointer is near or past its top or
  // bottom edge. With Ctrl or Shift held the rows join the selection.
  const startBox = (event: ReactPointerEvent<HTMLElement>) => {
    const before = selection;
    const kept = event.ctrlKey || event.metaKey || event.shiftKey ? [...before] : [];
    const idsIn = (rows: RowRange | null) =>
      rows ? jobsRef.current.slice(rows.first, rows.last + 1).map((job) => job.id) : [];
    selectionBox.begin(event, {
      onRows: (rows) => onSelectionChange(new Set([...kept, ...idsIn(rows)])),
      onFinish: (rows, downward) => {
        const ids = idsIn(rows);
        if (ids.length > 0) setAnchor(downward ? ids[0] : ids[ids.length - 1]);
      },
      onCancel: () => onSelectionChange(new Set(before)),
    });
  };

  const openMenu = (event: MouseEvent, job: JobSnapshot) => {
    event.preventDefault();
    if (!selection.has(job.id)) onSelectionChange(new Set([job.id]));
    setMenu({ x: event.clientX, y: event.clientY });
  };

  // Rows get stable handlers, so only rows whose job changed re-render on a progress tick.
  const latestHandlers = useRef({ select, openMenu });
  useLayoutEffect(() => {
    latestHandlers.current = { select, openMenu };
  });
  const handleRowClick = useCallback(
    (event: MouseEvent, job: JobSnapshot) => latestHandlers.current.select(event, job),
    [],
  );
  const handleRowMenu = useCallback(
    (event: MouseEvent, job: JobSnapshot) => latestHandlers.current.openMenu(event, job),
    [],
  );

  const selectedIds = jobs.filter((job) => selection.has(job.id)).map((job) => job.id);
  const menuItems = (): MenuItem[] => {
    const selectedJobs = jobs.filter((job) => selection.has(job.id));
    const paths = selectedJobs.map((job) => job.source).join("\n");
    const common: MenuItem[] = [
      {
        label: selectedJobs.length > 1 ? "Copy source paths" : "Copy source path",
        icon: <ClipboardCopy size={14} />,
        onSelect: () => void navigator.clipboard.writeText(paths),
      },
      "separator",
      {
        label: "Remove",
        icon: <Trash2 size={14} />,
        danger: true,
        onSelect: () => void transfers.remove(selectedIds),
      },
    ];
    if (view === "failed") {
      return [
        {
          label: "Retry",
          icon: <RotateCw size={14} />,
          onSelect: () => void transfers.resume(selectedIds),
        },
        ...common,
      ];
    }
    if (view === "completed") return common;
    const anyPaused = selectedJobs.some((job) => job.state === "paused");
    const anyActive = selectedJobs.some((job) => job.state !== "paused");
    return [
      {
        label: "Pause",
        icon: <Pause size={14} />,
        disabled: !anyActive,
        onSelect: () => void transfers.pause(selectedIds),
      },
      {
        label: "Resume",
        icon: <Play size={14} />,
        disabled: !anyPaused,
        onSelect: () => void transfers.resume(selectedIds),
      },
      "separator",
      {
        label: "Move to top",
        icon: <ArrowUpToLine size={14} />,
        onSelect: () => void transfers.move(selectedIds, true),
      },
      {
        label: "Move to bottom",
        icon: <ArrowDownToLine size={14} />,
        onSelect: () => void transfers.move(selectedIds, false),
      },
      "separator",
      ...common,
    ];
  };

  return (
    <div className="transfer-view">
      <div className="transfer-header" role="row">
        <span>Name</span>
        <span className="align-end">Size</span>
        <span>Progress</span>
        <span className="align-end">Speed</span>
        <span>Status</span>
      </div>
      <div
        ref={scrollRef}
        className="transfer-body"
        tabIndex={0}
        role="grid"
        aria-multiselectable
        onKeyDown={(event) => {
          if (event.key === "Delete" && selectedIds.length > 0) {
            void transfers.remove(selectedIds);
          } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a") {
            event.preventDefault();
            onSelectionChange(new Set(jobs.map((job) => job.id)));
          }
        }}
        onPointerDown={(event) => {
          // The scrollbar is right of the content.
          const body = event.currentTarget;
          if (event.clientX >= body.getBoundingClientRect().left + body.clientWidth) return;
          startBox(event);
        }}
        onClick={(event) => {
          if (event.target === event.currentTarget) onSelectionChange(new Set());
        }}
      >
        <div ref={contentRef} style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const job = jobs[item.index];
            return (
              <TransferRow
                key={job.id}
                job={job}
                top={item.start}
                selected={selection.has(job.id)}
                onClick={handleRowClick}
                onContextMenu={handleRowMenu}
              />
            );
          })}
          <div ref={selectionBox.boxRef} className="selection-box" aria-hidden />
        </div>
        {jobs.length === 0 && <p className="transfer-empty">{EMPTY_MESSAGES[view]}</p>}
      </div>
      {menu && selection.size > 0 && (
        <ContextMenu x={menu.x} y={menu.y} items={menuItems()} onClose={() => setMenu(null)} />
      )}
    </div>
  );
}

interface TransferRowProps {
  job: JobSnapshot;
  top: number;
  selected: boolean;
  onClick: (event: MouseEvent, job: JobSnapshot) => void;
  onContextMenu: (event: MouseEvent, job: JobSnapshot) => void;
}

const TransferRow = memo(function TransferRow({
  job,
  top,
  selected,
  onClick,
  onContextMenu,
}: TransferRowProps) {
  const progress = jobProgress(job);
  const DirectionIcon = DIRECTION_ICONS[job.direction];
  const status = statusText(job);
  return (
    <div
      className={`transfer-row state-${job.state} ${selected ? "is-selected" : ""}`}
      style={{ transform: `translateY(${top}px)`, height: ROW_HEIGHT }}
      role="row"
      aria-selected={selected}
      title={`${job.source}\nto ${job.target}`}
      onClick={(event) => onClick(event, job)}
      onContextMenu={(event) => onContextMenu(event, job)}
    >
      <span className="transfer-name">
        <DirectionIcon size={13} className={`transfer-direction direction-${job.direction}`} />
        {job.kind === "folder" && <Folder size={14} className="file-icon file-icon-folder" />}
        <span className="transfer-name-text">{job.name}</span>
        <span className="transfer-target">{job.targetDirectory}</span>
      </span>
      <span className="align-end">{job.kind === "file" ? formatSize(job.size) : ""}</span>
      <span className="transfer-progress">
        {job.kind === "file" && (
          <>
            <span className="progress-track">
              <span className="progress-fill" style={{ width: `${progress * 100}%` }} />
            </span>
            <span className="progress-text">{Math.floor(progress * 100)}%</span>
          </>
        )}
      </span>
      <span className="align-end">
        {job.state === "running" && job.kind === "file" ? formatSpeed(job.speed) : ""}
      </span>
      <span className="transfer-status" title={status}>
        {status}
      </span>
    </div>
  );
});
