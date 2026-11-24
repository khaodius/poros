import type { ImportEntry, ImportSource } from "./types";
import { t } from "../i18n";

export interface ImportSourceText {
  label: string;
  /** What to choose when the app's sites are not found on this computer, after "Choose". */
  files: string;
}

export const IMPORT_SOURCES: Record<ImportSource, ImportSourceText> = {
  fileZilla: {
    get label() {
      return t("connect.filezilla");
    },
    files: "sitemanager.xml, or a file exported from the Site Manager (File > Export).",
  },
  winScp: {
    get label() {
      return t("connect.winscp");
    },
    files: "WinSCP.ini, or a file from Tools > Export/Backup Configuration.",
  },
  smartFtp: {
    get label() {
      return t("connect.smartftp");
    },
    files: "the list from Favorites > Tools > Export, or .xml favorites from older versions.",
  },
  cyberduck: {
    get label() {
      return t("connect.cyberduckAndMountainDuck");
    },
    files: "their bookmark files (.duck).",
  },
  putty: {
    get label() {
      return t("connect.puttyAndKitty");
    },
    files: "a .reg export of their Sessions registry key, or PuTTY's session files.",
  },
  mobaXterm: {
    get label() {
      return t("connect.mobaxterm");
    },
    files: "MobaXterm.ini, or a .mxtsessions file from Sessions > Export.",
  },
  openSsh: {
    get label() {
      return t("connect.opensshConfig");
    },
    files: "an ssh config file, such as ~/.ssh/config.",
  },
  fileManager: {
    get label() {
      return t("connect.gnomeFilesAndKdeDolphin");
    },
    files: "gtk-3.0/bookmarks or user-places.xbel.",
  },
  urlList: {
    get label() {
      return t("connect.listOfAddresses");
    },
    files: "a text file with one sftp:// or ftp:// address per line.",
  },
};

/** Sites to tick at first: those not saved in Poros already. */
export function defaultSelection(entries: ImportEntry[]): Set<number> {
  const selected = new Set<number>();
  entries.forEach((entry, index) => {
    if (!entry.existingId) selected.add(index);
  });
  return selected;
}

/**
 * The entries an import of `selected` saves: those, and the jump hosts they go through that
 * are not saved already, as the backend plans it. Each jump host brought along maps to a site
 * that needs it.
 */
export function plannedImports(
  entries: ImportEntry[],
  selected: ReadonlySet<number>,
): Map<number, number | null> {
  const planned = new Map<number, number | null>();
  for (const index of [...selected].sort((first, second) => first - second)) {
    if (index < 0 || index >= entries.length) continue;
    planned.set(index, null);
  }
  for (const index of [...planned.keys()]) {
    let jump = entries[index].jump ?? null;
    const seen = new Set<number>([index]);
    while (jump !== null && !seen.has(jump)) {
      seen.add(jump);
      const target = entries[jump];
      if (!target || (!selected.has(jump) && target.existingId)) break;
      if (!planned.has(jump)) planned.set(jump, index);
      jump = target.jump ?? null;
    }
  }
  return planned;
}

/** "Clients / Acme / " for a site in nested folders. */
export function folderPrefix(entry: ImportEntry): string {
  return entry.folder.length > 0 ? `${entry.folder.join(" / ")} / ` : "";
}
