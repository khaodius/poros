import type { TranslatedArea } from "../catalog";

export const common = {
  // App
  settings: "Einstellungen",

  // components/AppearanceSettings
  delete: "Löschen",
  name: "Name",

  // components/ArchiveSettings
  browse: "Durchsuchen",
  listSeparator: ", ",

  // components/AutomationSettings
  refresh: "Aktualisieren",
  add: "Hinzufügen",
  labelWithMessage: "{label}: {message}",

  // components/BulkRenameDialog
  sentenceSeparator: " ",
  start: "Starten",
  tryAgain: "Erneut versuchen",
  cancel: "Abbrechen",
  rename: "Umbenennen",
  remove: "Entfernen",
  keep: "Behalten",
  no: "Nein",

  // components/CommandOutputDialog
  stop: "Stoppen",
  close: "Schließen",

  // components/ConfirmDialog
  working: "Wird ausgeführt...",

  // components/ConflictDialog
  size: "Größe",
  skip: "Überspringen",
  overwrite: "Überschreiben",
  resume: "Fortsetzen",

  // components/ConnectDialog
  connect: "Verbinden",
  save: "Speichern",

  // components/DriveStatus
  discard: "Verwerfen",
  open: "Öffnen",

  // components/EditorPane
  dismiss: "Ausblenden",

  // components/ExternalEditDialogs
  upload: "Hochladen",

  // components/ExternalEditorSettings
  minutesUnit: "min",

  // components/FileList
  folder: "Ordner",

  // components/FileOperationsHost
  replace: "Ersetzen",

  // components/FilePane
  asSentence: "{text}.",
  none: "Ohne",
  cut: "Ausschneiden",
  copy: "Kopieren",
  paste: "Einfügen",
  edit: "Bearbeiten",
  loading: "Wird geladen...",

  // components/FontPicker
  default: "Standard",

  // components/ImportConnectionsDialog
  clauseSeparator: "; ",
  back: "Zurück",

  // components/LimitScheduleSettings
  download: "Herunterladen",
  pause: "Anhalten",

  // components/PreviewPane
  nothing: "Nichts",

  // components/PropertiesDialog
  apply: "Übernehmen",
  ok: "OK",

  // components/SearchBar
  search: "Suchen",

  // components/SettingsDialog
  askMe: "Nachfragen",
  secondsUnit: "s",

  // components/SyncPlanList
  path: "Pfad",

  // components/TerminalPane
  clear: "Leeren",

  // components/TransferView
  retry: "Wiederholen",

  // components/UpdateNotice
  hide: "Ausblenden",

  // lib/format
  durationSeconds: "{seconds} s",
  durationMinutes: "{minutes} min {seconds} s",
  durationHours: "{hours} h {minutes} min",
} satisfies TranslatedArea<"common">;
