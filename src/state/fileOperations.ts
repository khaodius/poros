// Moving and copying files within one filesystem: on this computer, or on one SFTP server.
// Files headed elsewhere go through the transfer queue instead. Deleting on SFTP servers too.

import { formatCount } from "../lib/deletion";
import { englishCount } from "../lib/format";
import type { Point } from "../lib/fileFlight";
import { localSource, type PathStyle } from "../lib/fileSource";
import {
  isInsideVault,
  sameFilesystem,
  toLocation,
  worksInPlace,
  type FileOrigin,
} from "../lib/fileOrigin";
import { folderOf } from "../lib/search";
import { fileOperations, toAppError } from "../lib/ipc";
import { baseName, breadcrumbs, childPath, isWithin, samePath } from "../lib/path";
import type {
  Direction,
  FileEntry,
  NameClash,
  OperationMethod,
  OperationSummary,
  PlaceMode,
} from "../lib/types";
import { useClipboardStore } from "./clipboardStore";
import { serverSessionId } from "../lib/vaults";
import type { DropTarget } from "./dragStore";
import { flyPlaced, measureRows } from "./fileFlight";
import { forgetListing } from "./listings";
import { useLogStore } from "./logStore";
import { askNameClash, track } from "./operationStore";
import { getPane, listPanes, type PaneHandle } from "./paneRegistry";
import { useSessionStore } from "./sessionStore";
import { useToastStore } from "./toastStore";
import { paneWritingTo, refusesLockedVault, transferBetween } from "./transferActions";
import { t } from "../i18n";

export function paneOrigin(pane: Pick<PaneHandle, "kind" | "sessionId">): FileOrigin {
  const info = pane.sessionId
    ? useSessionStore.getState().sessions[pane.sessionId]?.info
    : undefined;
  return {
    kind: pane.kind,
    sessionId: pane.sessionId,
    server: info && {
      protocol: info.protocol,
      host: info.host,
      port: info.port,
      username: info.username,
      vaultRoot: info.vault?.root,
    },
  };
}

export function pathStyleOf(origin: Pick<FileOrigin, "kind">): PathStyle {
  return origin.kind === "remote" ? "posix" : localSource.pathStyle;
}

/**
 * Whether both are one tree of folders: one filesystem, or a server and a vault unlocked on it,
 * whose files are under the vault's folder on the server.
 */
function sharesFolders(first: FileOrigin, second: FileOrigin): boolean {
  if (sameFilesystem(first, second)) return true;
  const sessions = useSessionStore.getState().sessions;
  const serverOf = (origin: FileOrigin) => {
    const info = origin.sessionId ? sessions[origin.sessionId]?.info : undefined;
    return info && serverSessionId(info);
  };
  const server = serverOf(first);
  return server !== undefined && server === serverOf(second);
}

/** Whether entries from `origin` can be moved or copied into `target` without a transfer. */
export function placesInPlace(origin: FileOrigin, target: PaneHandle): boolean {
  return worksInPlace(origin) && sameFilesystem(origin, paneOrigin(target));
}

/**
 * Reloads the panes showing `folder`, moving their cursor to `focusPath` when given. On a server
 * the folder's remembered listing is forgotten first, and so are those of the folders among
 * `changed`, which were moved, deleted or written over, with everything inside them.
 */
export function refreshFolder(
  origin: FileOrigin,
  folder: string,
  focus?: string | readonly string[],
  changed: readonly string[] = [],
): void {
  if (origin.kind === "remote" && origin.sessionId) {
    forgetListing(origin.sessionId, folder, changed);
  }
  const style = pathStyleOf(origin);
  for (const pane of listPanes()) {
    const shown = pane.path();
    if (shown && samePath(shown, folder, style) && sameFilesystem(paneOrigin(pane), origin)) {
      pane.refresh(focus);
    }
  }
}

/**
 * Reloads the panes showing the folders `entries` are in, as search results from several
 * folders have them, forgetting the remembered listings of those folders and of the entries.
 * `reloaded` is a folder already reloaded, left out.
 */
export function refreshFoldersOf(
  origin: FileOrigin,
  entries: readonly FileEntry[],
  reloaded?: string,
): void {
  const style = pathStyleOf(origin);
  const folders = new Map<string, string[]>();
  for (const entry of entries) {
    const folder = folderOf(entry, style);
    if (reloaded !== undefined && samePath(folder, reloaded, style)) continue;
    folders.set(folder, [...(folders.get(folder) ?? []), entry.path]);
  }
  for (const [folder, paths] of folders) refreshFolder(origin, folder, undefined, paths);
}

