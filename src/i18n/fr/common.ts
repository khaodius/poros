import type { TranslatedArea } from "../catalog";

export const common = {
  // App
  settings: "Paramètres",

  // components/AppearanceSettings
  delete: "Supprimer",
  name: "Nom",

  // components/ArchiveSettings
  browse: "Parcourir",
  listSeparator: ", ",

  // components/AutomationSettings
  refresh: "Actualiser",
  add: "Ajouter",

  // components/BulkRenameDialog
  sentenceSeparator: " ",
  start: "Démarrer",
  tryAgain: "Réessayer",
  cancel: "Annuler",
  rename: "Renommer",
  remove: "Retirer",
  keep: "Conserver",
  no: "Non",

  // components/CommandOutputDialog
  stop: "Arrêter",
  close: "Fermer",

  // components/ConfirmDialog
  working: "En cours...",

  // components/ConflictDialog
  size: "Taille",
  skip: "Ignorer",
  overwrite: "Écraser",
  resume: "Reprendre",

  // components/ConnectDialog
  connect: "Se connecter",
  save: "Enregistrer",

  // components/DriveStatus
  discard: "Abandonner",
  open: "Ouvrir",

  // components/EditorPane
  dismiss: "Masquer",

  // components/ExternalEditDialogs
  upload: "Envoyer",

  // components/ExternalEditorSettings
  minutesUnit: "min",

  // components/FileList
  folder: "Dossier",

  // components/FileOperationsHost
  replace: "Remplacer",

  // components/FilePane
  asSentence: "{text}.",
  none: "Aucun",
  cut: "Couper",
  copy: "Copier",
  paste: "Coller",
  edit: "Modifier",
  loading: "Chargement...",

  // components/FontPicker
  default: "Par défaut",

  // components/ImportConnectionsDialog
  clauseSeparator: "\u202f; ",
  back: "Retour",

  // components/LimitScheduleSettings
  download: "Télécharger",
  pause: "Suspendre",

  // components/PreviewPane
  nothing: "Rien",

  // components/PropertiesDialog
  apply: "Appliquer",
  ok: "OK",

  // components/SearchBar
  search: "Rechercher",

  // components/SettingsDialog
  askMe: "Me demander",
  secondsUnit: "s",

  // components/SyncPlanList
  path: "Chemin",

  // components/TerminalPane
  clear: "Effacer",

  // components/TransferView
  retry: "Réessayer",

  // components/UpdateNotice
  hide: "Masquer",

  // lib/format
  durationSeconds: "{seconds}\u00a0s",
  durationMinutes: "{minutes}\u00a0min {seconds}\u00a0s",
  durationHours: "{hours}\u00a0h {minutes}\u00a0min",

  // state/archiveActions
  labelWithMessage: "{label}\u00a0: {message}",
} satisfies TranslatedArea<"common">;
