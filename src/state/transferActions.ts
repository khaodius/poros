import { local, toAppError, transfers } from "../lib/ipc";
import { pluralize } from "../lib/format";
import { isDirLike } from "../lib/sort";
import type { Direction, FileEntry, TransferItem } from "../lib/types";
import { useLogStore } from "./logStore";
import { getPane, lastActivePane, type PaneHandle } from "./paneRegistry";
import { useToastStore } from "./toastStore";

function transferable(entries: FileEntry[]): TransferItem[] {
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
  direction: Direction,
  sessionId: string,
  items: TransferItem[],
  folder: string,
  where: string,
  sourceSessionId?: string,
): Promise<void> {
  const showToast = useToastStore.getState().show;
  if (items.length === 0) {
    showToast("info", "Nothing to transfer: only regular files and folders can be copied.");
    return;
  }
  try {
    await transfers.enqueue({
      sessionId,
      sourceSessionId: sourceSessionId ?? null,
      direction,
      targetDirectory: folder,
      items,
    });
    const summary = `Queued ${VERBS[direction]} of ${pluralize(items.length, "item")} to ${where}`;
    useLogStore.getState().write("info", summary, sessionId);
  } catch (caught) {
    showToast("error", toAppError(caught).message);
  }
}

/**
 * Queues entries from one pane into a folder of another; the panes decide the direction. Between
 * two servers the files are copied directly when both allow it, or stream through Poros.
 */
export async function transferBetween(
  source: PaneHandle,
  entries: FileEntry[],
  target: PaneHandle,
  folder: string | null,
): Promise<void> {
  const showToast = useToastStore.getState().show;
  const targetFolder = folder ?? target.path();
  if (!targetFolder || (source.tabId === target.tabId && folder === null)) return;
  if (source.kind === "local" && target.kind === "remote" && target.sessionId) {
    await enqueue(
      "upload",
      target.sessionId,
      transferable(entries),
      targetFolder,
      `${target.label}:${targetFolder}`,
    );
  } else if (source.kind === "remote" && target.kind === "local" && source.sessionId) {
    await enqueue("download", source.sessionId, transferable(entries), targetFolder, targetFolder);
  } else if (
    source.kind === "remote" &&
    target.kind === "remote" &&
    source.sessionId &&
    target.sessionId
  ) {
    await enqueue(
      "relay",
      target.sessionId,
      transferable(entries),
      targetFolder,
      `${target.label}:${targetFolder}`,
      source.sessionId,
    );
  } else {
    showToast("info", "Drop local files on a server tab to upload them.");
  }
}

export function transferFromPane(
  sourceTabId: string,
  entries: FileEntry[],
  targetTabId: string,
  folder: string | null,
) {
  const source = getPane(sourceTabId);
  const target = getPane(targetTabId);
  if (source && target) void transferBetween(source, entries, target, folder);
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
          ? "Connect to a server to upload to it."
          : "Open a local tab to download into.",
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
    useToastStore.getState().show("info", "Drop files on a server tab to upload them.");
    return;
  }
  try {
    const entries = await local.stat(paths);
    const source: PaneHandle = {
      tabId: "external",
      kind: "local",
      label: "Local",
      path: () => null,
      refresh: () => undefined,
      visible: false,
      activatedAt: 0,
    };
    await transferBetween(source, entries, target, folder);
  } catch (caught) {
    useToastStore.getState().show("error", toAppError(caught).message);
  }
}
