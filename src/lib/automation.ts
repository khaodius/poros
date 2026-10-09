// What Poros says and does when the transfer queue finishes.

import { formatDuration, formatSize, pluralize } from "./format";
import type { WhenDoneAction } from "./settings";
import type { QueueFinished } from "./types";

export const WHEN_DONE_LABELS: Record<WhenDoneAction, string> = {
  nothing: "Do nothing",
  close: "Close Poros",
  lock: "Lock the computer",
  sleep: "Sleep",
  hibernate: "Hibernate",
  logOff: "Log off",
  shutDown: "Shut down",
  runCommand: "Run a command",
};

/** Actions that wait for a countdown, so the user can still stop them. */
export type CountdownAction = Exclude<WhenDoneAction, "nothing" | "runCommand">;

export const COUNTDOWN_TEXT: Record<CountdownAction, { title: string; confirm: string }> = {
  close: { title: "Closing Poros", confirm: "Close now" },
  lock: { title: "Locking the computer", confirm: "Lock now" },
  sleep: { title: "Putting the computer to sleep", confirm: "Sleep now" },
  hibernate: { title: "Hibernating the computer", confirm: "Hibernate now" },
  logOff: { title: "Logging off", confirm: "Log off now" },
  shutDown: { title: "Shutting down the computer", confirm: "Shut down now" },
};

export const COUNTDOWN_SECONDS = 30;

export function describeQueueFinished(finished: QueueFinished): { title: string; body: string } {
  const title = finished.failed > 0 ? "Transfers finished with errors" : "Transfers finished";
  const totals = [
    `${pluralize(finished.done, "file")} transferred (${formatSize(finished.bytes)}) in ${formatDuration(finished.elapsedMillis / 1000)}`,
  ];
  if (finished.failed > 0) totals.push(`${finished.failed} failed`);
  if (finished.skipped > 0) totals.push(`${finished.skipped} skipped`);
  return { title, body: totals.join(", ") };
}

/** Handed to the command run when the queue finishes. */
export function queueEnvironment(finished: QueueFinished): Record<string, string> {
  return {
    POROS_FILES_DONE: String(finished.done),
    POROS_FILES_FAILED: String(finished.failed),
    POROS_FILES_SKIPPED: String(finished.skipped),
    POROS_BYTES: String(finished.bytes),
  };
}
