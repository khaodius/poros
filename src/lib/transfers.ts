import type { JobSnapshot, JobState, TransferList, TransferStats, TransferUpdate } from "./types";

export interface TransferSnapshot {
  jobs: ReadonlyMap<number, JobSnapshot>;
  /** Job ids in queue order. */
  order: readonly number[];
  stats: TransferStats;
}

export const EMPTY_STATS: TransferStats = {
  counts: { queued: 0, running: 0, paused: 0, conflict: 0, done: 0, skipped: 0, failed: 0 },
  remainingBytes: 0,
  uploadSpeed: 0,
  downloadSpeed: 0,
  queuePaused: false,
};

export const EMPTY_SNAPSHOT: TransferSnapshot = { jobs: new Map(), order: [], stats: EMPTY_STATS };

export type TransferViewKind = "queue" | "failed" | "completed";

/** The job states each transfer list shows, in the order the queue lists them. */
export const VIEW_STATES: Record<TransferViewKind, readonly JobState[]> = {
  queue: ["running", "conflict", "queued", "paused"],
  failed: ["failed"],
  completed: ["done", "skipped"],
};

function byRank(jobs: ReadonlyMap<number, JobSnapshot>): number[] {
  return [...jobs.values()]
    .sort((left, right) => (left.rank < right.rank ? -1 : left.rank > right.rank ? 1 : 0))
    .map((job) => job.id);
}

export function snapshotFromList(list: TransferList): TransferSnapshot {
  const jobs = new Map(list.jobs.map((job) => [job.id, job]));
  return { jobs, order: byRank(jobs), stats: list.stats };
}

/** Merges a progress event; the order is only rebuilt when jobs come, go or move. */
export function applyUpdate(current: TransferSnapshot, update: TransferUpdate): TransferSnapshot {
  if (update.jobs.length === 0 && update.removed.length === 0) {
    return { ...current, stats: update.stats };
  }
  const jobs = new Map(current.jobs);
  let reorder = update.removed.length > 0;
  for (const id of update.removed) jobs.delete(id);
  for (const job of update.jobs) {
    if (jobs.get(job.id)?.rank !== job.rank) reorder = true;
    jobs.set(job.id, job);
  }
  return { jobs, order: reorder ? byRank(jobs) : current.order, stats: update.stats };
}

export function jobsIn(snapshot: TransferSnapshot, states: readonly JobState[]): JobSnapshot[] {
  const wanted = new Set(states);
  const result: JobSnapshot[] = [];
  for (const id of snapshot.order) {
    const job = snapshot.jobs.get(id);
    if (job && wanted.has(job.state)) result.push(job);
  }
  return result;
}

export function jobProgress(job: JobSnapshot): number {
  if (job.state === "done") return 1;
  if (job.kind === "folder" || job.size <= 0) return 0;
  return Math.min(1, job.transferred / job.size);
}

/** Seconds until the queue drains at the current speed, or null when it is not moving. */
export function secondsRemaining(stats: TransferStats): number | null {
  const speed = stats.uploadSpeed + stats.downloadSpeed;
  if (speed <= 0 || stats.remainingBytes <= 0) return null;
  return Math.ceil(stats.remainingBytes / speed);
}
