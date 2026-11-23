import type { MouseEvent, ReactNode } from "react";
import {
  FolderSync,
  FolderTree,
  GitCompareArrows,
  Info,
  ListChecks,
  LoaderCircle,
  SlidersHorizontal,
  X,
} from "lucide-react";
import type { FolderCompareBarModel, FolderCompareControls } from "../hooks/useFolderCompare";
import { LEGEND_STATES, STATE_LABELS } from "../lib/folderCompare";
import type { MenuItem } from "./ContextMenu";
import { COMPARE_CLASSES, COMPARE_GLYPHS } from "./FolderCompareGlyph";

export const COMPARE_FOLDERS_SHORTCUT = "Ctrl+Shift+D";
const DEEP_LABEL = "Compare contents of subfolders";

function selectItems(bar: FolderCompareBarModel): MenuItem[] {
  const counts = bar.counts;
  return [
    {
      label: "Select newer and only here",
      icon: <ListChecks size={14} />,
      disabled: !counts || counts.newer + counts.onlyHere === 0,
      onSelect: () => bar.select(["newer", "onlyHere"]),
    },
    {
      label: "Select different",
      icon: <ListChecks size={14} />,
      disabled: !counts || counts.different === 0,
      onSelect: () => bar.select(["different"]),
    },
  ];
}

function optionItems(bar: FolderCompareBarModel): MenuItem[] {
  const { options } = bar;
  return [
    {
      label: "Hide identical items",
      checked: options.hideIdentical,
      onSelect: () => bar.setOptions({ hideIdentical: !options.hideIdentical }),
    },
    {
      label: "Compare by size only",
      checked: bar.sizeOnly,
      onSelect: () => bar.setOptions({ sizeRule: bar.sizeOnly ? "sizeAndTime" : "sizeOnly" }),
    },
    {
      label: "Ignore time zone offsets (whole hours)",
      checked: options.ignoreHourOffsets,
      onSelect: () => bar.setOptions({ ignoreHourOffsets: !options.ignoreHourOffsets }),
    },
    {
      label: "Include hidden files",
      checked: options.includeHidden,
      onSelect: () => bar.setOptions({ includeHidden: !options.includeHidden }),
    },
    "separator",
    {
      label: "Browse both panes together",
      checked: options.browseTogether,
      onSelect: () => bar.setOptions({ browseTogether: !options.browseTogether }),
    },
  ];
}

/**
 * What Compare folders adds to a pane's context menu: the toggle on empty space, and while the
 * pane is compared, the ways to select by state.
 */
export function folderCompareMenuItems(
  compare: FolderCompareControls,
  onEmptySpace: boolean,
): MenuItem[] {
  const { bar } = compare;
  const toggle: MenuItem = {
    label: "Compare folders",
    icon: <GitCompareArrows size={14} />,
    shortcut: COMPARE_FOLDERS_SHORTCUT,
    checked: compare.comparing,
    onSelect: compare.toggle,
  };
  const deep: MenuItem[] =
    bar && onEmptySpace
      ? [
          {
            label: DEEP_LABEL,
            icon: <FolderTree size={14} />,
            disabled: !bar.counts || bar.deep?.status === "running",
            onSelect: bar.startDeep,
          },
        ]
      : [];
  return [...(onEmptySpace ? [toggle] : []), ...deep, ...(bar ? selectItems(bar) : [])];
}

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
        <ul className="folder-compare-legend" aria-label="Rows by how they compare">
          {LEGEND_STATES.map((state) => {
            const Glyph = COMPARE_GLYPHS[state];
            const count = counts?.[state];
            return (
              <li
                key={state}
                className={`folder-compare-chip is-${COMPARE_CLASSES[state]} ${count ? "" : "is-empty"}`}
                title={`${STATE_LABELS[state]}: ${count ?? "nothing to compare yet"}`}
              >
                <Glyph size={12} aria-hidden />
                <span>{STATE_LABELS[state]}</span>
                <span className="folder-compare-count">{count?.toLocaleString() ?? "-"}</span>
              </li>
            );
          })}
        </ul>
        <span className="folder-compare-actions">
          <BarButton
            label={done ? `${DEEP_LABEL} again` : DEEP_LABEL}
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
      {deep?.status === "running" && (
        <p className="folder-compare-status" role="status">
          <LoaderCircle size={13} className="spin" aria-hidden />
          <span>
            Comparing subfolders: {deep.leftEntries.toLocaleString()} and{" "}
            {deep.rightEntries.toLocaleString()} items listed
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
