import type { MessageArea } from "../catalog";

export const updates = {
  // components/SettingsDialog
  downloadingVersion: "Downloading {version}",
  downloadingVersionPercent: "Downloading {version}, {percent}",
  installingVersion: "Installing {version}",
  checkingForUpdates: "Checking for updates",
  versionIsAvailable: "{version} is available",
  upToDateCheckedAt: "Up to date, checked at {time}",
  theVersionYouAreRunning: "The version you are running",
  couldNotListTheReleases: "Could not list the releases: {message}",
  noOtherReleaseToInstall: "No other release from {version} on to install.",
  switchToAnotherRelease: "Switch to another release from {version} on.",
  versionIsReadyRestart: "{version} is ready. Restart Poros to switch to it.",
  restartingInto: "Restarting into {version}",
  couldNotInstallVersionBecause: "Could not install {version}: {message}",

  // components/StatusBar
  restartToUse: "Restart to use {version}",
  restarting: "Restarting",
  couldNotInstallVersion: "Could not install {version}",

  // components/UpdateDialog
  updateAvailable: "Update available",
  isAvailableYouHave: "Poros {version} is available. You have {current}.",
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
  of: "{received} of {total}",
  downloadingPoros: "Downloading Poros {version}",
  installingPoros: "Installing Poros {version}",
  porosIsReady: "Poros {version} is ready",
  restartingPoros: "Restarting Poros",
  couldNotInstallPoros: "Could not install Poros {version}",

  // state/updateStore
  transfersWillStop: {
    one: "{count} transfer in the queue will stop when Poros restarts.",
    other: "{count} transfers in the queue will stop when Poros restarts.",
  },
  transfersWillStopAndComeBackPaused: {
    one: "{count} transfer in the queue will stop when Poros restarts and come back paused.",
    other: "{count} transfers in the queue will stop when Poros restarts and come back paused.",
  },
  unsavedChangesInFilesWill: {
    one: "Unsaved changes in {count} file will be lost when Poros restarts.",
    other: "Unsaved changes in {count} files will be lost when Poros restarts.",
  },
  theLatestReleaseDoesNotOfferInAppUpdates:
    "The latest release on GitHub does not offer in-app updates yet",
  couldNotCheckForUpdates: "Could not check for updates: {message}",
  heldSavesUploadedAfterTheRestart: {
    one: "A file open in another program holds a save not uploaded yet. Poros offers to upload it after the restart.",
    other:
      "{count} files open in other programs hold saves not uploaded yet. Poros offers to upload them after the restart.",
  },
  heldSavesWipedByTheRestart: {
    one: "A file open in another program holds a save not uploaded yet. Restarting wipes it, as vault files are not kept unencrypted.",
    other:
      "{count} files open in other programs hold saves not uploaded yet. Restarting wipes them, as vault files are not kept unencrypted.",
  },
  heldSavesPartlyWipedByTheRestart: {
    one: "A file open in another program holds a save not uploaded yet. Those from vaults are wiped; Poros offers to upload the others after the restart.",
    other:
      "{count} files open in other programs hold saves not uploaded yet. Those from vaults are wiped; Poros offers to upload the others after the restart.",
  },
} as const satisfies MessageArea;
