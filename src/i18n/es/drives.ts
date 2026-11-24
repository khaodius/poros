import type { TranslatedArea } from "../catalog";

export const drives = {
  // components/DriveStatus
  filesNotUploaded: {
    one: "{count} archivo sin subir",
    many: "{count} de archivos sin subir",
    other: "{count} archivos sin subir",
  },
  drivesMountedFromPoros: "Unidades montadas desde Poros",
  mountedDrives: "Unidades montadas",
  driveAtMountPoint: "{label} en {mountPoint}",
  readOnly: "Solo lectura",
  factAt: "En",
  factShows: "Muestra",
  factActivity: "Actividad",
  uploads: "Subidas",
  unmount: "Desmontar",
  refusedWhileProgramsHaveFiles:
    "No se permite mientras haya programas con archivos abiertos en la unidad o subidas pendientes",
  forceUnmount: "Forzar desmontaje",
  unmountsAtOnceProgramsLose:
    "Desmonta de inmediato: los programas pierden los archivos que tienen abiertos y lo que aún no se ha subido se conserva en este equipo",
  notUploaded: "Sin subir",
  notUploadedKeptOnThis: "Sin subir, conservado en este equipo",
  keptFileFacts: "{connection}, {size}",
  keptFileFactsWithError: "{connection}, {size}: {error}",
  saveACopy: "Guardar una copia...",
  discardForGood: "Descartar definitivamente",

  // components/DrivesGoing
  unmountedWhenTheTabCloses: {
    one: "La unidad en {paths} se desmonta al cerrar la pestaña.",
    many: "Las unidades en {paths} se desmontan al cerrar la pestaña.",
    other: "Las unidades en {paths} se desmontan al cerrar la pestaña.",
  },
  unmountedWhenTheVaultLocks: {
    one: "La unidad en {paths} se desmonta al bloquear la bóveda.",
    many: "Las unidades en {paths} se desmontan al bloquear la bóveda.",
    other: "Las unidades en {paths} se desmontan al bloquear la bóveda.",
  },
  unmountedWhenPorosQuits: {
    one: "La unidad en {paths} se desmonta al salir de Poros.",
    many: "Las unidades en {paths} se desmontan al salir de Poros.",
    other: "Las unidades en {paths} se desmontan al salir de Poros.",
  },
  busyDrivesLoseOpenFiles:
    "Todavía hay programas con archivos abiertos en ella, o subidas pendientes. Los programas pierden los archivos abiertos, y lo que aún no está en el servidor se conserva en este equipo, en la lista de unidades de la barra de estado.",
  busyDrivesLoseOpenFilesWithVault:
    "Todavía hay programas con archivos abiertos en ella, o subidas pendientes. Los programas pierden los archivos abiertos, y lo que aún no está en el servidor se conserva en este equipo, en la lista de unidades de la barra de estado. Los archivos conservados de una bóveda se guardan sin cifrar hasta que los guardes o descartes.",

  // components/MountDialog
  mountNameAsADrive: "Montar {name} como unidad",
  theServersRoot: "La raíz del servidor",
  theVaultsRoot: "La raíz de la bóveda",
  rootAt: "{root}: {path}",
  whatProgramsSaveServer:
    "Lo que guardan los programas se sube cuando cierran el archivo. La unidad se desmonta al cerrar la pestaña o al salir de Poros.",
  whatProgramsSaveVault:
    "Lo que guardan los programas se sube cuando cierran el archivo y, hasta entonces, queda sin cifrar en este equipo. La unidad se desmonta al cerrar la pestaña, al bloquear la bóveda o al salir de Poros.",
  mountAsDrive: "Montar como unidad",
  theConnectionHasClosed: "La conexión se ha cerrado.",
  driveLetter: "Letra de unidad",
  folderOnThisComputer: "Carpeta en este equipo",
  aFreeLetterTheDrive: "Una letra libre con la que se muestra la unidad.",
  anEmptyFolderMadeIf: "Una carpeta vacía, que se crea si no existe y se quita después.",
  showOnTheDrive: "Mostrar en la unidad",
  thisFolder: "Esta carpeta",
  readOnlyProgramsCanOpen: "Solo lectura: los programas pueden abrir archivos, pero no cambiarlos",
  openInFileManagerWhen: "Abrir en el administrador de archivos al montar",
  theConnectionIsLostReconnect: "Se perdió la conexión. Reconecta para montarla.",
  mounting: "Montando...",
  mount: "Montar",

  // components/QuitDialog
  quitPoros: "¿Salir de Poros?",
  unmountAndQuit: "Desmontar y salir",

  // lib/mounts
  restartUnmountsDrivesInUse: {
    one: "Una unidad tiene archivos abiertos en programas o subidas pendientes. Al reiniciar se desmonta, y lo que aún no se ha subido se conserva en este equipo.",
    many: "{count} de unidades tienen archivos abiertos en programas o subidas pendientes. Al reiniciar se desmontan, y lo que aún no se ha subido se conserva en este equipo.",
    other:
      "{count} unidades tienen archivos abiertos en programas o subidas pendientes. Al reiniciar se desmontan, y lo que aún no se ha subido se conserva en este equipo.",
  },
  restartUnmountsDrivesInUseWithVault: {
    one: "Una unidad tiene archivos abiertos en programas o subidas pendientes. Al reiniciar se desmonta, y lo que aún no se ha subido se conserva en este equipo, sin cifrar en el caso de los archivos de una bóveda.",
    many: "{count} de unidades tienen archivos abiertos en programas o subidas pendientes. Al reiniciar se desmontan, y lo que aún no se ha subido se conserva en este equipo, sin cifrar en el caso de los archivos de una bóveda.",
    other:
      "{count} unidades tienen archivos abiertos en programas o subidas pendientes. Al reiniciar se desmontan, y lo que aún no se ha subido se conserva en este equipo, sin cifrar en el caso de los archivos de una bóveda.",
  },
  driveCount: { one: "{count} unidad", many: "{count} de unidades", other: "{count} unidades" },
  drivesWithFailedUploads: {
    one: "{drives}, {count} subida con error",
    many: "{drives}, {count} de subidas con error",
    other: "{drives}, {count} subidas con error",
  },
  drivesUploading: {
    one: "{drives}, subiendo {count}",
    many: "{drives}, subiendo {count}",
    other: "{drives}, subiendo {count}",
  },
  filesOpen: {
    one: "{count} archivo abierto",
    many: "{count} de archivos abiertos",
    other: "{count} archivos abiertos",
  },
  uploadsPending: {
    one: "{count} subida pendiente",
    many: "{count} de subidas pendientes",
    other: "{count} subidas pendientes",
  },
  sizeRead: "{size} leídos",
  sizeWritten: "{size} escritos",
  idle: "Sin actividad",
  uploadFailed: "Error: {error}",
  uploadWaiting: "En espera",
  uploading: "Subiendo",
  uploadingPercent: "Subiendo, {percent} %",
  chooseADriveLetter: "Elige una letra de unidad.",
  chooseAFolder: "Elige una carpeta.",
  driveLetterSuchAs: "Una letra de unidad, como {example}",
  fullPathSuchAs: "Una ruta completa, como {example}",

  // state/mountActions
  savedACopyAt: "Se guardó una copia en {destination}",
} satisfies TranslatedArea<"drives">;
