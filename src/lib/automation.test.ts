import { describe, expect, it } from "vitest";
import { describeQueueFinished, queueEnvironment } from "./automation";

const finished = {
  done: 12,
  failed: 0,
  skipped: 0,
  bytes: 3 * 1024 * 1024,
  elapsedMillis: 125_000,
  paused: 0,
};

describe("describeQueueFinished", () => {
  it("sums up a clean run", () => {
    expect(describeQueueFinished(finished)).toEqual({
      title: "Transfers finished",
      body: "12 files transferred (3.0 MB) in 2m 5s",
    });
  });

  it("mentions failures and skipped files", () => {
    const summary = describeQueueFinished({ ...finished, done: 1, failed: 2, skipped: 3 });
    expect(summary.title).toBe("Transfers finished with errors");
    expect(summary.body).toBe("1 file transferred (3.0 MB) in 2m 5s, 2 failed, 3 skipped");
  });
});

describe("queueEnvironment", () => {
  it("passes the totals as text", () => {
    expect(queueEnvironment({ ...finished, failed: 1 })).toEqual({
      POROS_FILES_DONE: "12",
      POROS_FILES_FAILED: "1",
      POROS_FILES_SKIPPED: "0",
      POROS_BYTES: String(3 * 1024 * 1024),
    });
  });
});
