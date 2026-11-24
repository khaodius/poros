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
import type { MenuItem } from "./ContextMenu";
import {
  COMPARE_FOLDERS_SHORTCUT,
  deepCompareLabel,
  optionItems,
  selectItems,
} from "./folderCompareMenu";
import { COMPARE_CLASSES, COMPARE_GLYPHS } from "./folderCompareMarks";
import { formatNumber, useT } from "../i18n";

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
  const t = useT();
  const { partner, counts, deep } = bar;
  const running = deep?.status === "running";
  const done = deep?.status === "done";
  const where = partner.otherWindow
    ? t("compare.inAnotherWindow", { label: partner.label })
    : partner.label;
  const menu = (items: MenuItem[]) => (event: MouseEvent<HTMLButtonElement>) =>
    onMenu(event.currentTarget.getBoundingClientRect(), items);
  const synchronize =
    "open" in bar.synchronize
      ? { label: t("compare.synchronizeTheseFolders"), open: bar.synchronize.open }
      : {
          label: t("compare.synchronizeTheseFolders2", {
            unavailable: bar.synchronize.unavailable,
          }),
          open: undefined,
        };

  return (
    <div className="folder-compare-bar" role="group" aria-label={t("compare.folderComparison")}>
      <div className="folder-compare-main">
        <span
          className="folder-compare-partner"
          title={
            partner.path
              ? t("compare.comparedWithAt", { where, path: partner.path })
              : t("compare.comparedWith", { where })
          }
        >
          <GitCompareArrows size={13} aria-hidden />
          <span className="folder-compare-with">
            {t.rich("compare.comparedWith", { where: <strong>{where}</strong> })}
          </span>
          {partner.path && <span className="folder-compare-path">{partner.path}</span>}
        </span>
        <span className="folder-compare-actions">
          <BarButton
            label={done ? t("compare.again", { action: deepCompareLabel() }) : deepCompareLabel()}
            pressed={done}
            disabled={!counts || running}
            onClick={bar.startDeep}
          >
            <FolderTree size={14} />
          </BarButton>
          <BarButton label={t("compare.selectByState")} onClick={menu(selectItems(bar))}>
            <ListChecks size={14} />
          </BarButton>
          <BarButton label={t("compare.comparisonOptions")} onClick={menu(optionItems(bar))}>
            <SlidersHorizontal size={14} />
          </BarButton>
          <BarButton
            label={synchronize.label}
            disabled={!synchronize.open}
            onClick={() => synchronize.open?.()}
          >
            <FolderSync size={14} />
          </BarButton>
          <BarButton
            label={t("compare.stopComparing", { shortcut: COMPARE_FOLDERS_SHORTCUT })}
            onClick={bar.close}
          >
            <X size={14} />
          </BarButton>
        </span>
      </div>
      <ul className="folder-compare-legend" aria-label={t("compare.rowsByHowTheyCompare")}>
        {LEGEND_STATES.map((state) => {
          const Glyph = COMPARE_GLYPHS[state];
          const count = counts?.[state];
          const concealed = state === "identical" && bar.options.hideIdentical;
          const described =
            count === undefined ? t("compare.nothingToCompareYet") : formatNumber(count);
          return (
            <li
              key={state}
              className={`folder-compare-chip is-${COMPARE_CLASSES[state]} ${count ? "" : "is-empty"}`}
              title={
                concealed
                  ? t("compare.stateCountHidden", { state: STATE_LABELS[state], amount: described })
                  : t("compare.stateCount", { state: STATE_LABELS[state], amount: described })
              }
            >
              <Glyph size={12} aria-hidden />
              <span>{STATE_LABELS[state]}</span>
              <span className="folder-compare-count">
                {count === undefined ? "-" : formatNumber(count)}
              </span>
              {concealed && <EyeOff size={12} className="folder-compare-concealed" aria-hidden />}
            </li>
          );
        })}
      </ul>
      {deep?.status === "running" && (
        <p className="folder-compare-status" role="status">
          <LoaderCircle size={13} className="spin" aria-hidden />
          <span>
            {t("compare.comparingSubfoldersItemsListedSo", {
              count: deep.leftEntries + deep.rightEntries,
            })}
          </span>
          <button type="button" className="link-button" onClick={bar.stopDeep}>
            {t("common.stop")}
          </button>
        </p>
      )}
      {deep?.status === "failed" && (
        <p className="folder-compare-status is-failed" role="status">
          <Info size={13} aria-hidden />
          <span>{t("compare.couldNotCompareSubfolders", { message: deep.message })}</span>
          <button type="button" className="link-button" onClick={bar.dismissDeep}>
            {t("common.dismiss")}
          </button>
        </p>
      )}
      {deep?.status === "done" && deep.comparison.truncated && (
        <p className="folder-compare-status">
          <Info size={13} aria-hidden />
          <span>{t("compare.thereAreTooManySubfolders")}</span>
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
          <span>{t("compare.thePanesAreNoLonger", { note: bar.note })}</span>
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
