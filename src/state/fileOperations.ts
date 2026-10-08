// Moving and copying files within one filesystem: on this computer, or on one SFTP server.
// Files headed elsewhere go through the transfer queue instead.

import { localSource, type PathStyle } from "../lib/fileSource";
import { sameFilesystem, toLocation, worksInPlace, type FileOrigin } from "../lib/fileOrigin";
import { pluralize } from "../lib/format";
import { fileOperations, toAppError } from "../lib/ipc";
import { baseName, breadcrumbs, isWithin, samePath } from "../lib/path";
import type {
  Direction,
  FileEntry,
  NameClash,
  OperationMethod,
  OperationSummary,
  PlaceMode,
} from "../lib/types";
import { useClipboardStore } from "./clipboardStore";
import type { DropTarget } from "./dragStore";
import { useLogStore } from "./logStore";
import { askNameClash, track } from "./operationStore";
import { getPane, listPanes, type PaneHandle } from "./paneRegistry";
import { useSessionStore } from "./sessionStore";
import { useToastStore } from "./toastStore";
import { transferBetween } from "./transferActions";

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
    },
  };
}

export function pathStyleOf(origin: Pick<FileOrigin, "kind">): PathStyle {
  return origin.kind === "remote" ? "posix" : localSource.pathStyle;
}

/** Whether entries from `origin` can be moved or copied into `target` without a transfer. */
export function placesInPlace(origin: FileOrigin, target: PaneHandle): boolean {
  return worksInPlace(origin) && sameFilesystem(origin, paneOrigin(target));
}

/** Reloads the panes showing `folder`, moving their cursor to `focusPath` when given. */
function refreshFolder(origin: FileOrigin, folder: string, focusPath?: string): void {
  const style = pathStyleOf(origin);
  for (const pane of listPanes()) {
    const shown = pane.path();
    if (shown && samePath(shown, folder, style) && sameFilesystem(paneOrigin(pane), origin)) {
      pane.refresh(focusPath);
    }
  }
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
      `${verb} ${pluralize(summary.placed.length, "item")} to ${folder}${skipped}${howItWasDone(summary.methods)}`,
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
          : `${summary.failures.length} items could not be ${mode === "move" ? "moved" : "copied"}. The log lists why.`,
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
}

/**
 * Moves or copies entries into a folder of `target`, asking first when names are taken.
 * Elsewhere the entries are queued as a transfer, which copies. False when nothing was done.
 */
export async function placeEntries(request: PlaceRequest): Promise<boolean> {
  const { mode, origin, sourceFolder, entries, target, targetFolder } = request;
  const showToast = useToastStore.getState().show;
  if (entries.length === 0) return false;
  const targetOrigin = paneOrigin(target);

  if (!placesInPlace(origin, target)) {
    if (sameFilesystem(origin, targetOrigin)) {
      const style = pathStyleOf(origin);
      // A transfer into the folder the files came from would write each file over itself.
      if (samePath(sourceFolder, targetFolder, style)) {
        showToast("info", "Copies in the same folder need an SFTP connection.");
        return false;
      }
      if (
        entries.some((entry) => entry.kind === "dir" && isWithin(targetFolder, entry.path, style))
      ) {
        showToast("info", "A folder cannot go inside itself.");
        return false;
      }
    }
    const source: PaneHandle = {
      tabId: "clipboard",
      kind: origin.kind,
      sessionId: origin.sessionId,
      label: origin.server?.host ?? "Local",
      path: () => sourceFolder,
      refresh: () => undefined,
      selected: () => entries,
      visible: false,
      activatedAt: 0,
    };
    if (mode === "move") {
      showToast("info", "Cut files are copied there; the originals stay.");
    }
    await transferBetween(source, entries, target, targetFolder);
    return true;
  }

  const style = pathStyleOf(origin);
  const inPlace = samePath(sourceFolder, targetFolder, style);
  if (inPlace && mode === "move") return false;
  const location = toLocation(targetOrigin);
  let conflict: NameClash = "keepBoth";
  try {
    if (!inPlace) {
      const names = entries.map((entry) => entry.name);
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

  const what = entries.length === 1 ? entries[0].name : pluralize(entries.length, "item");
  const title = `${mode === "move" ? "Moving" : "Copying"} ${what} to ${baseName(targetFolder)}`;
  let placed: string | undefined;
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
    report(mode, summary, targetFolder, targetOrigin.sessionId);
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
    refreshFolder(targetOrigin, targetFolder, placed);
    if (mode === "move") refreshFolder(origin, sourceFolder);
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
  const destination = getPane(target.tabId);
  const folder = target.folder ?? destination?.path();
  if (!source || !destination || !folder) return null;
  const origin = paneOrigin(source);
  if (source.kind !== destination.kind) {
    return { kind: "transfer", direction: source.kind === "local" ? "upload" : "download", folder };
  }
  if (source.tabId === destination.tabId && target.folder === null) return null;
  const style = pathStyleOf(origin);
  const sourceFolder = source.path();
  const sameFiles = sameFilesystem(origin, paneOrigin(destination));
  if (sameFiles) {
    // Over the dragged folder itself, the drag was a click that slipped.
    if (entries.some((entry) => samePath(folder, entry.path, style))) return null;
    if (entries.some((entry) => entry.kind === "dir" && isWithin(folder, entry.path, style))) {
      return { kind: "blocked", reason: "A folder cannot go inside itself" };
    }
  }
  // Between two servers, or on one reached other than over SFTP, the transfer queue copies.
  if (!placesInPlace(origin, destination)) {
    if (source.tabId === destination.tabId) {
      return { kind: "blocked", reason: "Moving on this server needs SFTP" };
    }
    if (sameFiles && sourceFolder && samePath(folder, sourceFolder, style)) return null;
    return { kind: "transfer", direction: "relay", folder };
  }
  // As in Windows Explorer, dragging to another drive copies.
  const otherDrive =
    style === "windows" &&
    !!sourceFolder &&
    !samePath(
      breadcrumbs(folder, style)[0]?.path ?? "",
      breadcrumbs(sourceFolder, style)[0]?.path ?? "",
      style,
    );
  const mode = copy || otherDrive ? "copy" : "move";
  if (mode === "move" && sourceFolder && samePath(folder, sourceFolder, style)) return null;
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
