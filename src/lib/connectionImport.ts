import type { ImportEntry, ImportSource } from "./types";

export interface ImportSourceText {
  label: string;
  /** What to choose when the app's sites are not found on this computer, after "Choose". */
  files: string;
}

export const IMPORT_SOURCES: Record<ImportSource, ImportSourceText> = {
  fileZilla: {
    label: "FileZilla",
    files: "sitemanager.xml, or a file exported from the Site Manager (File > Export).",
  },
  winScp: {
    label: "WinSCP",
    files: "WinSCP.ini, or a file from Tools > Export/Backup Configuration.",
  },
  smartFtp: {
    label: "SmartFTP",
    files: "the list from Favorites > Tools > Export, or .xml favorites from older versions.",
  },
  cyberduck: {
    label: "Cyberduck and Mountain Duck",
    files: "their bookmark files (.duck).",
  },
  putty: {
    label: "PuTTY and KiTTY",
    files: "a .reg export of their Sessions registry key, or PuTTY's session files.",
  },
  mobaXterm: {
    label: "MobaXterm",
    files: "MobaXterm.ini, or a .mxtsessions file from Sessions > Export.",
  },
  openSsh: {
    label: "OpenSSH config",
    files: "an ssh config file, such as ~/.ssh/config.",
  },
  fileManager: {
    label: "GNOME Files and KDE Dolphin",
    files: "gtk-3.0/bookmarks or user-places.xbel.",
  },
  urlList: {
    label: "List of addresses",
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
