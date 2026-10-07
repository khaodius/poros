import {
  memo,
  useCallback,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  ArrowDown,
  ArrowLeft,
  ArrowLeftRight,
  ArrowRight,
  ArrowUp,
  CircleAlert,
  Trash2,
} from "lucide-react";
import { formatDate, formatSize, pluralize } from "../lib/format";
import type { DateFormat } from "../lib/settings";
import {
  ACTION_LABELS,
  chosenTotals,
  describeCounts,
  describePlanned,
  describeReason,
  matchesFilter,
  possibleActions,
  type Choices,
  type ItemFilter,
} from "../lib/sync";
import type { SyncAction, SyncDirection, SyncFacts, SyncItem, SyncPlanView } from "../lib/types";
import { useSettingsStore } from "../state/settingsStore";
import { FileIcon } from "./FileIcon";

const ROW_HEIGHT = 28;

const FILTERS: { value: ItemFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "upload", label: "Upload" },
  { value: "download", label: "Download" },
  { value: "delete", label: "Delete" },
  { value: "conflict", label: "Conflicts" },
];

interface SyncPlanListProps {
  plan: SyncPlanView;
  direction: SyncDirection;
  choices: Choices;
  running: boolean;
  error: string | null;
  onChoices: Dispatch<SetStateAction<Choices>>;
  onBack: () => void;
  onCancel: () => void;
  onRun: () => void;
}

