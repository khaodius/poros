import type { TranslatedArea } from "../catalog";

export const schedule = {
  // components/SchedulerDialog
  scheduledTasks: "Tareas programadas",
  tasks: "Tareas",
  newTask: "Nueva tarea",
  shownInTheListOf: "Se muestra en la lista de tareas",
  turnedOn: "Activada",
  task: "Tarea",
  savedConnection: "Conexión guardada",
  ifPorosIsClosedAt: "Si Poros está cerrado a esa hora, ejecutarla la próxima vez que se inicie",
  nextRun: "Próxima ejecución",
  runningNow: "En ejecución",
  lastRun: "Última ejecución",
  chooseTheLocalFolder: "Elegir la carpeta local",
  localFolder: "Carpeta local",
  theFolderOnThisComputer: "La carpeta de este equipo",
  serverFolder: "Carpeta del servidor",
  direction: "Dirección",
  conflictsAreLeftAloneAnd: "{hint} Los conflictos no se tocan y se informa de ellos.",
  rsyncOptions: "Opciones de rsync",
  typedAsOnRsyncsCommand:
    "Escritos como en la línea de comandos de rsync; se aplican por encima de las opciones de arriba cada vez que se ejecuta la tarea.",
  command: "Comando",
  inFolderOptional: "En la carpeta (opcional)",
  theLoginFolder: "La carpeta de inicio de sesión",
  when: "Cuándo",
  dateAndTime: "Fecha y hora",
  every: "cada",
  repeatEvery: "Repetir cada",
  unit: "Unidad",
  from: "desde",
  starting: "Inicio",
  at: "a las",
  timeOfDay: "Hora del día",
  daysOfTheWeek: "Días de la semana",
  onDays: "Algunos días",
  repeat: "Repetir",
  once: "Una vez",
  synchronizeFolders: "Sincronizar carpetas",
  transferFiles: "Transferir archivos",
  runACommand: "Ejecutar un comando",
  uploadTo: "Subir a",
  downloadFrom: "Descargar de",
  copyTo: "Copiar a",
  tasksRunWhilePorosIsOpen:
    "Las tareas se ejecutan mientras Poros está abierto, o desde poros-cli con cron o un temporizador. Las que guardes aparecen aquí.",
  turnedOff: "Desactivada",
  chooseAConnection: "Elige una conexión",
  noSavedConnections: "No hay conexiones guardadas",
  noSavedSftpConnections: "No hay conexiones SFTP guardadas",
  aDeletedConnection: "Una conexión eliminada",
  usesSynchronizingAndRunningCommands:
    "{name} usa {label}; para sincronizar y ejecutar comandos se necesita una conexión SFTP.",
  thisTasksSavedConnectionWas: "Se eliminó la conexión guardada de esta tarea. Elige otra.",
  deleteForGood: "Eliminar definitivamente",
  starting2: "Iniciando...",
  runNow: "Ejecutar ahora",
  saving: "Guardando...",
  thisConnectionsPasswordIsNotSaved:
    "La contraseña de esta conexión no está guardada en el llavero del sistema, así que la tarea no puede iniciar sesión.",
  thePasswordOfIsNot:
    "La contraseña de {names} no está guardada en el llavero del sistema, así que la tarea no puede iniciar sesión.",
  thePasswordsOfAreNot:
    "Las contraseñas de {names} no están guardadas en el llavero del sistema, así que la tarea no puede iniciar sesión.",
  saveItInTheConnections: "Guárdala en la configuración de la conexión.",
  saveThemInTheConnections: "Guárdalas en la configuración de las conexiones.",
  noneTheTimeHasPassed: "Ninguna, la hora ya pasó",

  // components/TransferTaskFields
  aFullPathOnThe: "Una ruta completa en el servidor; termina una carpeta con /",
  aRunNowGoesInto: "Una ejecución ahora va a {folder}.",
  addFiles: "Agregar archivos",
  addFolders: "Agregar carpetas",
  addTheFilesAndFolders:
    "Agrega los archivos y carpetas que sube la tarea. Una carpeta va con todo lo que contiene.",
  addTheServerPathsThe:
    "Agrega las rutas del servidor que copia la tarea, como /var/www/site. Una carpeta va con todo lo que contiene.",
  betweenServers: "Entre servidores",
  chooseTheFolderToDownload: "Elige la carpeta en la que descargar",
  copiesFilesAndFoldersFrom: "Copia archivos y carpetas de este equipo al servidor.",
  copiesFilesAndFoldersFromOne: "Copia archivos y carpetas de una conexión guardada a otra.",
  copiesFilesAndFoldersFromThe: "Copia archivos y carpetas del servidor a este equipo.",
  copyFrom: "Copiar de",
  filesAndFoldersOnThe: "Archivos y carpetas del servidor",
  filesAndFoldersOnThis: "Archivos y carpetas de este equipo",
  inAFolderOfIts: "En una carpeta propia (opcional)",
  intoTheFolderOnThe: "En la carpeta del servidor",
  intoTheFolderOnThis: "En la carpeta de este equipo",
  itemsToCopy: "Elementos que copiar",
  remove: "Quitar {path}",
  removeFromTheTask: "Quitar de la tarea",
  serverPathToAdd: "Ruta del servidor que agregar",
  theFolderToDownloadInto: "La carpeta en la que descargar",
  whenAFileIsAlready: "Cuando un archivo ya existe",
  eachRunCanGetAFolder:
    "Cada ejecución puede recibir una carpeta con un nombre según su hora: {placeholders}.",

  // lib/schedule
  overwriteIfTheSourceIs: "Sobrescribir si el origen es más reciente",
  overwriteIfSizeOrDate: "Sobrescribir si difieren el tamaño o la fecha",
  keepBoth: "Conservar ambos",
  everyDay: "Todos los días",
  weekdays: "Entre semana",
  weekends: "Fines de semana",
  onceAt: "Una vez, {date}",
  everyMinutes: {
    one: "Cada minuto",
    many: "Cada {count} de minutos",
    other: "Cada {count} minutos",
  },
  everyHours: { one: "Cada hora", many: "Cada {count} de horas", other: "Cada {count} horas" },
  everyDays: { one: "Cada día", many: "Cada {count} de días", other: "Cada {count} días" },
  daysAt: "{days} a las {time}",
  unitMinutes: "minutos",
  unitHours: "horas",
  unitDays: "días",
  nameRun: "Ejecutar {program}",
  nameUploadOne: "Subir {name}",
  nameDownloadOne: "Descargar {name}",
  nameCopyOne: "Copiar {name}",
  nameUploadItems: {
    one: "Subir {count} elemento",
    many: "Subir {count} de elementos",
    other: "Subir {count} elementos",
  },
  nameDownloadItems: {
    one: "Descargar {count} elemento",
    many: "Descargar {count} de elementos",
    other: "Descargar {count} elementos",
  },
  nameCopyItems: {
    one: "Copiar {count} elemento",
    many: "Copiar {count} de elementos",
    other: "Copiar {count} elementos",
  },
  nameSync: "Sincronizar {name}",
  patternTooLong: {
    one: "El patrón de carpeta tiene más de {count} carácter",
    many: "El patrón de carpeta tiene más de {count} de caracteres",
    other: "El patrón de carpeta tiene más de {count} caracteres",
  },
  patternLoneBrace:
    "El patrón de carpeta tiene una llave sin su pareja; escribe los marcadores como {example}",
  patternUnknown: "El patrón de carpeta tiene {found}, que no es un marcador; usa {known}",
  patternBadName:
    'El patrón de carpeta da el nombre "{name}", que no puede ser un nombre de carpeta',

  // lib/scheduleDraft
  porosNoLongerKnowsThe:
    "Poros ya no conoce el servidor de estas transferencias. Programa los archivos desde una pestaña.",
  filesInAnUnlockedVault:
    "Los archivos de una bóveda desbloqueada no se pueden programar: una tarea no podría desbloquear la bóveda.",
  saveTheConnectionToFirst:
    "Guarda antes la conexión a {label}: una tarea programada inicia sesión con una conexión guardada.",
  selectTheTransfersToSchedule: "Selecciona las transferencias que quieres programar.",
  selectTransfersThatGoThe:
    "Para programarlas, selecciona transferencias que vayan en la misma dirección entre los mismos lugares.",
  theSelectedTransfersGoTo:
    "Las transferencias seleccionadas van a carpetas distintas. Selecciona las que van a una misma carpeta.",
  selectTheFilesToSchedule: "Selecciona los archivos que quieres programar.",
  blockedNoServer: "sin servidor",
  blockedInVault: "no disponible en una bóveda",
  blockedUnsaved: "guarda antes la conexión",

  // state/scheduleActions
  blockedNoTab: "sin pestaña",
} satisfies TranslatedArea<"schedule">;
