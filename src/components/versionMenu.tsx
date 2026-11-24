import { ArchiveRestore, ClipboardCopy, History, RefreshCw, Trash2 } from "lucide-react";
import type { Versioning } from "../lib/types";
import { keepsVersions } from "../lib/versions";
import type { MenuItem } from "./ContextMenu";
import { t } from "../i18n";

/** Versions... for a file, or why there are none, as menus say of what is unavailable. */
export function versionsMenuItem(versioning: Versioning | undefined, open: () => void): MenuItem {
  const keeps = keepsVersions(versioning);
  return {
    label: keeps ? t("versions.versions") : t("versions.versionsThisBucketKeepsNone"),
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
    label: keeps ? t("versions.showDeletedFiles") : t("versions.showDeletedFilesThisBucket"),
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
      label: several ? t("versions.restoreItems", { count: menu.count }) : t("versions.restore"),
      icon: <ArchiveRestore size={14} />,
      onSelect: menu.restore,
    },
    ...(menu.file && !several
      ? [{ label: t("versions.versions"), icon: <History size={14} />, onSelect: menu.versions }]
      : []),
    "separator",
    {
      label: several ? t("versions.copyPaths") : t("versions.copyPath"),
      icon: <ClipboardCopy size={14} />,
      onSelect: menu.copyPaths,
    },
    {
      label: t("common.refresh"),
      icon: <RefreshCw size={14} />,
      shortcut: "F5",
      onSelect: menu.refresh,
    },
    menu.showDeleted,
    "separator",
    {
      label: several
        ? t("versions.deleteItemsForGood", { count: menu.count })
        : t("versions.deleteForGood"),
      icon: <Trash2 size={14} />,
      shortcut: "Del",
      danger: true,
      onSelect: menu.purge,
    },
  ];
}
