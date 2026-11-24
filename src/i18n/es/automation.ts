import type { TranslatedArea } from "../catalog";

export const automation = {
  // components/AutomationSettings
  whenTheTransferQueueFinishes: "Cuando termine la cola de transferencias",
  then: "Después",
  thenHint:
    "También en la barra de herramientas de la cola. Todo salvo un comando hace una cuenta atrás de 30 segundos antes, para que puedas detenerlo.",
  onlyTheNextTime: "Solo la próxima vez",
  onlyTheNextTimeHint: "Vuelve a No hacer nada después de ocurrir.",
  command: "Comando",
  commandHint:
    "Para Ejecutar un comando; se ejecuta en este equipo. {done}, {failed}, {skipped} y {bytes} contienen los totales.",
  aCommandOrScript: "Un comando o script",
  showANotification: "Mostrar una notificación",
  onlyWhenPorosIsIn: "Solo cuando Poros está en segundo plano",
  playASound: "Reproducir un sonido",
  serverCommands: "Comandos del servidor",
  listedUnderCommandsWhenYou:
    "Aparecen en Comandos al hacer clic con el botón derecho en un panel de servidor. Se ejecutan en el shell del servidor, en la carpeta mostrada, con estos valores:",
  scheduledTasks: "Tareas programadas",
  synchronizeFoldersOrRunCommands: "Sincronizar carpetas o ejecutar comandos a horas fijas",
  synchronizeFoldersOrRunCommandsHint:
    "Las tareas se ejecutan mientras Poros está abierto, en segundo plano, mediante conexiones guardadas.",
  output: "Salida",
  openAWindowWithWhat:
    "Abrir una ventana con lo que imprime el comando; desactivado, se ejecuta sin mostrar nada",
  reloadTheServersFoldersWhen: "Volver a cargar las carpetas del servidor al terminar el comando",
  deleteCommand: "Eliminar comando",
  nameOfTheNewCommand: "Nombre del nuevo comando",
  newCommand: "Nuevo comando",

  // components/CommandOutputDialog
  earlierOutputWasDropped: "Se descartó la salida anterior.",
  theCommandPrintedNothing: "El comando no imprimió nada.",
  running: "En ejecución",
  commandIn: "{command} en {folder}",
  commandsIn: {
    one: "{count} comando en {folder}",
    many: "{count} de comandos en {folder}",
    other: "{count} comandos en {folder}",
  },
  theLoginFolder: "la carpeta de inicio de sesión",

  // components/RunCommandDialog
  runACommand: "Ejecutar un comando",
  runACommandOnThe: "Ejecutar un comando en el servidor",
  commandToRunIn: "Comando que se ejecutará en {folder}",
  refreshTheServersFoldersWhen: "Actualizar las carpetas del servidor al terminar",
  selectFilesOrFoldersTo: "Selecciona archivos o carpetas para rellenar los marcadores.",
  run: "Ejecutar",

  // components/WhenDoneControl
  whatHappensWhenTheQueue: "Qué ocurre cuando termina la cola",
  whenDone: "Al terminar",
  whenTheQueueFinishes: "Cuando termine la cola",

  // components/WhenDoneCountdown
  theTransferQueueHasFinished: "La cola de transferencias ha terminado. {title} en {seconds}.",
  seconds: { one: "{count} segundo", many: "{count} de segundos", other: "{count} segundos" },

  // components/commandMenu
  commands: "Comandos",
  runACommand2: "Ejecutar un comando...",
  editCommands: "Editar comandos...",

  // lib/automation
  filesTransferredIn: {
    one: "{count} archivo transferido ({size}) en {duration}",
    many: "{count} de archivos transferidos ({size}) en {duration}",
    other: "{count} archivos transferidos ({size}) en {duration}",
  },
  doNothing: "No hacer nada",
  closePoros: "Cerrar Poros",
  lockTheComputer: "Bloquear el equipo",
  sleep: "Suspender",
  hibernate: "Hibernar",
  logOff: "Cerrar sesión",
  shutDown: "Apagar",
  closingPoros: "Cerrando Poros",
  lockingTheComputer: "Bloqueando el equipo",
  puttingTheComputerToSleep: "Suspendiendo el equipo",
  hibernatingTheComputer: "Hibernando el equipo",
  loggingOff: "Cerrando sesión",
  shuttingDownTheComputer: "Apagando el equipo",
  confirmCloseNow: "Cerrar ahora",
  confirmLockNow: "Bloquear ahora",
  confirmSleepNow: "Suspender ahora",
  confirmHibernateNow: "Hibernar ahora",
  confirmLogOffNow: "Cerrar sesión ahora",
  confirmShutDownNow: "Apagar ahora",
  transfersFinishedWithErrors: "Transferencias terminadas con errores",
  transfersFinished: "Transferencias terminadas",
  failedCount: "{count} con error",
  skippedCount: "omitidos: {count}",

  // lib/commands
  eachSelectedItemsFullPath:
    "la ruta completa de cada elemento seleccionado, una ejecución por elemento",
  eachSelectedItemsNameRunning:
    "el nombre de cada elemento seleccionado, una ejecución por elemento",
  theFullPathsOfAll: "las rutas completas de todos los elementos seleccionados",
  theNamesOfAllSelected: "los nombres de todos los elementos seleccionados",
  theFolderShown: "la carpeta mostrada",
  stoppedAfter: "Detenido después de {elapsed}",
  endedBySignal: "Terminado por la señal {signal} después de {elapsed}",
  endedWithoutStatus: "Terminado sin estado de salida después de {elapsed}",
  exitedWithStatus: "Salió con el estado {status} después de {elapsed}",
  finishedIn: "Terminado en {elapsed}",

  // state/commandStore
  stopped: "Detenido",
  nothingToRun: "Nada que ejecutar",
  allCommandsFinished: "Terminaron los {finished} comandos",
  ofCommandsFailed: "Fallaron {failed} de {finished} comandos",

  // state/whenDone
  whenTheQueueFinished: "Al terminar la cola",
  theCommandAfterTheQueue: "El comando tras terminar la cola",
} satisfies TranslatedArea<"automation">;
