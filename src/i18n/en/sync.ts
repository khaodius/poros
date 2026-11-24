import type { MessageArea } from "../catalog";

export const sync = {
  // components/SyncDialog
  chooseTheLocalFolder: "Choose the local folder",
  synchronizeFolders: "Synchronize folders",
  localFolder: "Local folder",
  theFolderOnThisComputer: "The folder on this computer",
  server: "Server",
  noSftpServerConnected: "No SFTP server connected",
  serverFolder: "Server folder",
  homeMeSite: "/home/me/site",
  direction: "Direction",
  rsyncOptions: "rsync options",
  schedule: "Schedule...",
  runThisSynchronizationAtSet: "Run this synchronization at set times",
  fillInBothFoldersOf: "Fill in both folders of a saved connection to schedule this",
  comparingContentsOfFiles: {
    one: "Comparing contents: {compared} of {count} file",
    other: "Comparing contents: {compared} of {count} files",
  },

  // components/SyncPlanList
  rsync: "rsync {arguments}",
  theOptionsAsRsyncArguments: "The options as rsync arguments",
  dryRunRunningThisChanges:
    "Dry run: running this changes nothing and writes what would happen to the log.",
  unchangedSourceFilesWillBe: {
    one: "{count} unchanged source file will be removed as well.",
    other: "{count} unchanged source files will be removed as well.",
  },
  unchangedSourceFiles: {
    one: "{count} unchanged source file",
    other: "{count} unchanged source files",
  },
  includeAll: "Include all",
  action: "Action",
  why: "Why",
  entriesPassedOverBrokenOr: {
    one: "{count} entry passed over: broken or looping links, special files and folders that could not be read",
    other:
      "{count} entries passed over: broken or looping links, special files and folders that could not be read",
  },
  entries: { one: "{count} entry", other: "{count} entries" },
  include: "Include {path}",
  settle: "Settle {path}",
  leaveAsIs: "Leave as is",
  uploadLocalCopy: "Upload local copy",
  downloadServerCopy: "Download server copy",
  files: { one: "{count} file", other: "{count} files" },

  // components/SyncSettings
  rsync2: "rsync",
  rsyncRunsOnTheServer:
    "rsync runs on the server through your SSH connection, so the server needs rsync installed. rsync daemons (rsync:// addresses on port 873) are not supported.",
  useRsync: "Use rsync",
  useRsyncHint:
    "Files that already exist on the other side are updated by sending only the parts that changed. When this is off, or the server has no rsync, files are copied whole over SFTP.",
  forFilesLargerThan: "For files larger than",
  forFilesLargerThanHint: "Smaller files are quicker to copy whole.",
  rsyncOnTheServer: "rsync on the server",
  rsyncOnTheServerHint: "The command that starts it, for servers where it is not on the PATH.",
  folderSynchronization: "Folder synchronization",
  eachSynchronizationStartsWithThese:
    "Each synchronization starts with these options. A saved site's own rsync arguments apply on top, and Synchronize folders can change any of them for one run.",
  typedAsOnRsyncsCommand:
    "Typed as on rsync's command line; they apply over the options above for every synchronization.",

  // lib/sync
  folders: { one: "{count} folder", other: "{count} folders" },
  files2: { one: "{count} file, {size}", other: "{count} files, {size}" },
  deleteItems: { one: "Delete {count} item", other: "Delete {count} items" },
  deletionsHeldBackByThe: {
    one: "{count} deletion held back by the limit",
    other: "{count} deletions held back by the limit",
  },
  emptyFoldersLeftOut: {
    one: "{count} empty folder left out",
    other: "{count} empty folders left out",
  },
  wouldCopyFiles: {
    one: "would copy {count} file ({size})",
    other: "would copy {count} files ({size})",
  },
  deleteItems2: { one: "delete {count} item", other: "delete {count} items" },
  createFolders: { one: "create {count} folder", other: "create {count} folders" },
  queuedFiles: { one: "Queued {count} file ({size})", other: "Queued {count} files ({size})" },
  deletedItems: { one: "deleted {count} item", other: "deleted {count} items" },
  itemsToDeleteOnceCopying: {
    one: "{count} item to delete once copying is done",
    other: "{count} items to delete once copying is done",
  },
  createdFolders: { one: "created {count} folder", other: "created {count} folders" },
  createdLinks: { one: "created {count} link", other: "created {count} links" },
  updatedFiles: { one: "updated {count} file", other: "updated {count} files" },
  removedUnchangedSourceFiles: {
    one: "removed {count} unchanged source file",
    other: "removed {count} unchanged source files",
  },
  changesFailedTheLogHas: {
    one: ". {count} change failed; the log has the details",
    other: ". {count} changes failed; the log has the details",
  },
  // components/SyncPlanList
  all: "All",
  conflicts: "Conflicts",
  willBeRemovedAsWell: "{t} will be removed as well.",
  passedOverBrokenOrLooping:
    "{t} passed over: broken or looping links, special files and folders that could not be read",

  // lib/sync
  localToServer: "Local to server",
  makesTheServerFolderMatch: "Makes the server folder match the local one.",
  serverToLocal: "Server to local",
  makesTheLocalFolderMatch: "Makes the local folder match the server one.",
  bothWays: "Both ways",
  copiesNewFilesEachWay:
    "Copies new files each way; where both sides have a file, the newer one wins.",
  deleteLocal: "Delete local",
  deleteOnServer: "Delete on server",
  conflict: "Conflict",
} as const satisfies MessageArea;
