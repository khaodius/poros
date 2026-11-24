import { FolderTree, GitCompareArrows, ListChecks } from "lucide-react";
import type { FolderCompareBarModel, FolderCompareControls } from "../hooks/useFolderCompare";
import type { MenuItem } from "./ContextMenu";
import { t } from "../i18n";

export const COMPARE_FOLDERS_SHORTCUT = "Ctrl+Shift+D";
export function deepCompareLabel(): string {
  return t("compare.compareContentsOfSubfolders");
}

/** Selects the rows in a state, so the usual transfer, drag and delete actions apply to them. */
export function selectItems(bar: FolderCompareBarModel): MenuItem[] {
  const counts = bar.counts;
  return [
    {
      label: t("compare.selectNewerAndOnlyHere"),
      icon: <ListChecks size={14} />,
      disabled: !counts || counts.newer + counts.onlyHere === 0,
      onSelect: () => bar.select(["newer", "onlyHere"]),
    },
    {
      label: t("compare.selectDifferent"),
      icon: <ListChecks size={14} />,
      disabled: !counts || counts.different === 0,
      onSelect: () => bar.select(["different"]),
    },
  ];
}

/** The compare bar's options, remembered in the interface settings. */
export function optionItems(bar: FolderCompareBarModel): MenuItem[] {
  const { options } = bar;
  return [
    {
      label: t("compare.hideIdenticalItems"),
      checked: options.hideIdentical,
      onSelect: () => bar.setOptions({ hideIdentical: !options.hideIdentical }),
    },
    {
      label: t("compare.compareBySizeOnly"),
      checked: bar.sizeOnly,
      onSelect: () => bar.setOptions({ sizeRule: bar.sizeOnly ? "sizeAndTime" : "sizeOnly" }),
    },
    {
      label: t("compare.ignoreTimeZoneOffsetsWhole"),
      checked: options.ignoreHourOffsets,
      onSelect: () => bar.setOptions({ ignoreHourOffsets: !options.ignoreHourOffsets }),
    },
    {
      label: t("compare.includeHiddenFiles"),
      checked: options.includeHidden,
      onSelect: () => bar.setOptions({ includeHidden: !options.includeHidden }),
    },
    "separator",
    {
      label: t("compare.browseBothPanesTogether"),
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
    label: t("compare.compareFolders"),
    icon: <GitCompareArrows size={14} />,
    shortcut: COMPARE_FOLDERS_SHORTCUT,
    checked: compare.comparing,
    onSelect: compare.toggle,
  };
  const deep: MenuItem[] =
    bar && onEmptySpace
      ? [
          {
            label: deepCompareLabel(),
            icon: <FolderTree size={14} />,
            disabled: !bar.counts || bar.deep?.status === "running",
            onSelect: bar.startDeep,
          },
        ]
      : [];
  return [...(onEmptySpace ? [toggle] : []), ...deep, ...(bar ? selectItems(bar) : [])];
}
