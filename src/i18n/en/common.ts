import type { MessageArea } from "../catalog";

export const common = {
  // App
  settings: "Settings",

  // components/AppearanceSettings
  delete: "Delete",
  name: "Name",

  // components/ArchiveSettings
  browse: "Browse",
  listSeparator: ", ",

  // components/AutomationSettings
  refresh: "Refresh",
  add: "Add",

  // components/BulkRenameDialog
  sentenceSeparator: " ",
  start: "Start",
  tryAgain: "Try again",
  cancel: "Cancel",
  rename: "Rename",
  remove: "Remove",
  keep: "Keep",
  no: "No",

  // components/CommandOutputDialog
  stop: "Stop",
  close: "Close",

  // components/ConfirmDialog
  working: "Working...",

  // components/ConflictDialog
  size: "Size",
  skip: "Skip",
  overwrite: "Overwrite",
  resume: "Resume",

  // components/ConnectDialog
  connect: "Connect",
  save: "Save",

  // components/DriveStatus
  discard: "Discard",
  open: "Open",

  // components/EditorPane
  dismiss: "Dismiss",

  // components/ExternalEditDialogs
  upload: "Upload",

  // components/ExternalEditorSettings
  minutesUnit: "min",

  // components/FileList
  folder: "Folder",

  // components/FileOperationsHost
  replace: "Replace",

  // components/FilePane
  asSentence: "{text}.",
  none: "None",
  cut: "Cut",
  copy: "Copy",
  paste: "Paste",
  edit: "Edit",
  loading: "Loading...",

  // components/FontPicker
  default: "Default",

  // components/ImportConnectionsDialog
  clauseSeparator: "; ",
  back: "Back",

  // components/LimitScheduleSettings
  download: "Download",
  pause: "Pause",

  // components/PreviewPane
  nothing: "Nothing",

  // components/PropertiesDialog
  apply: "Apply",
  ok: "OK",

  // components/SearchBar
  search: "Search",

  // components/SettingsDialog
  askMe: "Ask me",
  secondsUnit: "s",

  // components/SyncPlanList
  path: "Path",

  // components/TerminalPane
  clear: "Clear",

  // components/TransferView
  retry: "Retry",

  // components/UpdateNotice
  hide: "Hide",

  // lib/format
  durationSeconds: "{seconds}s",
  durationMinutes: "{minutes}m {seconds}s",
  durationHours: "{hours}h {minutes}m",

  // state/archiveActions
  labelWithMessage: "{label}: {message}",
} as const satisfies MessageArea;
