import type { MessageArea } from "../catalog";

export const drives = {
  // components/DriveStatus
  filesNotUploaded: { one: "{count} file not uploaded", other: "{count} files not uploaded" },
  drivesMountedFromPoros: "Drives mounted from Poros",
  mountedDrives: "Mounted drives",
  driveAtMountPoint: "{label} at {mountPoint}",
  readOnly: "Read-only",
  factAt: "At",
  factShows: "Shows",
  factActivity: "Activity",
  uploads: "Uploads",
  unmount: "Unmount",
  refusedWhileProgramsHaveFiles:
    "Refused while programs have files open on the drive or uploads are pending",
  forceUnmount: "Force unmount",
  unmountsAtOnceProgramsLose:
    "Unmounts at once: programs lose the files they have open, and what is not uploaded yet is kept on this computer",
  notUploaded: "Not uploaded",
  notUploadedKeptOnThis: "Not uploaded, kept on this computer",
  keptFileFacts: "{connection}, {size}",
  keptFileFactsWithError: "{connection}, {size}: {error}",
  saveACopy: "Save a copy...",
  discardForGood: "Discard for good",

  // components/DrivesGoing
  unmountedWhenTheTabCloses: {
    one: "The drive at {paths} is unmounted when the tab closes.",
    other: "The drives at {paths} are unmounted when the tab closes.",
  },
  unmountedWhenTheVaultLocks: {
    one: "The drive at {paths} is unmounted when the vault locks.",
    other: "The drives at {paths} are unmounted when the vault locks.",
  },
  unmountedWhenPorosQuits: {
    one: "The drive at {paths} is unmounted when Poros quits.",
    other: "The drives at {paths} are unmounted when Poros quits.",
  },
  busyDrivesLoseOpenFiles:
    "Programs still have files open on it, or uploads are pending. They lose the open files, and what is not on the server yet is kept on this computer, listed under the drives in the status bar.",
  busyDrivesLoseOpenFilesWithVault:
    "Programs still have files open on it, or uploads are pending. They lose the open files, and what is not on the server yet is kept on this computer, listed under the drives in the status bar. Files kept from a vault are stored unencrypted until you save or discard them.",

  // components/MountDialog
  mountNameAsADrive: "Mount {name} as a drive",
  theServersRoot: "The server's root",
  theVaultsRoot: "The vault's root",
  rootAt: "{root}: {path}",
  whatProgramsSaveServer:
    "What programs save is uploaded when they close the file. The drive is unmounted when the tab closes or Poros quits.",
  whatProgramsSaveVault:
    "What programs save is uploaded when they close the file, and held unencrypted on this computer until then. The drive is unmounted when the tab closes, the vault locks or Poros quits.",
  mountAsDrive: "Mount as drive",
  theConnectionHasClosed: "The connection has closed.",
  driveLetter: "Drive letter",
  folderOnThisComputer: "Folder on this computer",
  aFreeLetterTheDrive: "A free letter the drive is shown under.",
  anEmptyFolderMadeIf: "An empty folder, made if it is not there and removed again after.",
  showOnTheDrive: "Show on the drive",
  thisFolder: "This folder",
  readOnlyProgramsCanOpen: "Read-only: programs can open files but not change them",
  openInFileManagerWhen: "Open in file manager when mounted",
  theConnectionIsLostReconnect: "The connection is lost. Reconnect to mount it.",
  mounting: "Mounting...",
  mount: "Mount",

  // components/QuitDialog
  quitPoros: "Quit Poros?",
  unmountAndQuit: "Unmount and quit",

  // lib/mounts
  restartUnmountsDrivesInUse: {
    one: "A drive has files open in programs or uploads pending. Restarting unmounts it, and what is not uploaded yet is kept on this computer.",
    other:
      "{count} drives have files open in programs or uploads pending. Restarting unmounts them, and what is not uploaded yet is kept on this computer.",
  },
  restartUnmountsDrivesInUseWithVault: {
    one: "A drive has files open in programs or uploads pending. Restarting unmounts it, and what is not uploaded yet is kept on this computer, unencrypted for files from a vault.",
    other:
      "{count} drives have files open in programs or uploads pending. Restarting unmounts them, and what is not uploaded yet is kept on this computer, unencrypted for files from a vault.",
  },
  driveCount: { one: "{count} drive", other: "{count} drives" },
  drivesWithFailedUploads: {
    one: "{drives}, {count} upload failed",
    other: "{drives}, {count} uploads failed",
  },
  drivesUploading: { one: "{drives}, uploading {count}", other: "{drives}, uploading {count}" },
  filesOpen: { one: "{count} file open", other: "{count} files open" },
  uploadsPending: { one: "{count} upload pending", other: "{count} uploads pending" },
  sizeRead: "{size} read",
  sizeWritten: "{size} written",
  idle: "Idle",
  uploadFailed: "Failed: {error}",
  uploadWaiting: "Waiting",
  uploading: "Uploading",
  uploadingPercent: "Uploading, {percent}%",
  chooseADriveLetter: "Choose a drive letter.",
  chooseAFolder: "Choose a folder.",
  driveLetterSuchAs: "A drive letter, such as {example}",
  fullPathSuchAs: "A full path, such as {example}",

  // state/mountActions
  savedACopyAt: "Saved a copy at {destination}",
} as const satisfies MessageArea;