const METHOD_NOTES: Partial<Record<OperationMethod, string>> = {
  rename: "renamed in place",
  copyData: "copied by the server",
  command: "with commands on the server",
  stream: "read and written back over SFTP",
};

function howItWasDone(methods: OperationMethod[]): string {
  const notes = methods.flatMap((method) => METHOD_NOTES[method] ?? []);
  return notes.length > 0 ? ` (${notes.join(", ")})` : "";
}

function report(mode: PlaceMode, summary: OperationSummary, folder: string, sessionId?: string) {
  const write = useLogStore.getState().write;
  const verb = mode === "move" ? "Moved" : "Copied";
  if (summary.placed.length > 0 || summary.skipped > 0) {
    const skipped = summary.skipped > 0 ? `, skipped ${summary.skipped}` : "";
    write(
      "info",
      `${verb} ${englishCount(summary.placed.length, "item")} to ${folder}${skipped}${howItWasDone(summary.methods)}`,
      sessionId,
    );
  }
  for (const failure of summary.failures) {
    write("error", `${failure.path}: ${failure.message}`, sessionId);
  }
  const [first] = summary.failures;
  if (first) {
    useToastStore
      .getState()
      .show(
        "error",
        summary.failures.length === 1
          ? `${baseName(first.path)}: ${first.message}`
          : mode === "move"
            ? t("pane.itemsCouldNotBeMoved", { count: summary.failures.length })
            : t("pane.itemsCouldNotBeCopied", { count: summary.failures.length }),
      );
  }
}

export interface PlaceRequest {
  mode: PlaceMode;
  origin: FileOrigin;
  /** The folder the entries are in. */
  sourceFolder: string;
  entries: FileEntry[];
  target: PaneHandle;
  targetFolder: string;
  /** The pane the entries were dragged from; pasted ones come from wherever their folder shows. */
  sourceTabId?: string;
  /** Where a drag let go of them, for the flying pile. */
  landing?: Point;
}

/**
 * Moves or copies entries into a folder of `target`, asking first when names are taken.
 * Elsewhere the entries are queued as a transfer, which copies. False when nothing was done.
 */
