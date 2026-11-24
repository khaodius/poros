// Turns transfers in the queue, or files selected in a pane, into a scheduled transfer task
// to finish in the scheduler dialog. Pure: what is known about each session is handed in.

import { newTask, transferAction } from "./schedule";
import type { Direction, ExistsAction, JobSnapshot, ScheduledTask, TaskItem } from "./types";
import { t } from "../i18n";

/** What a draft needs to know about a session. */
export interface SessionFacts {
  label: string;
  /** The saved connection it was opened from, which a task signs in with. */
  savedConnectionId?: string;
  /** An unlocked vault, which a task cannot unlock on its own. */
  vault: boolean;
}

export type TaskDraft = { task: ScheduledTask } | { problem: string };

type PathStyle = "posix" | "windows";

/** Local paths may be Windows ones; server paths always use `/`. */
function localStyle(path: string): PathStyle {
  return /^[A-Za-z]:[\\/]/.test(path) || path.startsWith("\\\\") ? "windows" : "posix";
}

function parts(path: string, style: PathStyle): string[] {
  return path.split(style === "windows" ? /[\\/]+/ : /\/+/).filter(Boolean);
}

function lastPart(path: string, style: PathStyle): string {
  const all = parts(path, style);
  return all[all.length - 1] ?? "";
}

/** `path` without its last `count` parts. */
function ancestor(path: string, count: number, style: PathStyle): string {
  let current = path;
  for (let step = 0; step < count; step += 1) {
    const trimmed = current.replace(style === "windows" ? /[\\/]+$/ : /\/+$/, "");
    const cut =
      style === "windows"
        ? Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"))
        : trimmed.lastIndexOf("/");
    current = cut <= 0 ? trimmed.slice(0, cut + 1) : trimmed.slice(0, cut);
  }
  return current;
}

function sameStart(path: string[], start: string[]): boolean {
  return start.every((part, index) => path[index] === part);
}

/**
 * The items and the one folder they go into. A folder that was queued has left the list for
 * the files it held, each of which goes into a folder below the destination; such a file stands
 * for the folder it came in, so scheduling them all schedules the folder.
 */
function itemsAndDestination(
  jobs: JobSnapshot[],
): { items: TaskItem[]; destination: string } | null {
  const direction = jobs[0].direction;
  const targetStyle = direction === "download" ? localStyle(jobs[0].targetDirectory) : "posix";
  const sourceStyle = direction === "upload" ? localStyle(jobs[0].source) : "posix";
  const shallowest = jobs.reduce((best, job) =>
    parts(job.targetDirectory, targetStyle).length < parts(best.targetDirectory, targetStyle).length
      ? job
      : best,
  );
  const destination = shallowest.targetDirectory;
  const destinationParts = parts(destination, targetStyle);
  const items: TaskItem[] = [];
  for (const job of jobs) {
    const below = parts(job.targetDirectory, targetStyle);
    if (!sameStart(below, destinationParts)) return null;
    const levels = below.length - destinationParts.length;
    const item: TaskItem =
      levels === 0
        ? { path: job.source, isDir: job.kind === "folder" }
        : { path: ancestor(job.source, levels, sourceStyle), isDir: true };
    // A file renamed on the way, or a source that does not mirror the target.
    if (levels > 0 && lastPart(item.path, sourceStyle) !== below[destinationParts.length]) {
      return null;
    }
    if (!items.some((existing) => existing.path === item.path)) items.push(item);
  }
  return { items, destination };
}

function connectionOf(facts: SessionFacts | undefined): { id: string } | { problem: string } {
  if (!facts) {
    return {
      problem: t("schedule.porosNoLongerKnowsThe"),
    };
  }
  if (facts.vault) {
    return {
      problem: t("schedule.filesInAnUnlockedVault"),
    };
  }
  if (!facts.savedConnectionId) {
    return {
      problem: t("schedule.saveTheConnectionToFirst", { label: facts.label }),
    };
  }
  return { id: facts.savedConnectionId };
}

/** What a scheduled copy does with a file already there: as the settings say, or overwrite. */
export function taskExistsAction(fromSettings: ExistsAction): ExistsAction {
  return fromSettings === "ask" ? "overwrite" : fromSettings;
}

/** A transfer task that repeats the selected transfers. */
export function taskFromJobs(
  jobs: JobSnapshot[],
  facts: (sessionId: string) => SessionFacts | undefined,
  exists: ExistsAction,
): TaskDraft {
  if (jobs.length === 0) return { problem: t("schedule.selectTheTransfersToSchedule") };
  const [first] = jobs;
  const together = jobs.every(
    (job) =>
      job.direction === first.direction &&
      job.sessionId === first.sessionId &&
      job.sourceSessionId === first.sourceSessionId,
  );
  if (!together) {
    return {
      problem: t("schedule.selectTransfersThatGoThe"),
    };
  }
  const target = connectionOf(facts(first.sessionId));
  if ("problem" in target) return target;
  let sourceConnectionId: string | null = null;
  if (first.direction === "relay") {
    const source = connectionOf(first.sourceSessionId ? facts(first.sourceSessionId) : undefined);
    if ("problem" in source) return source;
    sourceConnectionId = source.id;
  }
  const placed = itemsAndDestination(jobs);
  if (!placed) {
    return {
      problem: t("schedule.theSelectedTransfersGoTo"),
    };
  }
  return {
    task: newTask(
      transferAction(target.id, first.direction, {
        sourceConnectionId,
        items: placed.items,
        destination: placed.destination,
        exists: taskExistsAction(exists),
      }),
    ),
  };
}

export interface SelectedFiles {
  direction: Extract<Direction, "upload" | "download">;
  entries: TaskItem[];
  /**
   * The server: the tab's own for a download, the server tab looked at last for an upload.
   * Absent when there is none, and the task's connection is chosen in the dialog.
   */
  server?: SessionFacts;
  /** The folder on the other side, when a tab there shows one. */
  destination?: string;
  exists: ExistsAction;
}

/** A transfer task for files selected in a tab. */
export function taskFromFiles({
  direction,
  entries,
  server,
  destination,
  exists,
}: SelectedFiles): TaskDraft {
  if (entries.length === 0) return { problem: t("schedule.selectTheFilesToSchedule") };
  let connectionId = "";
  let folder = destination ?? "";
  if (direction === "download") {
    const connection = connectionOf(server);
    if ("problem" in connection) return connection;
    connectionId = connection.id;
  } else {
    // An upload names its server in the dialog when the one in view cannot be used.
    const connection = server ? connectionOf(server) : null;
    if (connection && "id" in connection) connectionId = connection.id;
    else folder = "";
  }
  return {
    task: newTask(
      transferAction(connectionId, direction, {
        items: entries.map((entry) => ({ path: entry.path, isDir: entry.isDir ?? false })),
        destination: folder,
        exists: taskExistsAction(exists),
      }),
    ),
  };
}

/** Why a tab's files cannot be scheduled for download, in a few words for a menu; or null. */
export function downloadBlocker(server: SessionFacts | undefined): string | null {
  if (!server) return "no server";
  if (server.vault) return "not in a vault";
  if (!server.savedConnectionId) return "save the connection first";
  return null;
}
