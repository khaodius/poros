import type { TranslatedArea } from "../catalog";

export const common = {
  // App
  settings: "Configuración",

  // components/AppearanceSettings
  delete: "Eliminar",
  name: "Nombre",

  // components/ArchiveSettings
  browse: "Examinar",

  // components/AutomationSettings
  refresh: "Actualizar",
  add: "Agregar",

  // components/BulkRenameDialog
  start: "Inicio",
  tryAgain: "Volver a intentarlo",
  cancel: "Cancelar",
  rename: "Cambiar nombre",
  remove: "Quitar",
  keep: "Conservar",
  no: "No",

  // components/CommandOutputDialog
  stop: "Detener",
  close: "Cerrar",

  // components/ConfirmDialog
  working: "Procesando...",

  // components/ConflictDialog
  size: "Tamaño",
  skip: "Omitir",
  overwrite: "Sobrescribir",
  resume: "Reanudar",

  // components/ConnectDialog
  connect: "Conectar",
  save: "Guardar",

  // components/DriveStatus
  discard: "Descartar",
  open: "Abrir",

  // components/DrivesGoing
  listSeparator: ", ",

  // components/EditorPane
  dismiss: "Cerrar aviso",

  // components/ExternalEditDialogs
  upload: "Subir",

  // components/FileList
  folder: "Carpeta",

  // components/FileOperationsHost
  replace: "Reemplazar",

  // components/FilePane
  none: "Ninguno",
  cut: "Cortar",
  copy: "Copiar",
  paste: "Pegar",
  edit: "Editar",
  loading: "Cargando...",

  // components/FontPicker
  default: "Predeterminada",

  // components/ImportConnectionsDialog
  back: "Atrás",

  // components/LimitScheduleSettings
  download: "Descargar",
  pause: "Pausar",

  // components/PreviewPane
  nothing: "Nada",

  // components/PropertiesDialog
  apply: "Aplicar",

  // components/SearchBar
  search: "Buscar",

  // components/SettingsDialog
  askMe: "Preguntarme",

  // components/SyncPlanList
  path: "Ruta",

  // components/TerminalPane
  clear: "Borrar",

  // components/TransferView
  retry: "Reintentar",

  // components/UpdateNotice
  hide: "Ocultar",

  // lib/format
  durationSeconds: "{seconds} s",
  durationMinutes: "{minutes} min {seconds} s",
  durationHours: "{hours} h {minutes} min",
} satisfies TranslatedArea<"common">;
