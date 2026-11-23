// Opens the scheduler with a transfer task made from transfers in the queue or files in a tab.

import { finishedRuns, serverChanges } from "../lib/schedule";
import {
  downloadBlocker,
  taskFromFiles,
  taskFromJobs,
  type SessionFacts,
  type TaskDraft,
} from "../lib/scheduleDraft";
import { isDirLike } from "../lib/sort";
import type { FileEntry, JobSnapshot, TaskView } from "../lib/types";
import { forgetListingsWithin, forgetServerListings } from "./listings";
import { getPane, lastActivePane } from "./paneRegistry";
import { useSessionStore } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";
import { useUiStore } from "./uiStore";

/** What a draft needs to know about a session open in this window. */
export function sessionFacts(sessionId: string | undefined): SessionFacts | undefined {
  const entry = sessionId ? useSessionStore.getState().sessions[sessionId] : undefined;
  if (!entry) return undefined;
  return {
    label: entry.info.label,
    savedConnectionId: entry.info.savedConnectionId,
    vault: Boolean(entry.info.vault),
  };
}

function existsAction() {
  return useSettingsStore.getState().settings.transfers.existsAction;
}

function openDraft(draft: TaskDraft): void {
  if ("problem" in draft) {
    useToastStore.getState().show("info", draft.problem);
    return;
  }
  useUiStore.getState().open({ kind: "schedules", draft: draft.task });
}

/** Opens the scheduler with a task that repeats the transfers. */
export function scheduleJobs(jobs: JobSnapshot[]): void {
  openDraft(taskFromJobs(jobs, sessionFacts, existsAction()));
}

/** Why the tab's files cannot be scheduled, in a few words for its menu; or null. */
export function scheduleBlocker(tabId: string): string | null {
  const pane = getPane(tabId);
  if (!pane) return "no tab";
  return pane.kind === "remote" ? downloadBlocker(sessionFacts(pane.sessionId)) : null;
}

/**
 * Opens the scheduler with a task that copies files selected in a tab to the other side: the
 * server tab looked at last for an upload, the local tab looked at last for a download.
 */
export function scheduleFiles(tabId: string, entries: FileEntry[]): void {
  const pane = getPane(tabId);
  if (!pane) return;
  const items = entries.map((entry) => ({ path: entry.path, isDir: isDirLike(entry) }));
  const exists = existsAction();
  if (pane.kind === "local") {
    const server = lastActivePane("remote");
    openDraft(
      taskFromFiles({
        direction: "upload",
        entries: items,
        server: sessionFacts(server?.sessionId),
        destination: server?.path() ?? undefined,
        exists,
      }),
    );
  } else {
    openDraft(
      taskFromFiles({
        direction: "download",
        entries: items,
        server: sessionFacts(pane.sessionId),
        destination: lastActivePane("local")?.path() ?? undefined,
        exists,
      }),
    );
  }
}

/**
 * A task ran in the backend or in poros-cli, on sessions of its own, so the panes on its
 * servers were not told what it changed: the listings remembered of the folders it wrote into
 * are forgotten, and panes showing them reload.
 */
export function forgetWhatRunsChanged(before: readonly TaskView[], after: readonly TaskView[]) {
  const sessions = Object.values(useSessionStore.getState().sessions);
  for (const task of finishedRuns(before, after)) {
    for (const change of serverChanges(task.action)) {
      // A vault's session lists its decrypted files, which the task did not see.
      const on = sessions.filter(
        (entry) =>
          !entry.info.vault &&
          (entry.info.savedConnectionId ?? entry.profile?.savedConnectionId) ===
            change.connectionId,
      );
      for (const entry of on) {
        if (change.folder === null) forgetServerListings(entry.info.id);
        else if (change.folder.trim()) forgetListingsWithin(entry.info.id, change.folder.trim());
      }
    }
  }
}
