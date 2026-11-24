import { local, remote, toAppError, transfers } from "../lib/ipc";
import type { Point } from "../lib/fileFlight";
import { pluralize } from "../lib/format";
import { baseName } from "../lib/path";
import { isDirLike } from "../lib/sort";
import type { Direction, FileEntry, SessionInfo, TransferItem } from "../lib/types";
import { isVaultSession, sessionShowing, vaultSign } from "../lib/vaults";
import { watchTransfer } from "./fileFlight";
import { useLogStore } from "./logStore";
import { getPane, lastActivePane, type PaneHandle } from "./paneRegistry";
import { useSessionStore } from "./sessionStore";
import { useToastStore } from "./toastStore";
import { t } from "../i18n";

/** The entries that can be copied: files and folders, not devices or broken links. */
export function transferable(entries: FileEntry[]): TransferItem[] {
  return entries
    .filter((entry) => entry.kind !== "other" && entry.linkTarget !== "broken")
    .map((entry) => ({
      path: entry.path,
      name: entry.name,
      isDir: isDirLike(entry),
      size: isDirLike(entry) ? 0 : entry.size,
    }));
}

const VERBS: Record<Direction, string> = { upload: "upload", download: "download", relay: "copy" };

async function enqueue(
  route: Route,
  items: TransferItem[],
  folder: string,
  where: string,
): Promise<boolean> {
  const showToast = useToastStore.getState().show;
  if (items.length === 0) {
    showToast("info", t("transfers.nothingToTransferOnlyRegular"));
    return false;
  }
  const { direction, sessionId, sourceSessionId } = route;
  try {
    await transfers.enqueue({
      sessionId,
      sourceSessionId: sourceSessionId ?? null,
      direction,
      targetDirectory: folder,
      items,
    });
    const summary = t("transfers.queuedOfItemsTo", {
      VERBS: VERBS[direction],
      count: items.length,
      where,
    });
    useLogStore.getState().write("info", summary, sessionId);
    return true;
  } catch (caught) {
    showToast("error", toAppError(caught).message);
    return false;
  }
}

interface Route {
  direction: Direction;
  /** The server written to, or read from for a download. */
  sessionId: string;
  /** The server a copy between servers reads from. */
  sourceSessionId?: string;
}

/**
 * The pane as files headed for `folder` see it. A folder inside an unlocked vault is written to
 * through the vault, also from a pane showing the folder the vault is in, so that nothing lands
 * unencrypted among the vault's encrypted files.
 */
export function paneWritingTo(pane: PaneHandle, folder: string): PaneHandle {
  if (pane.kind !== "remote" || !pane.sessionId) return pane;
  const sessionId = sessionShowing(useSessionStore.getState().sessions, pane.sessionId, folder);
  return sessionId === pane.sessionId ? pane : { ...pane, sessionId };
}

/**
 * Says so and returns true when `folder` is a vault that is not unlocked: files put there
 * through the server would lie unencrypted among the vault's encrypted files. The pane showing
 * the vault's folder knows by its card. A folder `target` does not show, usually a row of the
 * folder the vault is in, is listed to find out, and so is one whose pane has not said whether
 * its card is up, which is refused when it cannot be listed. Where the user asked to see what
 * the server stores in a vault, files go in as on any server.
 */
export async function refusesLockedVault(target: PaneHandle, folder: string): Promise<boolean> {
  if (target.kind !== "remote" || !target.sessionId) return false;
  const showToast = useToastStore.getState().show;
  // A pane still waiting for the server may show the folder as it was remembered, from before
  // it became a vault, so it is listed below like a folder no pane shows.
  if (target.path() === folder && target.locked !== undefined && !target.loading?.()) {
    if (target.locked) showToast("info", t("transfers.unlockTheVaultToCopy"));
    return target.locked;
  }
  const session = useSessionStore.getState().sessions[target.sessionId] as
    { info: SessionInfo } | undefined;
  if (!session || isVaultSession(session.info)) return false;
  let entries: FileEntry[];
  try {
    entries = (await remote.list(target.sessionId, folder)).entries;
  } catch {
    // The folder the pane shows may be a vault whose card is not up yet: nothing goes in
    // unchecked. Any other folder that cannot be listed is for the copy itself to report.
    if (target.path() !== folder) return false;
    showToast("error", t("transfers.couldNotCheckWhetherThis"));
    return true;
  }
  if (vaultSign(entries) === "none") return false;
  showToast("info", t("transfers.isALockedVaultOpen", { baseName: baseName(folder) }));
  return true;
}

