import type { MessageArea } from "../catalog";

export const transfers = {
  // components/TransferLookSettings
  timeLeft: "{duration} left",

  // components/TransferView
  copySourcePaths: "Copy source paths",
  copySourcePath: "Copy source path",
  moveToTop: "Move to top",
  moveToBottom: "Move to bottom",
  progress: "Progress",
  speed: "Speed",
  status: "Status",
  dragFilesBetweenTabsOr: "Drag files between tabs, or use Upload and Download in a file's menu.",
  transfersThatFailAfterTheir: "Transfers that fail after their retries end up here.",
  finishedTransfersAreListedHere: "Finished transfers are listed here.",
  done: "Done",
  scheduleThese: "Schedule these...",
  scheduleThis: "Schedule this...",
  connectionLostWaitingForTheServer: "Connection lost, waiting for the server",
  waitingToRetry: "Waiting to retry (attempt {attempt})",
  queued: "Queued",
  listingFolder: "Listing folder",
  updatingWithRsyncSent: "Updating with rsync, {size} sent",
  copyingDirectlyBetweenTheServers: "Copying directly between the servers",
  transferringOnConnections: {
    one: "Transferring on {count} connection",
    other: "Transferring on {count} connections",
  },
  transferring: "Transferring",
  paused: "Paused",
  alreadyExistsWaitingForYourChoice: "Already exists, waiting for your choice",
  doneCopiedDirectlyBetweenTheServers: "Done, copied directly between the servers",
  doneWithRsyncSent: "Done with rsync, sent {sent} of {size}",
  skippedBecause: "Skipped: {error}",
  skipped: "Skipped",
  failedBecause: "Failed: {error}",
  failedUnknownError: "Failed: unknown error",
  rowTitle: "{source}\nto {target}",
  rowTitleQueuedByTask: "{source}\nto {target}\nQueued by the task {task}",

  // lib/progressBars
  sizeOfSize: "{transferred} of {size}",

  // state/transferActions
  nothingToTransferOnlyRegular:
    "Nothing to transfer: only regular files and folders can be copied.",
  unlockTheVaultToCopy: "Unlock the vault to copy files into it.",
  couldNotCheckWhetherThis: "Could not check whether this folder is a vault.",
  isALockedVaultOpen: "{baseName} is a locked vault. Open it to copy files into it.",
  dropLocalFilesOnA: "Drop local files on a server tab to upload them.",
  copyingTo: "Copying to {folder}",
  movedTo: "Moved to {folder}",
  copiedTo: "Copied to {folder}",
  connectToAServerTo: "Connect to a server to upload to it.",
  openALocalTabTo: "Open a local tab to download into.",
  dropFilesOnAServer: "Drop files on a server tab to upload them.",
  local: "Local",
} as const satisfies MessageArea;
