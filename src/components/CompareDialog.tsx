import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowLeftRight, ChevronDown, ChevronUp } from "lucide-react";
import {
  adjacentChange,
  changeStarts,
  displayItems,
  lineEndingNote,
  overviewMarks,
  segments,
  swapComparison,
  verdict,
  type DisplayItem,
} from "../lib/compare";
import { formatSize, pluralize } from "../lib/format";
import { fileOperations, toAppError } from "../lib/ipc";
import type { CompareRow, CompareRowKind, Comparison, FileLocation } from "../lib/types";
import { Dialog } from "./Dialog";

export interface CompareSide {
  location: FileLocation;
  path: string;
  name: string;
  /** The tab the file was picked in. */
  where: string;
}

interface CompareDialogProps {
  left: CompareSide;
  right: CompareSide;
  onClose: () => void;
}

interface Options {
  ignoreWhitespace: boolean;
  ignoreCase: boolean;
}

/** A comparison and the options it was made with, so a stale one shows as such. */
interface Outcome {
  options: Options;
  comparison?: Comparison;
  error?: string;
}

const ESTIMATED_ROW_HEIGHT = 20;
/** The dialog narrows to fit smaller windows. */
const DIALOG_WIDTH = 1400;

export function CompareDialog({ left, right, onClose }: CompareDialogProps) {
  const [options, setOptions] = useState<Options>({ ignoreWhitespace: false, ignoreCase: false });
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [swapped, setSwapped] = useState(false);
  const [changesOnly, setChangesOnly] = useState(false);
  const [expanded, setExpanded] = useState<ReadonlySet<number>>(new Set());

  useEffect(() => {
    const operationId = crypto.randomUUID();
    let current = true;
    fileOperations
      .compare({
        operationId,
        left: { location: left.location, path: left.path },
        right: { location: right.location, path: right.path },
        ...options,
      })
      .then((comparison) => current && setOutcome({ options, comparison }))
      .catch((caught) => current && setOutcome({ options, error: toAppError(caught).message }));
    return () => {
      current = false;
      void fileOperations.cancel(operationId);
    };
  }, [left, right, options]);

  const loading = outcome?.options !== options;
  const comparison = useMemo(() => {
    const found = outcome?.comparison;
    return found && swapped ? swapComparison(found) : found;
  }, [outcome, swapped]);
  const [first, second] = swapped ? [right, left] : [left, right];
  const ignoring = options.ignoreWhitespace || options.ignoreCase;
  const content = comparison?.content;

  const toggle = (key: keyof Options) =>
    setOptions((current) => ({ ...current, [key]: !current[key] }));

  return (
    <Dialog title="Compare files" onClose={onClose} width={DIALOG_WIDTH} className="compare-dialog">
      <div className="compare-files">
        <FileHeading side={first} size={comparison?.leftSize} />
        <button
          type="button"
          className="icon-button"
          title="Swap sides"
          aria-label="Swap sides"
          onClick={() => setSwapped((current) => !current)}
        >
          <ArrowLeftRight size={15} />
        </button>
        <FileHeading side={second} size={comparison?.rightSize} />
      </div>

      <div className="compare-toolbar">
        <label className="check">
          <input
            type="checkbox"
            checked={changesOnly}
            disabled={content?.kind !== "text"}
            onChange={(event) => {
              setChangesOnly(event.target.checked);
              setExpanded(new Set());
            }}
          />
          Only changes
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={options.ignoreWhitespace}
            onChange={() => toggle("ignoreWhitespace")}
          />
          Ignore whitespace
        </label>
        <label className="check">
          <input
            type="checkbox"
            checked={options.ignoreCase}
            onChange={() => toggle("ignoreCase")}
          />
          Ignore case
        </label>
        <span className="compare-verdict" role="status">
          {loading
            ? "Comparing..."
            : comparison && content?.kind === "text" && verdict(comparison, ignoring)}
        </span>
      </div>

      {outcome?.error && !loading && <p className="form-error">{outcome.error}</p>}
      {content?.kind === "text" && (
        <>
          {lineEndingNote(content.leftLineEnding, content.rightLineEnding) && (
            <p className="field-hint">
              {lineEndingNote(content.leftLineEnding, content.rightLineEnding)}
            </p>
          )}
          <CompareRows
            leftLines={content.leftLines}
            rightLines={content.rightLines}
            rows={content.rows}
            changesOnly={changesOnly}
            expanded={expanded}
            stale={loading}
            onExpand={(start) => setExpanded((current) => new Set(current).add(start))}
          />
        </>
      )}
      {content?.kind === "binary" && comparison && (
        <div className="compare-binary">
          <p>{verdict(comparison, ignoring)}</p>
          {!comparison.identical && comparison.leftSize !== comparison.rightSize && (
            <p className="field-hint">
              {formatSize(comparison.leftSize)} on the left, {formatSize(comparison.rightSize)} on
              the right.
            </p>
          )}
        </div>
      )}
      {!outcome && <div className="compare-binary">Reading both files...</div>}
    </Dialog>
  );
}

function FileHeading({ side, size }: { side: CompareSide; size?: number }) {
  return (
    <div className="compare-file" title={side.path}>
      <span className="compare-file-name">{side.name}</span>
      <span className="compare-file-where">
        {side.where}
        {size !== undefined && `, ${formatSize(size)}`}
      </span>
    </div>
  );
}

