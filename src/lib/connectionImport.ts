import type { ImportEntry, ImportSource } from "./types";
import { t } from "../i18n";

export interface ImportSourceText {
  label: string;
  /** What to choose when the app's sites are not found on this computer, as a sentence. */
  choose: string;
}

export const IMPORT_SOURCES: Record<ImportSource, ImportSourceText> = {
  fileZilla: {
    get label() {
      return t("connect.filezilla");
    },
    get choose() {
      return t("connect.chooseFileZillaFiles");
    },
  },
  winScp: {
    get label() {
      return t("connect.winscp");
    },
    get choose() {
      return t("connect.chooseWinScpFiles");
    },
  },
  smartFtp: {
    get label() {
      return t("connect.smartftp");
    },
    get choose() {
      return t("connect.chooseSmartFtpFiles");
    },
  },
  cyberduck: {
    get label() {
      return t("connect.cyberduckAndMountainDuck");
    },
    get choose() {
      return t("connect.chooseCyberduckFiles");
    },
  },
  putty: {
    get label() {
      return t("connect.puttyAndKitty");
    },
    get choose() {
      return t("connect.choosePuttyFiles");
    },
  },
  mobaXterm: {
    get label() {
      return t("connect.mobaxterm");
    },
    get choose() {
      return t("connect.chooseMobaXtermFiles");
    },
  },
  openSsh: {
    get label() {
      return t("connect.opensshConfig");
    },
    get choose() {
      return t("connect.chooseOpenSshFiles");
    },
  },
  fileManager: {
    get label() {
      return t("connect.gnomeFilesAndKdeDolphin");
    },
    get choose() {
      return t("connect.chooseFileManagerFiles");
    },
  },
  urlList: {
    get label() {
      return t("connect.listOfAddresses");
    },
    get choose() {
      return t("connect.chooseUrlListFiles");
    },
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
