import { FolderTree, GitCompareArrows, ListChecks } from "lucide-react";
import type { FolderCompareBarModel, FolderCompareControls } from "../hooks/useFolderCompare";
import type { MenuItem } from "./ContextMenu";

export const COMPARE_FOLDERS_SHORTCUT = "Ctrl+Shift+D";
export const DEEP_COMPARE_LABEL = "Compare contents of subfolders";

/** Selects the rows in a state, so the usual transfer, drag and delete actions apply to them. */
export function selectItems(bar: FolderCompareBarModel): MenuItem[] {
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

/** The compare bar's options, remembered in the interface settings. */
export function optionItems(bar: FolderCompareBarModel): MenuItem[] {
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
            label: DEEP_COMPARE_LABEL,
            icon: <FolderTree size={14} />,
            disabled: !bar.counts || bar.deep?.status === "running",
            onSelect: bar.startDeep,
          },
        ]
      : [];
  return [...(onEmptySpace ? [toggle] : []), ...deep, ...(bar ? selectItems(bar) : [])];
}