export async function placeEntries(request: PlaceRequest): Promise<boolean> {
  const { mode, origin, sourceFolder, entries, targetFolder, sourceTabId, landing } = request;
  const showToast = useToastStore.getState().show;
  if (entries.length === 0) return false;
  const target = paneWritingTo(request.target, targetFolder);
  const targetOrigin = paneOrigin(target);

  const style = pathStyleOf(origin);
  // Entries are in `sourceFolder`, or, found by a search, each in a folder of its own.
  const inTarget = (entry: FileEntry) => samePath(folderOf(entry, style), targetFolder, style);

  if (!placesInPlace(origin, target)) {
    // A transfer into the folder the files came from would write each file over itself.
    if (sameFilesystem(origin, targetOrigin) && entries.some(inTarget)) {
      showToast(
        "info",
        isInsideVault(origin)
          ? t("pane.copiesInTheSameFolder")
          : t("pane.copiesInTheSameFolderNeed"),
      );
      return false;
    }
    if (
      sharesFolders(origin, targetOrigin) &&
      entries.some((entry) => entry.kind === "dir" && isWithin(targetFolder, entry.path, style))
    ) {
      showToast("info", t("pane.aFolderCannotGoInside"));
      return false;
    }
    const source: PaneHandle = {
      tabId: sourceTabId ?? "clipboard",
      kind: origin.kind,
      sessionId: origin.sessionId,
      label: origin.server?.host ?? t("pane.local"),
      path: () => sourceFolder,
      refresh: () => undefined,
      selected: () => entries,
      visible: false,
      activatedAt: 0,
    };
    if (mode === "move") {
      showToast("info", t("pane.cutFilesAreCopiedThere"));
    }
    await transferBetween(source, entries, target, targetFolder, landing);
    return true;
  }

  if (await refusesLockedVault(target, targetFolder)) return false;
  const inPlace = entries.every(inTarget);
  if (inPlace && mode === "move") return false;
  const location = toLocation(targetOrigin);
  let conflict: NameClash = "keepBoth";
  try {
    if (!inPlace) {
      // Those already in the folder are left there, or copied under a new name, unasked.
      const names = entries.filter((entry) => !inTarget(entry)).map((entry) => entry.name);
      const taken = await fileOperations.conflicts(location, names, targetFolder);
      if (taken.length > 0) {
        const choice = await askNameClash({
          mode,
          names: taken,
          total: entries.length,
          folder: baseName(targetFolder),
        });
        if (!choice) return false;
        conflict = choice;
      }
    }
  } catch (caught) {
    showToast("error", toAppError(caught).message);
    return false;
  }

  const what = entries.length === 1 ? entries[0].name : t("pane.items", { count: entries.length });
  const title = `${mode === "move" ? "Moving" : "Copying"} ${what} to ${baseName(targetFolder)}`;
  const side = { kind: origin.kind, sessionId: origin.sessionId };
  // Moved files' rows go away, so where they were is measured before they leave.
  const points = measureRows(sourceTabId, side, entries);
  let placed: string | undefined;
  // What a name clash has written over is not what it was, whether or not the rest went through.
  const written = entries.map((entry) => childPath(targetFolder, entry.name, style));
  try {
    const summary = await track(title, (operationId) =>
      fileOperations.moveOrCopy({
        operationId,
        location,
        mode,
        sources: entries.map((entry) => entry.path),
        targetDirectory: targetFolder,
        conflict,
      }),
    );
    placed = summary.placed[0];
    written.push(...summary.placed);
    report(mode, summary, targetFolder, targetOrigin.sessionId);
    flyPlaced({
      side,
      source: { tabId: sourceTabId, folder: sourceFolder, points },
      target: { tabId: target.tabId, dropPoint: landing },
      entries,
      placed: summary.placed,
    });
    return true;
  } catch (caught) {
    const error = toAppError(caught);
    if (error.kind === "cancelled") {
      useLogStore.getState().write("info", `Cancelled: ${title}`, targetOrigin.sessionId);
    } else {
      showToast("error", error.message);
      useLogStore.getState().write("error", `${title}: ${error.message}`, targetOrigin.sessionId);
    }
    return false;
  } finally {
    // Whatever was done before a failure or cancel shows too.
    refreshFolder(targetOrigin, targetFolder, placed, written);
    if (mode === "move") {
      const moved = entries.map((entry) => entry.path);
      refreshFolder(origin, sourceFolder, undefined, moved);
      // Search results moved out of the folders they were found in.
      refreshFoldersOf(origin, entries, sourceFolder);
    }
  }
}

export interface DeleteOnServer {
  origin: FileOrigin;
  /** The folder the entries are in. */
  folder: string;
  entries: FileEntry[];
  /** Where the cursor goes once the folder reloads. */
  focusPath?: string;
}

/**
 * Deletes entries on an SFTP server, showing in the tray how many of how many are gone, where it
 * is and what would not go, with a button to stop. The panes showing the folder reload after.
 */
export async function deleteOnServer(request: DeleteOnServer): Promise<void> {
  const { origin, folder, entries, focusPath } = request;
  const sessionId = origin.sessionId;
  if (!sessionId || entries.length === 0) return;
  const write = useLogStore.getState().write;
  const showToast = useToastStore.getState().show;
  const what = entries.length === 1 ? entries[0].name : t("pane.items", { count: entries.length });
  const title = `Deleting ${what} from ${baseName(folder) || folder}`;
  try {
    const summary = await track(
      title,
      (operationId) =>
        fileOperations.delete({
          operationId,
          sessionId,
          paths: entries.map((entry) => entry.path),
        }),
      "delete",
    );
    const inside =
      summary.deleted > entries.length ? `, ${formatCount(summary.deleted)} entries` : "";
    write(
      "info",
      `Deleted ${what} in ${folder}${inside}${howItWasDone(summary.methods)}`,
      sessionId,
    );
    for (const failure of summary.failures) {
      write("error", `${failure.path}: ${failure.message}`, sessionId);
    }
    const unlisted = summary.failed - summary.failures.length;
    if (unlisted > 0) {
      write("error", `${formatCount(unlisted)} more entries could not be deleted`, sessionId);
    }
    const [first] = summary.failures;
    if (first) {
      showToast(
        "error",
        summary.failed === 1
          ? `${baseName(first.path)}: ${first.message}`
          : t("pane.entriesCouldNotBeDeleted", { count: summary.failed }),
      );
    }
  } catch (caught) {
    const error = toAppError(caught);
    if (error.kind === "cancelled") {
      write("info", `Cancelled: ${title}`, sessionId);
    } else {
      showToast("error", error.message);
      write("error", `${title}: ${error.message}`, sessionId);
    }
  } finally {
    // Whatever went before a failure or cancel shows too.
    const deleted = entries.map((entry) => entry.path);
    refreshFolder(origin, folder, focusPath, deleted);
    // Search results deleted from the folders they were found in.
    refreshFoldersOf(origin, entries, folder);
  }
}