/** How files go from one pane to another, if they can. */
function routeBetween(source: PaneHandle, target: PaneHandle): Route | null {
  if (source.kind === "local" && target.kind === "remote" && target.sessionId) {
    return { direction: "upload", sessionId: target.sessionId };
  }
  if (source.kind === "remote" && target.kind === "local" && source.sessionId) {
    return { direction: "download", sessionId: source.sessionId };
  }
  if (
    source.kind === "remote" &&
    target.kind === "remote" &&
    source.sessionId &&
    target.sessionId
  ) {
    return { direction: "relay", sessionId: target.sessionId, sourceSessionId: source.sessionId };
  }
  return null;
}

/**
 * Queues entries from one pane into a folder of another; the panes decide the direction. Between
 * two servers the files are copied directly when both allow it, or stream through Poros. Each
 * file flies to where it lands as it finishes; `landing` is where a drag let go of them, where
 * files land whose rows the pane does not show.
 */
export async function transferBetween(
  source: PaneHandle,
  entries: FileEntry[],
  target: PaneHandle,
  folder: string | null,
  landing?: Point,
): Promise<void> {
  const targetFolder = folder ?? target.path();
  if (!targetFolder || (source.tabId === target.tabId && folder === null)) return;
  const writer = paneWritingTo(target, targetFolder);
  if (await refusesLockedVault(writer, targetFolder)) return;
  const route = routeBetween(source, writer);
  if (!route) {
    useToastStore.getState().show("info", t("transfers.dropLocalFilesOnA"));
    return;
  }
  const items = transferable(entries);
  // Watched before it is queued, as small files can be done before queuing returns. Files
  // dropped in from other programs have no pane to leave from.
  const forget =
    source.path() === null
      ? () => undefined
      : watchTransfer({
          ...route,
          source: { tabId: source.tabId },
          target: { tabId: target.tabId, folder: targetFolder, dropPoint: landing },
          items: items.map((item) => ({ path: item.path, isDir: item.isDir })),
        });
  const where = target.kind === "remote" ? `${target.label}:${targetFolder}` : targetFolder;
  if (!(await enqueue(route, items, targetFolder, where))) {
    forget();
    return;
  }
  if (folder === null) sayWhereResultsDropWent(target, "Copying", targetFolder);
}

/**
 * Says where files went that were let go of on the blank space of a pane showing search
 * results: into the folder the search started in, where the results do not show them.
 */
export function sayWhereResultsDropWent(target: PaneHandle, verb: string, folder: string) {
  if (target.showsResults?.())
    useToastStore.getState().show("info", t("transfers.to2", { verb, folder }));
}

export function transferFromPane(
  sourceTabId: string,
  entries: FileEntry[],
  targetTabId: string,
  folder: string | null,
  landing?: Point,
) {
  const source = getPane(sourceTabId);
  const target = getPane(targetTabId);
  if (source && target) void transferBetween(source, entries, target, folder, landing);
}

/** Sends entries to the pane of the other kind the user looked at last. */
export function transferToOtherSide(sourceTabId: string, entries: FileEntry[]): void {
  const source = getPane(sourceTabId);
  if (!source) return;
  const target = lastActivePane(source.kind === "local" ? "remote" : "local");
  if (!target) {
    useToastStore
      .getState()
      .show(
        "info",
        source.kind === "local"
          ? t("transfers.connectToAServerTo")
          : t("transfers.openALocalTabTo"),
      );
    return;
  }
  void transferBetween(source, entries, target, null);
}

/** Uploads files dropped from the operating system onto a server pane. */
export async function uploadDroppedPaths(
  paths: string[],
  targetTabId: string,
  folder: string | null,
) {
  const target = getPane(targetTabId);
  if (!target) return;
  if (target.kind !== "remote") {
    useToastStore.getState().show("info", t("transfers.dropFilesOnAServer"));
    return;
  }
  try {
    const entries = await local.stat(paths);
    const source: PaneHandle = {
      tabId: "external",
      kind: "local",
      label: t("transfers.local"),
      path: () => null,
      refresh: () => undefined,
      selected: () => [],
      visible: false,
      activatedAt: 0,
    };
    await transferBetween(source, entries, target, folder);
  } catch (caught) {
    useToastStore.getState().show("error", toAppError(caught).message);
  }
}