interface CompareRowsProps {
  leftLines: string[];
  rightLines: string[];
  rows: CompareRow[];
  changesOnly: boolean;
  expanded: ReadonlySet<number>;
  stale: boolean;
  onExpand: (start: number) => void;
}

/** Where the user stepped to with next and previous; tied to the items it was found in. */
interface Anchor {
  items: DisplayItem[];
  position: number;
}

function CompareRows({
  leftLines,
  rightLines,
  rows,
  changesOnly,
  expanded,
  stale,
  onExpand,
}: CompareRowsProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const items = useMemo(
    () => displayItems(rows, changesOnly, expanded),
    [rows, changesOnly, expanded],
  );
  const starts = useMemo(() => changeStarts(items, rows), [items, rows]);
  const marks = useMemo(() => overviewMarks(items, rows), [items, rows]);
  const [anchor, setAnchor] = useState<Anchor>({ items, position: -1 });
  const position = anchor.items === items ? anchor.position : -1;
  const currentIndex = starts.indexOf(position);
  const currentEnd = useMemo(() => {
    let end = position;
    while (end >= 0 && end < items.length && isChangedItem(items[end], rows)) end++;
    return end;
  }, [items, rows, position]);
  const numberWidth = `${String(Math.max(leftLines.length, rightLines.length)).length + 1}ch`;

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ESTIMATED_ROW_HEIGHT,
    overscan: 12,
  });

  const goTo = (target: number | null) => {
    if (target === null) return;
    setAnchor({ items, position: target });
    virtualizer.scrollToIndex(target, { align: "center" });
  };
  const step = (direction: 1 | -1) => goTo(adjacentChange(starts, position, direction));

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!event.altKey || (event.key !== "ArrowDown" && event.key !== "ArrowUp")) return;
    event.preventDefault();
    step(event.key === "ArrowDown" ? 1 : -1);
  };

  return (
    <div className={`compare-view ${stale ? "is-stale" : ""}`} onKeyDown={handleKeyDown}>
      <div className="compare-steps">
        <button
          type="button"
          className="icon-button"
          title="Previous change (Alt+Up)"
          aria-label="Previous change"
          disabled={adjacentChange(starts, position, -1) === null}
          onClick={() => step(-1)}
        >
          <ChevronUp size={15} />
        </button>
        <button
          type="button"
          className="icon-button"
          title="Next change (Alt+Down)"
          aria-label="Next change"
          disabled={adjacentChange(starts, position, 1) === null}
          onClick={() => step(1)}
        >
          <ChevronDown size={15} />
        </button>
        <span>
          {starts.length === 0
            ? "No changes"
            : currentIndex >= 0
              ? `Change ${currentIndex + 1} of ${starts.length}`
              : pluralize(starts.length, "change")}
        </span>
      </div>
      <div className="compare-body">
        <div
          ref={scrollRef}
          className="compare-scroll"
          tabIndex={0}
          style={{ "--line-number-width": numberWidth } as CSSProperties}
        >
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((virtual) => {
              const item = items[virtual.index];
              const style = { transform: `translateY(${virtual.start}px)` };
              if (item.kind === "fold") {
                return (
                  <button
                    key={`fold-${item.start}`}
                    type="button"
                    ref={virtualizer.measureElement}
                    data-index={virtual.index}
                    className="compare-fold"
                    style={style}
                    onClick={() => onExpand(item.start)}
                  >
                    Show {pluralize(item.end - item.start, "unchanged line")}
                  </button>
                );
              }
              const row = rows[item.index];
              const current = virtual.index >= position && virtual.index < currentEnd;
              return (
                <div
                  key={item.index}
                  ref={virtualizer.measureElement}
                  data-index={virtual.index}
                  className={`compare-row is-${row.kind} ${current ? "is-current" : ""}`}
                  style={style}
                >
                  <LineCell
                    number={row.left}
                    text={row.left === undefined ? undefined : leftLines[row.left]}
                    kind={row.kind}
                    changes={row.leftChanges}
                    side="left"
                  />
                  <LineCell
                    number={row.right}
                    text={row.right === undefined ? undefined : rightLines[row.right]}
                    kind={row.kind}
                    changes={row.rightChanges}
                    side="right"
                  />
                </div>
              );
            })}
          </div>
        </div>
        <div className="compare-overview" aria-hidden>
          {marks.map((mark, index) => (
            <span
              key={index}
              className={`compare-mark is-${mark.kind}`}
              style={{ top: `${mark.top * 100}%`, height: `${mark.height * 100}%` }}
              onClick={() => goTo(starts[index])}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function isChangedItem(item: DisplayItem, rows: CompareRow[]): boolean {
  return item.kind === "row" && rows[item.index].kind !== "equal";
}

interface LineCellProps {
  number?: number;
  text?: string;
  kind: CompareRowKind;
  changes?: [number, number][];
  side: "left" | "right";
}

function LineCell({ number, text, kind, changes, side }: LineCellProps) {
  if (text === undefined || number === undefined) {
    return (
      <>
        <span className="compare-number" />
        <span className="compare-text is-missing" />
      </>
    );
  }
  return (
    <>
      <span className="compare-number">{number + 1}</span>
      <span className={`compare-text is-${side}`}>
        {segments(text, kind, changes).map((segment, index) =>
          segment.changed ? (
            <mark key={index}>{segment.text}</mark>
          ) : (
            <span key={index}>{segment.text}</span>
          ),
        )}
      </span>
    </>
  );
}
