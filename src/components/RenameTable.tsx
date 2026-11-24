import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowRight } from "lucide-react";
import { changedRange } from "../lib/bulkRename";
import { useT } from "../i18n";

const ROW_HEIGHT = 26;

export interface RenameTableRow {
  name: string;
  newName: string;
  /** How the row reads: about to change or changed, left alone, or in the way. */
  tone: "plain" | "dim" | "bad";
  /** What the last column says of the row. */
  status: string;
}

interface RenameTableProps {
  label: string;
  count: number;
  /** The row at a position, asked for only while it is on screen. */
  row: (index: number) => RenameTableRow;
  /** What to say in place of rows when there are none. */
  empty: string;
}

/** Old names beside new ones, with what changed marked. Only the rows on screen are drawn. */
export function RenameTable({ label, count, row, empty }: RenameTableProps) {
  const t = useT();
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
  });

  return (
    <div className="rename-table" role="table" aria-label={label} aria-rowcount={count + 1}>
      <div className="rename-table-header" role="row">
        <span role="columnheader">{t("common.name")}</span>
        <span aria-hidden="true" />
        <span role="columnheader">{t("rename.newName")}</span>
        <span role="columnheader">{t("rename.status")}</span>
      </div>
      {/* Focusable so the keyboard can scroll it. */}
      <div ref={scrollRef} className="rename-table-body" role="rowgroup" tabIndex={0}>
        {count === 0 ? (
          <p className="rename-table-empty">{empty}</p>
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((item) => (
              <TableRow key={item.key} row={row(item.index)} index={item.index} top={item.start} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function TableRow({ row, index, top }: { row: RenameTableRow; index: number; top: number }) {
  const t = useT();
  const { name, newName, tone, status } = row;
  const [start, end] = changedRange(name, newName);
  const changed = name !== newName;
  return (
    <div
      className={`rename-table-row is-${tone}`}
      role="row"
      aria-rowindex={index + 2}
      style={{ transform: `translateY(${top}px)` }}
    >
      <span role="cell" title={name}>
        <bdi>{name}</bdi>
      </span>
      <ArrowRight size={12} aria-hidden="true" />
      <span role="cell" title={newName}>
        {newName === "" ? (
          <em>{t("rename.noName")}</em>
        ) : (
          <bdi>
            {newName.slice(0, start)}
            {changed && (
              <mark className={start === end ? "is-cut" : ""}>{newName.slice(start, end)}</mark>
            )}
            {newName.slice(end)}
          </bdi>
        )}
      </span>
      <span role="cell" className="rename-table-status" title={status}>
        {status}
      </span>
    </div>
  );
}
