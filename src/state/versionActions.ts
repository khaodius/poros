// What Poros does with the history versioned S3 buckets and Backblaze B2 keep: whether a bucket
// keeps one, and restoring, downloading, opening and removing versions and deleted files.

import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { create } from "zustand";
import { formatCount } from "../lib/deletion";
import { pluralize } from "../lib/format";
import { toAppError, versions } from "../lib/ipc";
import { allTabs, editorTab, type EditorTab } from "../lib/layout";
import { baseName } from "../lib/path";
import type { DateFormat } from "../lib/settings";
import type { DeletedAction, FileEntry, ObjectVersion, Versioning } from "../lib/types";
import { bucketOf, deletedItems, formatVersionTime, versionFileName } from "../lib/versions";
import { paneOrigin, refreshFolder } from "./fileOperations";
import { useLayoutStore } from "./layoutStore";
import { useLogStore } from "./logStore";
import { track } from "./operationStore";
import { lastActivePane } from "./paneRegistry";
import { useToastStore } from "./toastStore";

/** A file on a server whose history is shown. */
export interface VersionedFile {
  sessionId: string;
  path: string;
  name: string;
}

interface VersioningState {
  /** What each bucket said, by session and bucket. */
  known: Readonly<Record<string, Versioning>>;
}

export const useVersioningStore = create<VersioningState>(() => ({ known: {} }));
const asking = new Set<string>();

export function versioningKey(sessionId: string, path: string): string | null {
  const bucket = bucketOf(path);
  return bucket === null ? null : `${sessionId}\n${bucket}`;
}

/** Asks once for each bucket whether it keeps versions. One that cannot say counts as unknown,
 * which offers versions and leaves the last word to the service. */
export function learnVersioning(sessionId: string, path: string): void {
  const key = versioningKey(sessionId, path);
  if (key === null || key in useVersioningStore.getState().known || asking.has(key)) return;
  asking.add(key);
  const remember = (versioning: Versioning) =>
    useVersioningStore.setState((state) => ({ known: { ...state.known, [key]: versioning } }));
  versions
    .status(sessionId, path)
    .then(remember, () => remember("unknown"))
    .finally(() => asking.delete(key));
}

function folderOf(path: string): string {
  return path.slice(0, path.lastIndexOf("/")) || "/";
}

/** Shows the change in every pane on the folder, with the file selected. */
function showChange(file: VersionedFile): void {
  refreshFolder(paneOrigin({ kind: "remote", sessionId: file.sessionId }), folderOf(file.path), [
    file.path,
  ]);
}

function log(message: string, sessionId: string): void {
  useLogStore.getState().write("info", message, sessionId);
}

/** Makes a version the current file again by copying it on top. */
export async function restoreVersion(
  file: VersionedFile,
  version: ObjectVersion,
  dateFormat: DateFormat,
): Promise<void> {
  await versions.restore(file.sessionId, file.path, version.versionId);
  log(
    `Restored the version of ${file.path} from ${formatVersionTime(version.modified, dateFormat)}`,
    file.sessionId,
  );
  showChange(file);
}

/** Removes one version or delete marker for good. */
export async function deleteVersion(
  file: VersionedFile,
  version: ObjectVersion,
  dateFormat: DateFormat,
): Promise<void> {
  await versions.remove(file.sessionId, file.path, version.versionId);
  const what = version.kind === "deleteMarker" ? "the delete marker" : "the version";
  log(
    `Deleted ${what} of ${file.path} from ${formatVersionTime(version.modified, dateFormat)} for good`,
    file.sessionId,
  );
  showChange(file);
}

/**
 * Queues the download of a version into the folder of the local pane on show, or one the user
 * picks when none is. Resolves with the folder, or null when the user picked none.
 */
export async function downloadVersion(
  file: VersionedFile,
  version: ObjectVersion,
): Promise<string | null> {
  const local = lastActivePane("local");
  const shown = local?.visible ? local.path() : null;
  const picked =
    shown ??
    (await openFileDialog({ directory: true, title: `Download a version of ${file.name} to` }));
  if (typeof picked !== "string") return null;
  const name = versionFileName(file.name, version);
  await versions.download({
    sessionId: file.sessionId,
    path: file.path,
    versionId: version.versionId,
    targetDirectory: picked,
    name,
    size: version.size,
  });
  log(
    `Queued the download of ${file.path} (version ${version.versionId}) as ${name}`,
    file.sessionId,
  );
  return picked;
}

function showsVersion(tab: EditorTab, file: VersionedFile, version: ObjectVersion): boolean {
  return (
    tab.sessionId === file.sessionId && tab.path === file.path && tab.version === version.versionId
  );
}

/** Opens a version in the editor, to read, or shows the tab already showing it. */
export async function openVersion(
  file: VersionedFile,
  version: ObjectVersion,
  dateFormat: DateFormat,
  nearGroupId?: string,
): Promise<void> {
  const layout = useLayoutStore.getState();
  const open = allTabs(layout.root).find(
    (tab): tab is EditorTab => tab.kind === "editor" && showsVersion(tab, file, version),
  );
  if (open) {
    layout.activateTab(open.id);
    return;
  }
  const document = await versions.open(file.sessionId, file.path, version.versionId);
  const time = formatVersionTime(version.modified, dateFormat);
  layout.openTab(
    {
      ...editorTab(document, file.sessionId),
      name: `${document.name} (${time})`,
      version: version.versionId,
    },
    nearGroupId,
  );
}

/** Where deleted entries are listed, and which to select once they are dealt with. */
export interface DeletedEntries {
  sessionId: string;
  folder: string;
  entries: FileEntry[];
  focusPath?: string | null;
}

const ACTION_WORDS: Record<DeletedAction, { running: string; done: string; failed: string }> = {
  restore: { running: "Restoring", done: "Restored", failed: "could not be restored" },
  purge: { running: "Deleting for good", done: "Deleted for good", failed: "could not be deleted" },
};

/**
 * Brings back deleted files and folders, everything under a folder included, or removes them
 * for good, as one operation the tray shows and can cancel.
 */
export async function resolveDeleted(action: DeletedAction, request: DeletedEntries) {
  const { sessionId, folder, entries } = request;
  if (entries.length === 0) return;
  const words = ACTION_WORDS[action];
  const write = useLogStore.getState().write;
  const showToast = useToastStore.getState().show;
  const what = entries.length === 1 ? entries[0].name : pluralize(entries.length, "item");
  const title = `${words.running} ${what} in ${baseName(folder) || folder}`;
  try {
    const summary = await track(
      title,
      (operationId) =>
        versions.deleted({ operationId, sessionId, action, items: deletedItems(entries) }),
      action === "restore" ? "restore" : "delete",
    );
    const inside =
      summary.files > entries.length ? `, ${pluralize(summary.files, "file")} in all` : "";
    write("info", `${words.done} ${what} in ${folder}${inside}`, sessionId);
    for (const failure of summary.failures) {
      write("error", `${failure.path}: ${failure.message}`, sessionId);
    }
    const [first] = summary.failures;
    if (first) {
      showToast(
        "error",
        summary.failed === 1
          ? `${baseName(first.path)}: ${first.message}`
          : `${formatCount(summary.failed)} files ${words.failed}. The log lists why.`,
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
    const focus =
      action === "restore" ? entries.map((entry) => entry.path) : (request.focusPath ?? undefined);
    refreshFolder(paneOrigin({ kind: "remote", sessionId }), folder, focus);
  }
}