/** What a comparison found, with a choice for each item. */
export function SyncPlanList({
  plan,
  direction,
  choices,
  running,
  error,
  onChoices,
  onBack,
  onCancel,
  onRun,
}: SyncPlanListProps) {
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const [filter, setFilter] = useState<ItemFilter>("all");
  const scrollRef = useRef<HTMLDivElement>(null);
  const shown = useMemo(
    () => plan.items.filter((item) => matchesFilter(item, filter)),
    [plan.items, filter],
  );
  const virtualizer = useVirtualizer({
    count: shown.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });
  const totals = useMemo(() => chosenTotals(plan.items, choices), [plan.items, choices]);
  const deleting = totals.delete.items > 0;
  const nothingChosen = totals.upload.items + totals.download.items + totals.delete.items === 0;

  const toggleable = shown.filter((item) => item.action !== "conflict");
  const chosenCount = toggleable.filter((item) => choices.get(item.id)).length;
  const choose = useCallback(
    (id: number, action: SyncAction | null) =>
      onChoices((current) => new Map(current).set(id, action)),
    [onChoices],
  );
  const chooseAll = (on: boolean) =>
    onChoices((current) => {
      const next = new Map(current);
      for (const item of toggleable) next.set(item.id, on ? item.action : null);
      return next;
    });

  const filterCounts = useMemo(() => {
    const counts = new Map<ItemFilter, number>();
    for (const { value } of FILTERS) {
      counts.set(value, plan.items.filter((item) => matchesFilter(item, value)).length);
    }
    return counts;
  }, [plan.items]);
  const counts = describeCounts(plan.counts, direction);
  const DirectionIcon =
    direction === "upload" ? ArrowRight : direction === "download" ? ArrowLeft : ArrowLeftRight;

  return (
    <div className="sync-preview">
      <p className="sync-roots selectable">
        <span className="mono" title={plan.localRoot}>
          {plan.localRoot}
        </span>
        <DirectionIcon size={14} aria-label={direction} />
        <span className="mono" title={plan.remoteRoot}>
          {plan.remoteRoot}
        </span>
      </p>
      {plan.items.length === 0 ? (
        <div className="sync-list sync-empty">
          <p className="sync-empty-title">Everything is in sync.</p>
          {counts && <p>{counts}.</p>}
        </div>
      ) : (
        <>
          <div className="sync-toolbar">
            <div className="toggle-group" role="tablist">
              {FILTERS.filter(
                ({ value }) => value === "all" || (filterCounts.get(value) ?? 0) > 0,
              ).map(({ value, label }) => (
                <button
                  key={value}
                  type="button"
                  role="tab"
                  aria-selected={filter === value}
                  className={`toggle ${filter === value ? "is-on" : ""}`}
                  onClick={() => setFilter(value)}
                >
                  {label} {filterCounts.get(value)}
                </button>
              ))}
            </div>
            {counts && <span className="sync-counts">{counts}</span>}
          </div>
          <div className="sync-list">
            <div className="sync-header" role="row">
              <span className="sync-check">
                <input
                  type="checkbox"
                  aria-label="Include all"
                  checked={toggleable.length > 0 && chosenCount === toggleable.length}
                  ref={(input) => {
                    if (input) {
                      input.indeterminate = chosenCount > 0 && chosenCount < toggleable.length;
                    }
                  }}
                  disabled={toggleable.length === 0 || running}
                  onChange={(event) => chooseAll(event.target.checked)}
                />
              </span>
              <span>Action</span>
              <span>Path</span>
              <span className="align-end">Size</span>
              <span>Why</span>
            </div>
            <div ref={scrollRef} className="sync-body" role="grid">
              <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
                {virtualizer.getVirtualItems().map((row) => {
                  const item = shown[row.index];
                  return (
                    <PlanRow
                      key={item.id}
                      item={item}
                      choice={choices.get(item.id) ?? null}
                      top={row.start}
                      disabled={running}
                      dateFormat={dateFormat}
                      onChoose={choose}
                    />
                  );
                })}
              </div>
            </div>
          </div>
        </>
      )}
      {plan.passedOver.length > 0 && (
        <details className="sync-passed-over">
          <summary>
            {pluralize(plan.counts.passedOver, "entry", "entries")} passed over: broken or looping
            links, special files and folders that could not be read
          </summary>
          <ul className="delete-preview selectable">
            {plan.passedOver.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        </details>
      )}
      {error && <p className="form-error">{error}</p>}
      <div className="dialog-actions">
        <button type="button" className="button" disabled={running} onClick={onBack}>
          Back
        </button>
        <span className="sync-planned">{plan.items.length > 0 ? describePlanned(totals) : ""}</span>
        <button type="button" className="button" onClick={onCancel}>
          {plan.items.length === 0 ? "Close" : "Cancel"}
        </button>
        {plan.items.length > 0 && (
          <button
            type="button"
            className={`button ${deleting ? "button-danger" : "button-primary"}`}
            disabled={nothingChosen || running}
            onClick={onRun}
          >
            {running ? "Starting..." : "Synchronize"}
          </button>
        )}
      </div>
    </div>
  );
}

function sideDetail(label: string, facts: SyncFacts | undefined, dateFormat: DateFormat): string {
  if (!facts) return `${label}: missing`;
  if (facts.isDir) return `${label}: folder`;
  return `${label}: ${formatSize(facts.size)}, ${formatDate(facts.modified, dateFormat)}`;
}

const ACTION_ICONS: Partial<Record<SyncAction, typeof ArrowUp>> = {
  upload: ArrowUp,
  download: ArrowDown,
  deleteLocal: Trash2,
  deleteRemote: Trash2,
};

interface PlanRowProps {
  item: SyncItem;
  choice: SyncAction | null;
  top: number;
  disabled: boolean;
  dateFormat: DateFormat;
  onChoose: (id: number, action: SyncAction | null) => void;
}

const PlanRow = memo(function PlanRow({
  item,
  choice,
  top,
  disabled,
  dateFormat,
  onChoose,
}: PlanRowProps) {
  const conflict = item.action === "conflict";
  const settles = possibleActions(item);
  const shownAction = choice ?? item.action;
  const Icon = ACTION_ICONS[shownAction];
  const name = item.path.slice(item.path.lastIndexOf("/") + 1);
  const size = conflict
    ? choice === "upload"
      ? item.local?.size
      : choice === "download"
        ? item.remote?.size
        : undefined
    : item.bytes;
  const detail = [
    item.path,
    sideDetail("Local", item.local, dateFormat),
    sideDetail("Server", item.remote, dateFormat),
  ].join("\n");

  return (
    <div
      className={`sync-row action-${shownAction} ${choice ? "" : "is-off"}`}
      style={{ transform: `translateY(${top}px)`, height: ROW_HEIGHT }}
      role="row"
      title={detail}
    >
      <span className="sync-check">
        {!conflict && (
          <input
            type="checkbox"
            aria-label={`Include ${item.path}`}
            checked={choice !== null}
            disabled={disabled}
            onChange={(event) => onChoose(item.id, event.target.checked ? item.action : null)}
          />
        )}
      </span>
      <span className="sync-action">
        {conflict && settles.length > 0 ? (
          <select
            value={choice ?? ""}
            aria-label={`Settle ${item.path}`}
            disabled={disabled}
            onChange={(event) => onChoose(item.id, (event.target.value || null) as SyncAction)}
          >
            <option value="">Leave as is</option>
            <option value="upload">Upload local copy</option>
            <option value="download">Download server copy</option>
          </select>
        ) : (
          <>
            {Icon ? <Icon size={13} /> : <CircleAlert size={13} />}
            {conflict ? "Settle by hand" : ACTION_LABELS[item.action]}
          </>
        )}
      </span>
      <span className="sync-path">
        <FileIcon entry={{ name, kind: item.isDir ? "dir" : "file" }} />
        <span className="sync-path-text">{item.path}</span>
        {item.isDir && item.files > 0 && (
          <span className="sync-path-count">{pluralize(item.files, "file")}</span>
        )}
      </span>
      <span className="align-end">{size === undefined ? "" : formatSize(size)}</span>
      <span className="sync-reason">{describeReason(item)}</span>
    </div>
  );
});
