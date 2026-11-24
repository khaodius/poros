import type { MessageArea } from "../catalog";

export const workspace = {
  // App
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
  log: "Log",

  // components/CloseTabDialog
  close: "Close {label}?",
  transfersIntoOrOutOf:
    "Transfers into or out of an unlocked vault are paused either way: closing the tab locks the vault. Resume them once the vault is unlocked again.",
  pauseTransfersAndClose: "Pause transfers and close",
  keepTransferring: "Keep transferring",
  transfersForThisServerStillQueued: {
    one: "{count} transfer for this server is still in the queue. They can keep running on their own connections after the tab closes, or wait paused until you resume them.",
    other:
      "{count} transfers for this server are still in the queue. They can keep running on their own connections after the tab closes, or wait paused until you resume them.",
  },

  // components/DockArea
  dragToResizeDoubleClick: "Drag to resize, double-click to even out",
  moveToMainWindow: "Move to main window",
  newWindow: "New window",
  moveToThatWindow: "Move to that window",

  // components/DragGhost
  uploadTo: "Upload to {folder}",
  downloadTo: "Download to {folder}",
  copyTo: "Copy to {folder}",
  copyTo2: "Copy to {baseName}",
  moveTo: "Move to {baseName}",

  // components/DropMarker
  splitRight: "Split right",
  splitDown: "Split down",
  mergeAsTab: "Merge as tab",
  splitLeft: "Split left",
  splitUp: "Split up",

  // components/RestoringPane
  signIn: "Sign in",
  closeTab: "Close tab",
  reopensNameOnceConnected: "Reopens {name} once connected.",
  opensANewShellOnceConnected: "Opens a new shell once connected.",
  reopensOnceConnected: "Reopens once connected.",
  openingName: "Opening {name}...",
  openingFile: "Opening file...",
  reconnectingTo: "Reconnecting to {label}...",
  signInTo: "Sign in to {label}",
  couldNotReconnectTo: "Could not reconnect to {label}",
  porosKeepsNoPasswordForThisServer: "Poros keeps no password for this server.",

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
  appVersion: "Poros {version}",
  transfersLeft: { one: "{count} transfer left", other: "{count} transfers left" },
  aboutDuration: "about {duration}",
  queueStatusPaused: "{status} (paused)",

  // components/TabBar
  openTerminal: "Open terminal",
  moveToNewWindow: "Move to new window",
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
  reconnectingInS: "Reconnecting in {secondsLeft} s",
  reconnecting: "Reconnecting...",
  reconnectingInSecondsAttempt: "Reconnecting in {seconds} s (attempt {attempt})",
  reconnectingAttempt: "Reconnecting... (attempt {attempt})",
  reconnectNow: "Reconnect now",
  reconnect: "Reconnect",

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

  // components/Workspace
  dragToResizeDoubleClickTo: "Drag to resize, double-click to reset",

  // hooks/useTabLabel
  closedSession: "Closed session",

  // state/restoreActions
  couldNotReopen: "Could not reopen {name}: {message}",
  porosNoLongerKnowsHow: "Poros no longer knows how to reach it.",
} as const satisfies MessageArea;