export type DropAction =
  | { kind: "place"; mode: PlaceMode; source: PaneHandle; target: PaneHandle; folder: string }
  | { kind: "transfer"; direction: Direction; folder: string }
  | { kind: "blocked"; reason: string };

/**
 * What dropping files dragged from one pane onto `target` does: a move or copy within one
 * filesystem, a transfer elsewhere, or nothing at all.
 */
export function dropAction(
  sourceTabId: string,
  entries: FileEntry[],
  target: DropTarget,
  copy: boolean,
): DropAction | null {
  if (target.kind !== "pane") return null;
  const source = getPane(sourceTabId);
  const shown = getPane(target.tabId);
  const folder = target.folder ?? shown?.path();
  if (!source || !shown || !folder) return null;
  const destination = paneWritingTo(shown, folder);
  const origin = paneOrigin(source);
  if (source.kind !== destination.kind) {
    return { kind: "transfer", direction: source.kind === "local" ? "upload" : "download", folder };
  }
  if (source.tabId === destination.tabId && target.folder === null) return null;
  const style = pathStyleOf(origin);
  // Each entry is where its path says: search results come from many folders, not only the
  // one the source pane stands for.
  const inFolder = (entry: FileEntry) => samePath(folderOf(entry, style), folder, style);
  const destinationOrigin = paneOrigin(destination);
  const sameFiles = sameFilesystem(origin, destinationOrigin);
  if (sharesFolders(origin, destinationOrigin)) {
    // Over the dragged folder itself, the drag was a click that slipped.
    if (entries.some((entry) => samePath(folder, entry.path, style))) return null;
    if (entries.some((entry) => entry.kind === "dir" && isWithin(folder, entry.path, style))) {
      return { kind: "blocked", reason: t("pane.aFolderCannotGoInsideItself") };
    }
  }
  // Between two servers, or on one reached other than over SFTP, the transfer queue copies.
  if (!placesInPlace(origin, destination)) {
    if (source.tabId === destination.tabId && source.sessionId === destination.sessionId) {
      return {
        kind: "blocked",
        reason: isInsideVault(origin)
          ? t("pane.filesCannotBeMovedInside")
          : t("pane.movingOnThisServerNeeds"),
      };
    }
    if (sameFiles && entries.every(inFolder)) return null;
    if (sameFiles && entries.some(inFolder)) {
      return { kind: "blocked", reason: t("pane.someOfTheseAreIn") };
    }
    return { kind: "transfer", direction: "relay", folder };
  }
  // As in Windows Explorer, dragging to another drive copies.
  const drive = (path: string) => breadcrumbs(path, style)[0]?.path ?? "";
  const otherDrive =
    style === "windows" &&
    entries.some((entry) => !samePath(drive(folder), drive(entry.path), style));
  const mode = copy || otherDrive ? "copy" : "move";
  // Those already there stay where they are; the others move.
  if (mode === "move" && entries.every(inFolder)) return null;
  return { kind: "place", mode, source, target: destination, folder };
}

/** Puts entries a pane shows on the clipboard, to be moved or copied where they are pasted. */
export function putOnClipboard(mode: PlaceMode, pane: PaneHandle, entries: FileEntry[]): void {
  const folder = pane.path();
  if (!folder || entries.length === 0) return;
  useClipboardStore.getState().put({ mode, origin: paneOrigin(pane), folder, entries });
}

/** Pastes the clipboard into `folder` of `target`; cut entries leave the clipboard. */
export async function paste(target: PaneHandle, folder: string): Promise<void> {
  const clip = useClipboardStore.getState().clip;
  if (!clip) return;
  const done = await placeEntries({
    mode: clip.mode,
    origin: clip.origin,
    sourceFolder: clip.folder,
    entries: clip.entries,
    target,
    targetFolder: folder,
  });
  if (done && clip.mode === "move" && useClipboardStore.getState().clip === clip) {
    useClipboardStore.getState().clear();
  }
}
