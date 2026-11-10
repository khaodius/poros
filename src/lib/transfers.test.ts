import { describe, expect, it } from "vitest";
import {
  EMPTY_SNAPSHOT,
  EMPTY_STATS,
  applyUpdate,
  isSettled,
  jobProgress,
  jobsIn,
  missesUpdates,
  replayOnList,
  secondsRemaining,
  VIEW_STATES,
} from "./transfers";
import type { JobSnapshot, TransferUpdate } from "./types";

function job(id: number, rank: string, overrides: Partial<JobSnapshot> = {}): JobSnapshot {
  return {
    id,
    rank,
    sessionId: "s",
    direction: "upload",
    kind: "file",
    name: `file-${id}`,
    source: `/local/file-${id}`,
    target: `/remote/file-${id}`,
    targetDirectory: "/remote",
    size: 100,
    transferred: 0,
    speed: 0,
    connections: 0,
    state: "queued",
    attempts: 0,
    ...overrides,
  };
}

function update(sequence: number, jobs: JobSnapshot[]): TransferUpdate {
  return { sequence, jobs, removed: [], changedDirectories: [], stats: EMPTY_STATS };
}

describe("transfer snapshots", () => {
  it("keeps jobs in rank order and only reorders when needed", () => {
    let snapshot = applyUpdate(EMPTY_SNAPSHOT, {
      sequence: 1,
      jobs: [job(1, "80000002"), job(2, "80000001"), job(3, "8000000100000001")],
      removed: [],
      changedDirectories: [],
      stats: EMPTY_STATS,
    });
    expect(snapshot.order).toEqual([2, 3, 1]);

    const progressed = applyUpdate(snapshot, {
      sequence: 2,
      jobs: [job(1, "80000002", { state: "running", transferred: 50 })],
      removed: [],
      changedDirectories: [],
      stats: EMPTY_STATS,
    });
    expect(progressed.order).toBe(snapshot.order);
    expect(jobProgress(progressed.jobs.get(1)!)).toBe(0.5);

    snapshot = applyUpdate(progressed, {
      sequence: 3,
      jobs: [job(4, "7fffffff", { state: "failed" })],
      removed: [3],
      changedDirectories: [],
      stats: EMPTY_STATS,
    });
    expect(snapshot.order).toEqual([4, 2, 1]);
    expect(jobsIn(snapshot, VIEW_STATES.queue).map((entry) => entry.id)).toEqual([2, 1]);
  });

  it("ignores updates it already holds and notices missed ones", () => {
    const running = update(5, [job(1, "80000001", { state: "running", transferred: 99 })]);
    const done = update(6, [job(1, "80000001", { state: "done", transferred: 100 })]);
    const snapshot = applyUpdate(applyUpdate(EMPTY_SNAPSHOT, running), done);
    // Running arrives again late, after the job has finished: the job stays done.
    expect(applyUpdate(snapshot, running)).toBe(snapshot);
    expect(snapshot.jobs.get(1)?.state).toBe("done");

    expect(missesUpdates(snapshot, update(7, []))).toBe(false);
    expect(missesUpdates(snapshot, update(9, []))).toBe(true);
  });

  it("merges the updates that arrived while the list was read", () => {
    const list = {
      sequence: 10,
      jobs: [job(1, "80000001", { state: "running" }), job(2, "80000002")],
      stats: EMPTY_STATS,
    };
    // Sent before the list was read, then delivered after it.
    const stale = update(9, [job(1, "80000001", { state: "running", transferred: 10 })]);
    const finished = update(11, [job(1, "80000001", { state: "done", transferred: 100 })]);
    const started = update(12, [job(2, "80000002", { state: "running" })]);

    const replayed = replayOnList(list, [stale, started, finished]);
    expect(replayed.complete).toBe(true);
    expect(replayed.snapshot.sequence).toBe(12);
    expect(replayed.snapshot.jobs.get(1)?.state).toBe("done");
    expect(replayed.snapshot.jobs.get(2)?.state).toBe("running");

    const gap = replayOnList(list, [started]);
    expect(gap.complete).toBe(false);
  });

  it("tells when the queue has settled", () => {
    expect(isSettled(EMPTY_STATS)).toBe(true);
    expect(isSettled({ ...EMPTY_STATS, counts: { ...EMPTY_STATS.counts, running: 1 } })).toBe(
      false,
    );
    expect(isSettled({ ...EMPTY_STATS, counts: { ...EMPTY_STATS.counts, failed: 3 } })).toBe(true);
  });

  it("estimates the time left", () => {
    expect(secondsRemaining({ ...EMPTY_STATS, remainingBytes: 1000, uploadSpeed: 300 })).toBe(4);
    expect(secondsRemaining({ ...EMPTY_STATS, remainingBytes: 1000 })).toBeNull();
  });
});
