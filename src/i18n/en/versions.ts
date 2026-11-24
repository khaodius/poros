import type { MessageArea } from "../catalog";

export const versions = {
  // components/VersionsDialog
  restoreThisVersion: "Restore this version",
  downloadThisVersion: "Download this version",
  openInTheEditor: "Open in the editor",
  compareWithTheCurrentFile: "Compare with the current file",
  downloadingTheVersionFromTo:
    "Downloading the version from {when} to {folder}. The transfer queue shows how it goes.",
  versionsOf: "Versions of {name}",
  loadingVersions: "Loading versions...",
  turnOnVersioningForThe:
    "This bucket keeps no versions. Turn on versioning for the bucket to keep the files it replaces and deletes.",
  noVersionsOfThisFile: "No versions of this file were found.",
  saved: "Saved",
  status: "Status",
  storageClass: "Storage class",
  etag: "ETag",
  actions: "Actions",
  version: "Version {versionId}",
  entries: { one: "{count} entry", other: "{count} entries" },
  restoreVersion: "Restore version",
  restore: "Restore",
  makeTheVersionOf: "Make the version of {name} from {when} the current file again?",
  thisBringsTheDeletedFile: "This brings the deleted file back. The delete stays in the history.",
  theCurrentContentStaysIn:
    "The current content stays in the history, so it can be restored in turn.",
  removeDeleteMarker: "Remove delete marker",
  deleteVersion: "Delete version",
  removeMarker: "Remove marker",
  removeTheDeleteMarkerOf:
    "Remove the delete marker of {name} from {when} for good? This cannot be undone.",
  deleteTheVersionOf: "Delete the version of {name} from {when} for good? This cannot be undone.",
  deleteForGood: "Delete for good",
  deletedItems: { one: "{count} deleted item", other: "{count} deleted items" },
  everythingDeletedUnderAFolder:
    "Everything deleted under a folder goes too. Files still in it are not touched.",
  andMore: { one: "and {count} more", other: "and {count} more" },
  removeThisDeleteMarker: "Remove this delete marker",
  deleteThisVersionForGood: "Delete this version for good",
  versionFrom: "Version from {when}",
  currentVersion: "Current version",
  isNoLongerDeleted: "{name} is no longer deleted.",
  theseItemsAreNoLongerDeleted: "These items are no longer deleted.",
  removeWithEveryVersion:
    "Remove {what} and every version kept of it for good? This cannot be undone.",

  // components/versionMenu
  versions: "Versions...",
  versionsThisBucketKeepsNone: "Versions (this bucket keeps none)",
  showDeletedFiles: "Show deleted files",
  showDeletedFilesThisBucket: "Show deleted files (this bucket keeps no versions)",
  restoreItems: { one: "Restore {count} item", other: "Restore {count} items" },
  copyPaths: "Copy paths",
  copyPath: "Copy path",
  deleteItemsForGood: {
    one: "Delete {count} item for good",
    other: "Delete {count} items for good",
  },

  // hooks/usePaneVersions
  deletedFilesCannotBeShown: "Deleted files cannot be shown: {message}",

  // lib/versions
  current: "Current",
  deleted: "Deleted",
  deleteMarker: "Delete marker",
  restoredCount: { one: "{count} restored", other: "{count} restored" },
  findingWhatToRestore: "Finding what to restore...",
  restoredFoundSoFar: {
    one: "{count} restored, {total} found so far",
    other: "{count} restored, {total} found so far",
  },
  restoredOfTotal: { one: "{count} of {total} restored", other: "{count} of {total} restored" },
  nothingOfTheFileIsLeft: "Nothing of the file is left after it.",
  theFileComesBackAsItsNewestVersion: "The file comes back as its newest version.",
  theFileStaysDeleted: "The file stays deleted: another delete marker is under this one.",
  thisIsTheOnlyVersion: "This is the only version: the file will be gone.",
  theVersionBeforeItBecomesCurrent: "The version before it becomes the current file.",
  theFileWillShowAsDeleted:
    "The file will show as deleted: the entry before it is a delete marker.",
  unknownTime: "Unknown time",
  versioningIsSuspended:
    "Versioning is suspended on this bucket: what it kept stays, and new writes replace the version marked null.",
  theseKeysMayNotSeeVersioning:
    "These keys may not see whether the bucket keeps versions, so some actions may be refused.",

  // state/versionActions
  downloadAVersionOfTo: "Download a version of {name} to",
  restoringItemsIn: {
    one: "Restoring {count} item in {folder}",
    other: "Restoring {count} items in {folder}",
  },
  restoringNameIn: "Restoring {name} in {folder}",
  deletingItemsForGoodIn: {
    one: "Deleting for good {count} item in {folder}",
    other: "Deleting for good {count} items in {folder}",
  },
  deletingNameForGoodIn: "Deleting for good {name} in {folder}",
  filesCouldNotBeRestored: {
    one: "{count} file could not be restored. The log lists why.",
    other: "{count} files could not be restored. The log lists why.",
  },
  filesCouldNotBeDeleted: {
    one: "{count} file could not be deleted. The log lists why.",
    other: "{count} files could not be deleted. The log lists why.",
  },
} as const satisfies MessageArea;
