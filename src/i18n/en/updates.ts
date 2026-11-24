import type { MessageArea } from "../catalog";

export const updates = {
  // components/UpdateDialog
  updateAvailable: "Update available",
  poros: "Poros",
  isAvailableYouHave: "is available. You have {version}.",
  porosKeepsWorkingWhileThe:
    "Poros keeps working while the update downloads, and asks before it restarts.",
  skipThisVersion: "Skip this version",
  later: "Later",
  update: "Update",

  // components/UpdateNotice
  hideTheStatusBarShows: "Hide; the status bar shows it again",
  restartPorosToSwitchTo:
    "Restart Poros to switch to it now, or it takes effect the next time you open Poros.",
  restartNow: "Restart now",

  // state/updateStore
  transfersInTheQueueWill: {
    one: "{count} transfer in the queue will stop when Poros restarts",
    other: "{count} transfers in the queue will stop when Poros restarts",
  },
  unsavedChangesInFilesWill: {
    one: "Unsaved changes in {count} file will be lost when Poros restarts.",
    other: "Unsaved changes in {count} files will be lost when Poros restarts.",
  },
  // components/UpdateNotice
  of: "{size} of {size2}",
} as const satisfies MessageArea;
