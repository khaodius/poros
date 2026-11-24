import type { MessageArea } from "../catalog";

export const openWith = {
  // components/ExternalEditDialogs
  isGoneFromTheServer: "{name} is gone from the server",
  changedOnTheServer: "{name} changed on the server",
  modified: "Modified",
  onTheServer: "On the server",
  yourSavedCopy: "Your saved copy",
  keepBothUploadsYourCopy:
    "Keep both uploads your copy beside the server's, under a name not taken there.",
  comparingReadsTheServersCopy: "Comparing reads the server's copy over SFTP",
  dontUpload: "Don't upload",
  uploadItAgain: "Upload it again",
  keepBoth: "Keep both",
  upload: "Upload {name}?",
  uploadEverySaveWithoutAsking: "Upload every save without asking",
  notNow: "Not now",
  defaultProgram: "Default program",
  setUpForFilesLike: "Set up for files like this one",
  builtInEditor: "Built-in editor",
  editTheTextInA: "Edit the text in a tab here",
  chooseAProgram: "Choose a program...",
  anyProgramOnThisComputer: "Any program on this computer",
  isAProgram: "{name} is a program",
  open: "Open {name}",
  programs: "Programs",
  filesNotUploadedLastTime: "Files not uploaded last time",
  later: "Later",
  lock: "Lock {label}?",
  close: "Close {label}?",
  theirSavesKeepBeingUploaded: "Their saves keep being uploaded after the tab closes.",
  stopEditingAndClose: "Stop editing and close",
  uploadSavedFilesBeforeQuitting: "Upload saved files before quitting?",
  someCouldNotBeUploaded: "Some could not be uploaded.",
  quitWithoutUploading: "Quit without uploading",
  deleted: "Deleted",
  compare: "Compare",
  compareSftpOnly: "Compare (SFTP only)",
  lockAnyway: "Lock anyway",
  closeAnyway: "Close anyway",
  uploading2: "Uploading...",
  uploadAndLock: "Upload and lock",
  uploadAndClose: "Upload and close",
  keepEditing: "Keep editing",
  uploadAndQuit: "Upload and quit",
  uploadedAs: "Uploaded {name} as {renamedTo}",
  uploadedName: "Uploaded {name}",
  couldNotUploadKept:
    "Could not upload {name}: {message}. The saved file is kept; try again from the status bar.",
  couldNotOpen: "Could not open {name}: {message}",
  someoneDeletedIt:
    "Someone deleted it from {server} since you opened it. Your save has not been uploaded yet.",
  someoneChangedIt:
    "Someone changed it on {server} since you opened it. Your save has not been uploaded yet.",
  serverChangedThere: "{server}, changed there",
  savedInProgramUploadNow: "{name} was saved in {program}. Upload it to {server} now?",
  savedUploadNow: "{name} was saved. Upload it to {server} now?",
  theProgramThisSystemOpensFilesWith: "The program this system opens {extension} files with",
  theProgramThisSystemOpensSuchFilesWith: "The program this system opens such files with",
  holdsAProgramNotOpened:
    "{name} holds a program or a script, so it was not opened. Poros does not run files it opens: pick an editor to look at it in.",
  isAProgramOrAScript:
    "{name} is a program or a script. Poros does not run files it opens: pick an editor to look at it in.",
  whenPorosLastClosed: {
    one: "When Poros last closed, this file was saved in another program but not uploaded. Upload once its server is connected, or discard the changes. The status bar keeps the list.",
    other:
      "When Poros last closed, these files were saved in another program but not uploaded. Upload once its server is connected, or discard the changes. The status bar keeps the list.",
  },
  copiesFromVaultsStayUnencrypted:
    "Copies of files from vaults stay unencrypted on this computer until then.",
  filesFromThisVaultOpen: {
    one: "{count} file from this vault is open in other programs:",
    other: "{count} files from this vault are open in other programs:",
  },
  filesFromThisServerOpen: {
    one: "{count} file from this server is open in other programs:",
    other: "{count} files from this server are open in other programs:",
  },
  couldNotBeUploadedLockingWipes: {
    one: "This one could not be uploaded. Locking anyway wipes the changes in it.",
    other: "These could not be uploaded. Locking anyway wipes the changes in them.",
  },
  couldNotBeUploadedClosingWipes: {
    one: "This one could not be uploaded. Closing anyway wipes the changes in vault files.",
    other: "These could not be uploaded. Closing anyway wipes the changes in vault files.",
  },
  couldNotBeUploadedClosingLeaves: {
    one: "This one could not be uploaded. Closing anyway leaves them in the status bar's list, to upload later.",
    other:
      "These could not be uploaded. Closing anyway leaves them in the status bar's list, to upload later.",
  },
  lockingEndsEditing: {
    one: "Locking the vault ends editing the file from it and wipes the working copies. What was saved is uploaded first.",
    other:
      "Locking the vault ends editing the files from it and wipes the working copies. What was saved is uploaded first.",
  },
  closingLocksVaultEndsEditing: {
    one: "Closing the tab locks its vault, which ends editing the file from it and wipes the working copies. What was saved is uploaded first.",
    other:
      "Closing the tab locks its vault, which ends editing the files from it and wipes the working copies. What was saved is uploaded first.",
  },
  savedButNotReachedTheServer: {
    one: "This file was saved in other programs but has not reached the server:",
    other: "These files were saved in other programs but have not reached the server:",
  },
  quittingWithoutUploadingKeepsThem:
    "Quitting without uploading keeps them, and Poros offers to upload them when it starts again.",
  workingCopiesOfVaultFilesAreWiped: "Working copies of vault files are wiped.",

  // components/ExternalEditorSettings
  openWith: "Open with",
  openCtrlEnterUsesThe:
    "Open (Ctrl+Enter) uses the program set up for a file's kind, else the system's own. A server file opens from a copy on this computer, which is uploaded each time the program saves it.",
  uploadEachSaveWithoutAsking: "Upload each save without asking",
  uploadEachSaveWithoutAskingHint: "Off: asks before every upload.",
  keepCopiesAfterTheProgram: "Keep copies after the program closes",
  keepCopiesAfterTheProgramHint:
    "Counted from the last save or upload. Copies of files in vaults are wiped at once.",
  removeProgram: "Remove program",
  remove: "Remove {programLabel}",
  program: "Program",
  arguments: "Arguments",
  defaultFor: "Default for",
  filesThisProgramOpensBy: "Files this program opens by default",
  psdXcf: "*.psd, *.xcf",
  nameOfTheNewProgram: "Name of the new program",
  newProgram: "New program",
  inTheArgumentsStandsFor:
    "{placeholder} in the arguments stands for the file, and the file goes last when it is left out. Quotes keep spaces in one argument. Files matching a pattern under Default for open in that program.",

  // components/ExternalEditsStatus
  filesOpenInOtherPrograms: "Files open in other programs",
  stopEditing: "Stop editing {name}?",
  stopAndDiscard: "Stop and discard",
  openInOtherPrograms: "Open in other programs",
  settingsForOpeningFiles: "Settings for opening files",
  noFilesAreOpenIn: "No files are open in other programs. Right-click a file and pick Open with.",
  inAVault: "In a vault",
  uploaded: "uploaded {uploadTime}",
  of: "{transferred} of {total}",
  decideWhatToUpload: "Decide what to upload...",
  openInAnEditor: "Open in an editor...",
  uploadNow: "Upload now",
  uploadNow2: "Upload {name} now",
  openAgain: "Open again",
  openAgain2: "Open {name} again",
  showTheWorkingCopysFolder: "Show the working copy's folder",
  showTheFolderOf: "Show the folder of {name}",
  stopEditingAndDeleteThe: "Stop editing and delete the working copy",
  stopEditing2: "Stop editing {name}",
  savedButNotUploadedWhen: "Saved but not uploaded when Poros last closed",
  saved: "saved {date}",
  thisCopyOfAVault:
    "This copy of a vault file is not encrypted on this computer. Upload or discard it.",
  uploadTo: "Upload to {label}",
  connectToFirst: "Connect to {server} first",
  upload2: "Upload {name}",
  discardTheSavedChanges: "Discard the saved changes",
  discard: "Discard {name}",
  discard2: "Discard {name}?",
  notEditingAnyFiles: "Not editing any files",
  eachSaveIsUploadedAs: "Each save is uploaded as soon as the program has written it.",
  savesWaitForYouTo: "Saves wait for you to upload them.",
  changesSavedInProgramNotReached:
    "Changes saved in {program} have not reached {server}. Stopping deletes the working copy with them.",
  changesSavedInTheProgramNotReached:
    "Changes saved in the program have not reached {server}. Stopping deletes the working copy with them.",
  connectAndUnlockToUpload: "Connect to {server} and unlock its vault to upload it.",
  connectToUpload: "Connect to {server} to upload it.",
  neverReachedDiscardingWipes:
    "The changes saved to it never reached {server}. Discarding wipes them.",
  neverReachedDiscardingDeletes:
    "The changes saved to it never reached {server}. Discarding deletes them.",

  // components/openWithMenu
  openIn: "Open in {programLabel}",
  setUpPrograms: "Set up programs...",

  // lib/externalEditor
  downloading: "Downloading",
  watchingForSaves: "Watching for saves",
  savedNotUploaded: "Saved, not uploaded",
  waitingToUpload: "Waiting to upload",
  changedOnTheServer2: "Changed on the server",
  uploading: "Uploading",
  uploadFailed: "Upload failed",
  notOpenedItIsAProgram: "Not opened: it is a program",
  openingNamePercent: "Opening {name}, {percent}",
  uploadingName: "Uploading {name}",
  nameNotOpenedItIsAProgram: "{name}: not opened: it is a program",
  nameChangedOnTheServer: "{name}: changed on the server",
  nameUploadFailed: "{name}: upload failed",
  editedFilesNeedAttention: {
    one: "{count} edited file needs attention",
    other: "{count} edited files need attention",
  },
  editingName: "Editing {name}",
  editingFiles: { one: "Editing {count} file", other: "Editing {count} files" },
  filesNotUploaded: { one: "{count} file not uploaded", other: "{count} files not uploaded" },

  // state/externalEditStore
  isOpenAlreadyItsWorking: "{name} is open already. Its working copy opens again.",
  chooseAProgram2: "Choose a program",
} as const satisfies MessageArea;
