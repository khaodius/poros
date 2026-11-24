import { ArchiveRestore, ClipboardCopy, History, RefreshCw, Trash2 } from "lucide-react";
import { pluralize } from "../lib/format";
import type { Versioning } from "../lib/types";
import { keepsVersions } from "../lib/versions";
import type { MenuItem } from "./ContextMenu";

/** Versions... for a file, or why there are none, as menus say of what is unavailable. */
export function versionsMenuItem(versioning: Versioning | undefined, open: () => void): MenuItem {
  const keeps = keepsVersions(versioning);
  return {
    label: keeps ? "Versions..." : "Versions (this bucket keeps none)",
    icon: <History size={14} />,
    disabled: !keeps,
    onSelect: open,
  };
}

export function showDeletedMenuItem(
  versioning: Versioning | undefined,
  shown: boolean,
  toggle: () => void,
): MenuItem {
  const keeps = keepsVersions(versioning);
  return {
    label: keeps ? "Show deleted files" : "Show deleted files (this bucket keeps no versions)",
    checked: shown && keeps,
    disabled: !keeps,
    onSelect: toggle,
  };
}

interface DeletedMenu {
  /** The deleted entries the menu acts on. */
  count: number;
  /** A single deleted file, whose history can be shown. */
  file: boolean;
  restore: () => void;
  versions: () => void;
  purge: () => void;
  copyPaths: () => void;
  refresh: () => void;
  showDeleted: MenuItem;
}

/** What a deleted row offers: bringing it back, its history, and removing it for good. */
export function deletedMenuItems(menu: DeletedMenu): MenuItem[] {
  const several = menu.count > 1;
  return [
    {
      label: several ? `Restore ${pluralize(menu.count, "item")}` : "Restore",
      icon: <ArchiveRestore size={14} />,
      onSelect: menu.restore,
    },
    ...(menu.file && !several
      ? [{ label: "Versions...", icon: <History size={14} />, onSelect: menu.versions }]
      : []),
    "separator",
    {
      label: several ? "Copy paths" : "Copy path",
      icon: <ClipboardCopy size={14} />,
      onSelect: menu.copyPaths,
    },
    {
      label: "Refresh",
      icon: <RefreshCw size={14} />,
      shortcut: "F5",
      onSelect: menu.refresh,
    },
    menu.showDeleted,
    "separator",
    {
      label: several ? `Delete ${pluralize(menu.count, "item")} for good` : "Delete for good",
      icon: <Trash2 size={14} />,
      shortcut: "Del",
      danger: true,
      onSelect: menu.purge,
    },
  ];
}
