import { describe, expect, it } from "vitest";
import {
  EMPTY_SNAPSHOT,
  EMPTY_STATS,
  applyUpdate,
  jobProgress,
  jobsIn,
  secondsRemaining,
  VIEW_STATES,
} from "./transfers";
import type { JobSnapshot } from "./types";

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

describe("transfer snapshots", () => {
  it("keeps jobs in rank order and only reorders when needed", () => {
    let snapshot = applyUpdate(EMPTY_SNAPSHOT, {
      jobs: [job(1, "80000002"), job(2, "80000001"), job(3, "8000000100000001")],
      removed: [],
      changedDirectories: [],
      stats: EMPTY_STATS,
    });
    expect(snapshot.order).toEqual([2, 3, 1]);

    const progressed = applyUpdate(snapshot, {
      jobs: [job(1, "80000002", { state: "running", transferred: 50 })],
      removed: [],
      changedDirectories: [],
      stats: EMPTY_STATS,
    });
    expect(progressed.order).toBe(snapshot.order);
    expect(jobProgress(progressed.jobs.get(1)!)).toBe(0.5);

    snapshot = applyUpdate(progressed, {
      jobs: [job(4, "7fffffff", { state: "failed" })],
      removed: [3],
      changedDirectories: [],
      stats: EMPTY_STATS,
    });
    expect(snapshot.order).toEqual([4, 2, 1]);
    expect(jobsIn(snapshot, VIEW_STATES.queue).map((entry) => entry.id)).toEqual([2, 1]);
  });

  it("estimates the time left", () => {
    expect(secondsRemaining({ ...EMPTY_STATS, remainingBytes: 1000, uploadSpeed: 300 })).toBe(4);
    expect(secondsRemaining({ ...EMPTY_STATS, remainingBytes: 1000 })).toBeNull();
  });
});
