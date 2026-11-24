import type { MessageArea } from "../catalog";

export const transfers = {
  // components/TransferView
  copySourcePaths: "Copy source paths",
  copySourcePath: "Copy source path",
  moveToTop: "Move to top",
  moveToBottom: "Move to bottom",
  progress: "Progress",
  speed: "Speed",
  status: "Status",
  to: "{source}\nto {target}",

  // state/transferActions
  nothingToTransferOnlyRegular:
    "Nothing to transfer: only regular files and folders can be copied.",
  queuedOfItemsTo: {
    one: "Queued {VERBS} of {count} item to {where}",
    other: "Queued {VERBS} of {count} items to {where}",
  },
  unlockTheVaultToCopy: "Unlock the vault to copy files into it.",
  couldNotCheckWhetherThis: "Could not check whether this folder is a vault.",
  isALockedVaultOpen: "{baseName} is a locked vault. Open it to copy files into it.",
  dropLocalFilesOnA: "Drop local files on a server tab to upload them.",
  to2: "{verb} to {folder}",
  connectToAServerTo: "Connect to a server to upload to it.",
  openALocalTabTo: "Open a local tab to download into.",
  dropFilesOnAServer: "Drop files on a server tab to upload them.",
  local: "Local",
  // components/TransferView
  dragFilesBetweenTabsOr: "Drag files between tabs, or use Upload and Download in a file's menu.",
  transfersThatFailAfterTheir: "Transfers that fail after their retries end up here.",
  finishedTransfersAreListedHere: "Finished transfers are listed here.",

  // lib/progressBars: the text beside or on a bar
  sizeOfSize: "{transferred} of {size}",
  done: "Done",
  timeLeft: "{duration} left",
  // components/TransferView
  scheduleThese: "Schedule these...",
  scheduleThis: "Schedule this...",
} as const satisfies MessageArea;
