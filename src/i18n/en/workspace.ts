import type { MessageArea } from "../catalog";

export const workspace = {
  // App
  couldNotReadSavedConnections: "Could not read saved connections",
  couldNotReadScheduledTasks: "Could not read scheduled tasks",
  couldNotReadTheStart: "Could not read the start-up log",
  couldNotStart: "Could not start",
  poros: "Poros",
  connectToServer: "Connect to server",
  synchronizeFolders: "Synchronize folders",
  scheduledTasks: "Scheduled tasks",
  settingsCtrl: "Settings (Ctrl+,)",

  // components/BottomPanel
  queue: "Queue",
  failed: "Failed",
  completed: "Completed",
  startQueue: "Start queue",
  pauseQueue: "Pause queue",
  pauseSelected: "Pause selected",
  resumeSelected: "Resume selected",
  moveSelectedToTheTop: "Move selected to the top",
  moveSelectedToTheBottom: "Move selected to the bottom",
  removeSelected: "Remove selected",
  retryAll: "Retry all",
  retrySelected: "Retry selected",
  clearFailedTransfers: "Clear failed transfers",
  keepEachTransferInView: "Keep each transfer in view as it finishes",
  follow: "Follow",
  clearFinishedTransfers: "Clear finished transfers",
  simultaneousTransfers: "Simultaneous transfers",
  workers: "Workers",

  // components/CloseTabDialog
  close: "Close {label}?",
  transfers: { one: "{count} transfer", other: "{count} transfers" },
  transfersIntoOrOutOf:
    "Transfers into or out of an unlocked vault are paused either way: closing the tab locks the vault. Resume them once the vault is unlocked again.",
  pauseTransfersAndClose: "Pause transfers and close",
  keepTransferring: "Keep transferring",

  // components/DockArea
  dragToResizeDoubleClick: "Drag to resize, double-click to even out",

  // components/DragGhost
  uploadTo: "Upload to {folder}",
  downloadTo: "Download to {folder}",
  copyTo: "Copy to {folder}",
  copyTo2: "Copy to {baseName}",
  moveTo: "Move to {baseName}",

  // components/RestoringPane
  signIn: "Sign in",
  closeTab: "Close tab",

  // components/SplashScreen
  loadingPoros: "Loading Poros",

  // components/StatusBar
  connectionToLost: "Connection to {label} lost",
  connectedTo: "Connected to {label}",
  serversConnected: { one: "{count} server connected", other: "{count} servers connected" },
  notConnected: "Not connected",
  updateTo: "Update to {version}",
  uploadSpeed: "Upload speed",
  downloadSpeed: "Download speed",
  poros2: "Poros {value}",

  // components/TabBar
  openTerminal: "Open terminal",
  splitRight: "Split right",
  splitDown: "Split down",
  moveToNewWindow: "Move to new window",
  moveToMainWindow: "Move to main window",
  closeOtherTabs: "Close other tabs",
  newTabCtrlT: "New tab (Ctrl+T)",
  newTab: "New tab",
  disconnected: "Disconnected",
  unsavedChanges: "Unsaved changes",
  close2: "Close {label}",
  closeCtrlW: "Close (Ctrl+W)",

  // components/TabContent
  local: "Local",
  thisSessionHasEnded: "This session has ended",
  connectionLost: "Connection lost",

  // components/WelcomePanel
  localFiles: "Local files",
  import: "Import",
  importFromFilezillaWinscpAnd: "Import from FileZilla, WinSCP and other apps",
  savedConnections: "Saved connections",
  serversYouSaveFromThe:
    "Servers you save from the connect dialog, or from the bookmark button of a server tab, appear here. Sites saved in FileZilla, WinSCP, PuTTY and other apps can be imported.",
  connecting: "Connecting...",

  // components/WindowControls
  minimize: "Minimize",
  restore: "Restore",
  maximize: "Maximize",

  // components/WindowSketch
  newWindow: "New window",
  log: "Log",

  // components/Workspace
  dragToResizeDoubleClickTo: "Drag to resize, double-click to reset",

  // state/restoreActions
  couldNotReopen: "Could not reopen {name}: {message}",
  porosNoLongerKnowsHow: "Poros no longer knows how to reach it.",
  // components/DropMarker
  mergeAsTab: "Merge as tab",
  splitLeft: "Split left",
  splitUp: "Split up",
} as const satisfies MessageArea;
