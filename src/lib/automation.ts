// What Poros says and does when the transfer queue finishes.

import { formatDuration, formatSize } from "./format";
import type { WhenDoneAction } from "./settings";
import type { QueueFinished } from "./types";
import { t } from "../i18n";

export const WHEN_DONE_LABELS: Record<WhenDoneAction, string> = {
  get nothing() {
    return t("automation.doNothing");
  },
  get close() {
    return t("automation.closePoros");
  },
  get lock() {
    return t("automation.lockTheComputer");
  },
  get sleep() {
    return t("automation.sleep");
  },
  get hibernate() {
    return t("automation.hibernate");
  },
  get logOff() {
    return t("automation.logOff");
  },
  get shutDown() {
    return t("automation.shutDown");
  },
  get runCommand() {
    return t("automation.runACommand");
  },
};

/** Actions that wait for a countdown, so the user can still stop them. */
export type CountdownAction = Exclude<WhenDoneAction, "nothing" | "runCommand">;

export const COUNTDOWN_TEXT: Record<CountdownAction, { title: string; confirm: string }> = {
  close: {
    get title() {
      return t("automation.closingPoros");
    },
    get confirm() {
      return t("automation.confirmCloseNow");
    },
  },
  lock: {
    get title() {
      return t("automation.lockingTheComputer");
    },
    get confirm() {
      return t("automation.confirmLockNow");
    },
  },
  sleep: {
    get title() {
      return t("automation.puttingTheComputerToSleep");
    },
    get confirm() {
      return t("automation.confirmSleepNow");
    },
  },
  hibernate: {
    get title() {
      return t("automation.hibernatingTheComputer");
    },
    get confirm() {
      return t("automation.confirmHibernateNow");
    },
  },
  logOff: {
    get title() {
      return t("automation.loggingOff");
    },
    get confirm() {
      return t("automation.confirmLogOffNow");
    },
  },
  shutDown: {
    get title() {
      return t("automation.shuttingDownTheComputer");
    },
    get confirm() {
      return t("automation.confirmShutDownNow");
    },
  },
};

export const COUNTDOWN_SECONDS = 30;

export function describeQueueFinished(finished: QueueFinished): { title: string; body: string } {
  const title =
    finished.failed > 0
      ? t("automation.transfersFinishedWithErrors")
      : t("automation.transfersFinished");
  const totals = [
    t("automation.filesTransferredIn", {
      count: finished.done,
      size: formatSize(finished.bytes),
      duration: formatDuration(finished.elapsedMillis / 1000),
    }),
  ];
  if (finished.failed > 0) totals.push(t("automation.failedCount", { count: finished.failed }));
  if (finished.skipped > 0) totals.push(t("automation.skippedCount", { count: finished.skipped }));
  return { title, body: totals.join(t("common.listSeparator")) };
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
