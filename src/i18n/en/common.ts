import type { MessageArea } from "../catalog";

export const common = {
  // App
  settings: "Settings",

  // components/AppearanceSettings
  delete: "Delete",
  name: "Name",

  // components/ArchiveSettings
  browse: "Browse",

  // components/AutomationSettings
  refresh: "Refresh",
  add: "Add",

  // components/BulkRenameDialog
  start: "Start",
  tryAgain: "Try again",
  cancel: "Cancel",

  // components/CommandOutputDialog
  stop: "Stop",
  close: "Close",

  // components/ConflictDialog
  size: "Size",
  skip: "Skip",

  // components/ConnectDialog
  connect: "Connect",

  // components/EditorPane
  dismiss: "Dismiss",

  // components/ExternalEditDialogs
  overwrite: "Overwrite",
  upload: "Upload",

  // components/ExternalEditsStatus
  discard: "Discard",

  // components/FileList
  folder: "Folder",

  // components/FilePane
  none: "None",
  cut: "Cut",
  copy: "Copy",
  paste: "Paste",
  open: "Open",
  edit: "Edit",
  rename: "Rename",
  loading: "Loading...",

  // components/ImportConnectionsDialog
  back: "Back",

  // components/LimitScheduleSettings
  download: "Download",

  // components/SaveConnectionDialog
  save: "Save",

  // components/SearchBar
  search: "Search",
  apply: "Apply",

  // components/SyncPlanList
  path: "Path",

  // components/TerminalPane
  clear: "Clear",

  // components/TransferView
  remove: "Remove",
  retry: "Retry",
  pause: "Pause",
  resume: "Resume",

  // components/UpdateNotice
  hide: "Hide",
  // components/BulkRenameDialog
  keep: "Keep",
  no: "No",

  // components/FileOperationsHost
  replace: "Replace",

  // components/SettingsDialog
  askMe: "Ask me",

  // components/TransferLookSettings
  nothing: "Nothing",

  // lib/format: durations, such as the time a transfer has left
  durationSeconds: "{seconds}s",
  durationMinutes: "{minutes}m {seconds}s",
  durationHours: "{hours}h {minutes}m",

  // Between the parts of a list that reads as one line, such as totals
  listSeparator: ", ",
  // components/ConfirmDialog
  working: "Working...",

  // components/FontPicker
  default: "Default",
} as const satisfies MessageArea;
