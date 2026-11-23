import type { MouseEvent, ReactNode } from "react";
import {
  CirclePause,
  EyeOff,
  FolderSync,
  FolderTree,
  GitCompareArrows,
  Info,
  ListChecks,
  LoaderCircle,
  SlidersHorizontal,
  TriangleAlert,
  X,
} from "lucide-react";
import type { FolderCompareBarModel } from "../hooks/useFolderCompare";
import { LEGEND_STATES, STATE_LABELS } from "../lib/folderCompare";
import { pluralize } from "../lib/format";
import type { MenuItem } from "./ContextMenu";
import {
  COMPARE_FOLDERS_SHORTCUT,
  DEEP_COMPARE_LABEL,
  optionItems,
  selectItems,
} from "./folderCompareMenu";
import { COMPARE_CLASSES, COMPARE_GLYPHS } from "./folderCompareMarks";

interface FolderCompareBarProps {
  bar: FolderCompareBarModel;
  /** Opens a menu below the button pressed. */
  onMenu: (anchor: DOMRect, items: MenuItem[]) => void;
}

/**
 * The bar over a compared pane: which pane it is compared with, how many rows are in each
 * state, and what to do about it.
 */
export function FolderCompareBar({ bar, onMenu }: FolderCompareBarProps) {
  const { partner, counts, deep } = bar;
  const running = deep?.status === "running";
  const done = deep?.status === "done";
  const where = partner.otherWindow ? `${partner.label} in another window` : partner.label;
  const menu = (items: MenuItem[]) => (event: MouseEvent<HTMLButtonElement>) =>
    onMenu(event.currentTarget.getBoundingClientRect(), items);
  const synchronize =
    "open" in bar.synchronize
      ? { label: "Synchronize these folders", open: bar.synchronize.open }
      : { label: `Synchronize these folders (${bar.synchronize.unavailable})`, open: undefined };

  return (
    <div className="folder-compare-bar" role="group" aria-label="Folder comparison">
      <div className="folder-compare-main">
        <span
          className="folder-compare-partner"
          title={`Compared with ${where}${partner.path ? `: ${partner.path}` : ""}`}
        >
          <GitCompareArrows size={13} aria-hidden />
          <span className="folder-compare-with">
            Compared with <strong>{where}</strong>
          </span>
          {partner.path && <span className="folder-compare-path">{partner.path}</span>}
        </span>
        <span className="folder-compare-actions">
          <BarButton
            label={done ? `${DEEP_COMPARE_LABEL} again` : DEEP_COMPARE_LABEL}
            pressed={done}
            disabled={!counts || running}
            onClick={bar.startDeep}
          >
            <FolderTree size={14} />
          </BarButton>
          <BarButton label="Select by state" onClick={menu(selectItems(bar))}>
            <ListChecks size={14} />
          </BarButton>
          <BarButton label="Comparison options" onClick={menu(optionItems(bar))}>
            <SlidersHorizontal size={14} />
          </BarButton>
          <BarButton
            label={synchronize.label}
            disabled={!synchronize.open}
            onClick={() => synchronize.open?.()}
          >
            <FolderSync size={14} />
          </BarButton>
          <BarButton label={`Stop comparing (${COMPARE_FOLDERS_SHORTCUT})`} onClick={bar.close}>
            <X size={14} />
          </BarButton>
        </span>
      </div>
      <ul className="folder-compare-legend" aria-label="Rows by how they compare">
        {LEGEND_STATES.map((state) => {
          const Glyph = COMPARE_GLYPHS[state];
          const count = counts?.[state];
          const concealed = state === "identical" && bar.options.hideIdentical;
          const described = count === undefined ? "nothing to compare yet" : count.toLocaleString();
          return (
            <li
              key={state}
              className={`folder-compare-chip is-${COMPARE_CLASSES[state]} ${count ? "" : "is-empty"}`}
              title={`${STATE_LABELS[state]}: ${described}${concealed ? ", not shown" : ""}`}
            >
              <Glyph size={12} aria-hidden />
              <span>{STATE_LABELS[state]}</span>
              <span className="folder-compare-count">{count?.toLocaleString() ?? "-"}</span>
              {concealed && <EyeOff size={12} className="folder-compare-concealed" aria-hidden />}
            </li>
          );
        })}
      </ul>
      {deep?.status === "running" && (
        <p className="folder-compare-status" role="status">
          <LoaderCircle size={13} className="spin" aria-hidden />
          <span>
            Comparing subfolders: {pluralize(deep.leftEntries + deep.rightEntries, "item")} listed
            so far
          </span>
          <button type="button" className="link-button" onClick={bar.stopDeep}>
            Stop
          </button>
        </p>
      )}
      {deep?.status === "failed" && (
        <p className="folder-compare-status is-failed" role="status">
          <Info size={13} aria-hidden />
          <span>Could not compare subfolders: {deep.message}</span>
          <button type="button" className="link-button" onClick={bar.dismissDeep}>
            Dismiss
          </button>
        </p>
      )}
      {deep?.status === "done" && deep.comparison.truncated && (
        <p className="folder-compare-status">
          <Info size={13} aria-hidden />
          <span>There are too many subfolders to mark them all. The deepest are not marked.</span>
        </p>
      )}
      {bar.paused && (
        <p className="folder-compare-status" role="status">
          <CirclePause size={13} aria-hidden />
          <span>{bar.paused}.</span>
        </p>
      )}
      {bar.outdated && (
        <p className="folder-compare-status is-outdated" role="status">
          <TriangleAlert size={13} aria-hidden />
          <span>{bar.outdated}.</span>
        </p>
      )}
      {bar.note && (
        <p className="folder-compare-status" role="status">
          <Info size={13} aria-hidden />
          <span>{bar.note}. The panes are no longer in step.</span>
        </p>
      )}
    </div>
  );
}

interface BarButtonProps {
  label: string;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  pressed?: boolean;
  children: ReactNode;
}

function BarButton({ label, onClick, disabled, pressed, children }: BarButtonProps) {
  return (
    <button
      type="button"
      className={`icon-button ${pressed ? "is-pressed" : ""}`}
      title={label}
      aria-label={label}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
