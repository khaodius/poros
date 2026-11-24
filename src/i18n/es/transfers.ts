import type { TranslatedArea } from "../catalog";

export const transfers = {
  // components/TransferLookSettings
  timeLeft: "Quedan {duration}",

  // components/TransferView
  copySourcePaths: "Copiar las rutas de origen",
  copySourcePath: "Copiar la ruta de origen",
  moveToTop: "Mover al principio",
  moveToBottom: "Mover al final",
  progress: "Progreso",
  speed: "Velocidad",
  status: "Estado",
  dragFilesBetweenTabsOr:
    "Arrastra archivos entre pestañas o usa Subir y Descargar en el menú de un archivo.",
  transfersThatFailAfterTheir:
    "Aquí aparecen las transferencias que fallan tras agotar sus reintentos.",
  finishedTransfersAreListedHere: "Aquí aparecen las transferencias terminadas.",
  done: "Terminada",
  scheduleThese: "Programar estas...",
  scheduleThis: "Programar esta...",
  connectionLostWaitingForTheServer: "Se perdió la conexión, esperando al servidor",
  waitingToRetry: "Esperando para reintentar (intento {attempt})",
  queued: "En cola",
  listingFolder: "Listando la carpeta",
  updatingWithRsyncSent: "Actualizando con rsync, {size} enviados",
  copyingDirectlyBetweenTheServers: "Copiando directamente entre los servidores",
  transferringOnConnections: {
    one: "Transfiriendo por {count} conexión",
    many: "Transfiriendo por {count} de conexiones",
    other: "Transfiriendo por {count} conexiones",
  },
  transferring: "Transfiriendo",
  paused: "En pausa",
  alreadyExistsWaitingForYourChoice: "Ya existe, esperando tu decisión",
  doneCopiedDirectlyBetweenTheServers: "Terminada, copiada directamente entre los servidores",
  doneWithRsyncSent: "Terminada con rsync, enviados {sent} de {size}",
  skippedBecause: "Omitida: {error}",
  skipped: "Omitida",
  failedBecause: "Error: {error}",
  failedUnknownError: "Error: error desconocido",
  rowTitle: "{source}\na {target}",
  rowTitleQueuedByTask: "{source}\na {target}\nAgregada a la cola por la tarea {task}",

  // lib/progressBars
  sizeOfSize: "{transferred} de {size}",

  // state/transferActions
  nothingToTransferOnlyRegular:
    "Nada que transferir: solo se pueden copiar archivos normales y carpetas.",
  unlockTheVaultToCopy: "Desbloquea la bóveda para copiar archivos en ella.",
  couldNotCheckWhetherThis: "No se pudo comprobar si esta carpeta es una bóveda.",
  isALockedVaultOpen: "{baseName} es una bóveda bloqueada. Ábrela para copiar archivos en ella.",
  dropLocalFilesOnA: "Suelta archivos locales en una pestaña de servidor para subirlos.",
  copyingTo: "Copiando a {folder}",
  movedTo: "Movido a {folder}",
  copiedTo: "Copiado a {folder}",
  connectToAServerTo: "Conéctate a un servidor para subir archivos a él.",
  openALocalTabTo: "Abre una pestaña local en la que descargar.",
  dropFilesOnAServer: "Suelta archivos en una pestaña de servidor para subirlos.",
  local: "Local",
} satisfies TranslatedArea<"transfers">;
