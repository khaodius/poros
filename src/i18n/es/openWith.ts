import type { TranslatedArea } from "../catalog";

export const openWith = {
  // components/ExternalEditDialogs
  isGoneFromTheServer: "{name} ya no está en el servidor",
  changedOnTheServer: "{name} cambió en el servidor",
  modified: "Modificado",
  onTheServer: "En el servidor",
  yourSavedCopy: "Tu copia guardada",
  keepBothUploadsYourCopy:
    "Conservar ambos sube tu copia junto a la del servidor, con un nombre que allí no esté en uso.",
  comparingReadsTheServersCopy: "La comparación lee la copia del servidor por SFTP",
  dontUpload: "No subir",
  uploadItAgain: "Volver a subirlo",
  keepBoth: "Conservar ambos",
  upload: "¿Subir {name}?",
  uploadEverySaveWithoutAsking: "Subir cada guardado sin preguntar",
  notNow: "Ahora no",
  defaultProgram: "Programa predeterminado",
  setUpForFilesLike: "Configurado para archivos como este",
  builtInEditor: "Editor integrado",
  editTheTextInA: "Editar el texto en una pestaña aquí",
  chooseAProgram: "Elegir un programa...",
  anyProgramOnThisComputer: "Cualquier programa de este equipo",
  isAProgram: "{name} es un programa",
  open: "Abrir {name}",
  programs: "Programas",
  filesNotUploadedLastTime: "Archivos que no se subieron la última vez",
  later: "Más tarde",
  lock: "¿Bloquear {label}?",
  close: "¿Cerrar {label}?",
  theirSavesKeepBeingUploaded:
    "Lo que se guarde en ellos se sigue subiendo después de cerrar la pestaña.",
  stopEditingAndClose: "Dejar de editar y cerrar",
  uploadSavedFilesBeforeQuitting: "¿Subir los archivos guardados antes de salir?",
  someCouldNotBeUploaded: "Algunos no se pudieron subir.",
  quitWithoutUploading: "Salir sin subir",
  deleted: "Eliminado",
  compare: "Comparar",
  compareSftpOnly: "Comparar (solo SFTP)",
  lockAnyway: "Bloquear de todos modos",
  closeAnyway: "Cerrar de todos modos",
  uploading2: "Subiendo...",
  uploadAndLock: "Subir y bloquear",
  uploadAndClose: "Subir y cerrar",
  keepEditing: "Seguir editando",
  uploadAndQuit: "Subir y salir",
  uploadedAs: "Se subió {name} como {renamedTo}",
  uploadedName: "Se subió {name}",
  couldNotUploadKept:
    "No se pudo subir {name}: {message}. El archivo guardado se conserva; vuelve a intentarlo desde la barra de estado.",
  couldNotOpen: "No se pudo abrir {name}: {message}",
  someoneDeletedIt:
    "Alguien lo eliminó de {server} después de que lo abrieras. Lo que guardaste aún no se ha subido.",
  someoneChangedIt:
    "Alguien lo cambió en {server} después de que lo abrieras. Lo que guardaste aún no se ha subido.",
  serverChangedThere: "{server}, cambiado allí",
  savedInProgramUploadNow: "{name} se guardó en {program}. ¿Subirlo a {server} ahora?",
  savedUploadNow: "{name} se guardó. ¿Subirlo a {server} ahora?",
  theProgramThisSystemOpensFilesWith:
    "El programa con el que este sistema abre los archivos {extension}",
  theProgramThisSystemOpensSuchFilesWith: "El programa con el que este sistema abre esos archivos",
  holdsAProgramNotOpened:
    "{name} contiene un programa o un script, así que no se abrió. Poros no ejecuta los archivos que abre: elige un editor para verlo.",
  isAProgramOrAScript:
    "{name} es un programa o un script. Poros no ejecuta los archivos que abre: elige un editor para verlo.",
  whenPorosLastClosed: {
    one: "Cuando Poros se cerró por última vez, este archivo estaba guardado en otro programa, pero no subido. Súbelo cuando su servidor esté conectado o descarta los cambios. La barra de estado conserva la lista.",
    many: "Cuando Poros se cerró por última vez, estos archivos estaban guardados en otro programa, pero no subidos. Súbelos cuando su servidor esté conectado o descarta los cambios. La barra de estado conserva la lista.",
    other:
      "Cuando Poros se cerró por última vez, estos archivos estaban guardados en otro programa, pero no subidos. Súbelos cuando su servidor esté conectado o descarta los cambios. La barra de estado conserva la lista.",
  },
  copiesFromVaultsStayUnencrypted:
    "Hasta entonces, las copias de archivos de bóvedas quedan sin cifrar en este equipo.",
  filesFromThisVaultOpen: {
    one: "{count} archivo de esta bóveda está abierto en otros programas:",
    many: "{count} de archivos de esta bóveda están abiertos en otros programas:",
    other: "{count} archivos de esta bóveda están abiertos en otros programas:",
  },
  filesFromThisServerOpen: {
    one: "{count} archivo de este servidor está abierto en otros programas:",
    many: "{count} de archivos de este servidor están abiertos en otros programas:",
    other: "{count} archivos de este servidor están abiertos en otros programas:",
  },
  couldNotBeUploadedLockingWipes: {
    one: "Este no se pudo subir. Si bloqueas de todos modos, se borran sus cambios.",
    many: "Estos no se pudieron subir. Si bloqueas de todos modos, se borran sus cambios.",
    other: "Estos no se pudieron subir. Si bloqueas de todos modos, se borran sus cambios.",
  },
  couldNotBeUploadedClosingWipes: {
    one: "Este no se pudo subir. Si cierras de todos modos, se borran los cambios de los archivos de la bóveda.",
    many: "Estos no se pudieron subir. Si cierras de todos modos, se borran los cambios de los archivos de la bóveda.",
    other:
      "Estos no se pudieron subir. Si cierras de todos modos, se borran los cambios de los archivos de la bóveda.",
  },
  couldNotBeUploadedClosingLeaves: {
    one: "Este no se pudo subir. Si cierras de todos modos, queda en la lista de la barra de estado para subirlo más tarde.",
    many: "Estos no se pudieron subir. Si cierras de todos modos, quedan en la lista de la barra de estado para subirlos más tarde.",
    other:
      "Estos no se pudieron subir. Si cierras de todos modos, quedan en la lista de la barra de estado para subirlos más tarde.",
  },
  lockingEndsEditing: {
    one: "Al bloquear la bóveda termina la edición de su archivo y se borran las copias de trabajo. Lo que se guardó se sube antes.",
    many: "Al bloquear la bóveda termina la edición de sus archivos y se borran las copias de trabajo. Lo que se guardó se sube antes.",
    other:
      "Al bloquear la bóveda termina la edición de sus archivos y se borran las copias de trabajo. Lo que se guardó se sube antes.",
  },
  closingLocksVaultEndsEditing: {
    one: "Al cerrar la pestaña se bloquea su bóveda, lo que termina la edición de su archivo y borra las copias de trabajo. Lo que se guardó se sube antes.",
    many: "Al cerrar la pestaña se bloquea su bóveda, lo que termina la edición de sus archivos y borra las copias de trabajo. Lo que se guardó se sube antes.",
    other:
      "Al cerrar la pestaña se bloquea su bóveda, lo que termina la edición de sus archivos y borra las copias de trabajo. Lo que se guardó se sube antes.",
  },
  savedButNotReachedTheServer: {
    one: "Este archivo se guardó en otros programas, pero no ha llegado al servidor:",
    many: "Estos archivos se guardaron en otros programas, pero no han llegado al servidor:",
    other: "Estos archivos se guardaron en otros programas, pero no han llegado al servidor:",
  },
  quittingWithoutUploadingKeepsThem:
    "Si sales sin subir, se conservan, y Poros ofrece subirlos la próxima vez que se inicie.",
  workingCopiesOfVaultFilesAreWiped: "Las copias de trabajo de los archivos de bóvedas se borran.",

  // components/ExternalEditorSettings
  openWith: "Abrir con",
  openCtrlEnterUsesThe:
    "Abrir (Ctrl+Enter) usa el programa configurado para el tipo de archivo o, si no hay ninguno, el del sistema. Un archivo del servidor se abre desde una copia en este equipo, que se sube cada vez que el programa la guarda.",
  uploadEachSaveWithoutAsking: "Subir cada guardado sin preguntar",
  uploadEachSaveWithoutAskingHint: "Desactivado: pregunta antes de cada subida.",
  keepCopiesAfterTheProgram: "Conservar las copias después de cerrar el programa",
  keepCopiesAfterTheProgramHint:
    "Contado desde el último guardado o subida. Las copias de archivos de bóvedas se borran de inmediato.",
  removeProgram: "Quitar programa",
  remove: "Quitar {programLabel}",
  program: "Programa",
  arguments: "Argumentos",
  defaultFor: "Predeterminado para",
  filesThisProgramOpensBy: "Archivos que este programa abre de forma predeterminada",
  psdXcf: "*.psd, *.xcf",
  nameOfTheNewProgram: "Nombre del nuevo programa",
  newProgram: "Nuevo programa",
  inTheArgumentsStandsFor:
    "{placeholder} en los argumentos representa el archivo; si no aparece, el archivo va al final. Las comillas mantienen los espacios dentro de un argumento. Los archivos que coinciden con un patrón de Predeterminado para se abren en ese programa.",

  // components/ExternalEditsStatus
  filesOpenInOtherPrograms: "Archivos abiertos en otros programas",
  stopEditing: "¿Dejar de editar {name}?",
  stopAndDiscard: "Dejar de editar y descartar",
  openInOtherPrograms: "Abiertos en otros programas",
  settingsForOpeningFiles: "Configuración para abrir archivos",
  noFilesAreOpenIn:
    "No hay archivos abiertos en otros programas. Haz clic con el botón derecho en un archivo y elige Abrir con.",
  inAVault: "En una bóveda",
  uploadedAtTime: "subido a las {time}",
  uploadedOnDate: "subido el {date}",
  of: "{transferred} de {total}",
  decideWhatToUpload: "Decidir qué subir...",
  openInAnEditor: "Abrir en un editor...",
  uploadNow: "Subir ahora",
  uploadNow2: "Subir {name} ahora",
  openAgain: "Volver a abrir",
  openAgain2: "Volver a abrir {name}",
  showTheWorkingCopysFolder: "Mostrar la carpeta de la copia de trabajo",
  showTheFolderOf: "Mostrar la carpeta de {name}",
  stopEditingAndDeleteThe: "Dejar de editar y eliminar la copia de trabajo",
  stopEditing2: "Dejar de editar {name}",
  savedButNotUploadedWhen: "Guardado, pero sin subir cuando Poros se cerró por última vez",
  saved: "guardado: {date}",
  thisCopyOfAVault:
    "Esta copia de un archivo de bóveda no está cifrada en este equipo. Súbela o descártala.",
  uploadTo: "Subir a {label}",
  connectToFirst: "Conéctate primero a {server}",
  upload2: "Subir {name}",
  discardTheSavedChanges: "Descartar los cambios guardados",
  discard: "Descartar {name}",
  discard2: "¿Descartar {name}?",
  notEditingAnyFiles: "No se está editando ningún archivo",
  eachSaveIsUploadedAs: "Cada guardado se sube en cuanto el programa lo ha escrito.",
  savesWaitForYouTo: "Los guardados esperan a que los subas.",
  changesSavedInProgramNotReached:
    "Los cambios guardados en {program} no han llegado a {server}. Al dejar de editar se elimina la copia de trabajo con ellos.",
  changesSavedInTheProgramNotReached:
    "Los cambios guardados en el programa no han llegado a {server}. Al dejar de editar se elimina la copia de trabajo con ellos.",
  connectAndUnlockToUpload: "Conéctate a {server} y desbloquea su bóveda para subirlo.",
  connectToUpload: "Conéctate a {server} para subirlo.",
  neverReachedDiscardingWipes:
    "Los cambios guardados en él nunca llegaron a {server}. Al descartarlo se borran.",
  neverReachedDiscardingDeletes:
    "Los cambios guardados en él nunca llegaron a {server}. Al descartarlo se eliminan.",

  // components/openWithMenu
  openIn: "Abrir en {programLabel}",
  setUpPrograms: "Configurar programas...",

  // lib/externalEditor
  downloading: "Descargando",
  watchingForSaves: "Vigilando los guardados",
  savedNotUploaded: "Guardado, sin subir",
  waitingToUpload: "Esperando para subir",
  changedOnTheServer2: "Cambiado en el servidor",
  uploading: "Subiendo",
  uploadFailed: "Error al subir",
  notOpenedItIsAProgram: "No se abrió: es un programa",
  openingNamePercent: "Abriendo {name}, {percent}",
  uploadingName: "Subiendo {name}",
  nameNotOpenedItIsAProgram: "{name}: no se abrió: es un programa",
  nameChangedOnTheServer: "{name}: cambiado en el servidor",
  nameUploadFailed: "{name}: error al subir",
  editedFilesNeedAttention: {
    one: "{count} archivo editado necesita atención",
    many: "{count} de archivos editados necesitan atención",
    other: "{count} archivos editados necesitan atención",
  },
  editingName: "Editando {name}",
  editingFiles: {
    one: "Editando {count} archivo",
    many: "Editando {count} de archivos",
    other: "Editando {count} archivos",
  },
  filesNotUploaded: {
    one: "{count} archivo sin subir",
    many: "{count} de archivos sin subir",
    other: "{count} archivos sin subir",
  },

  // state/externalEditStore
  isOpenAlreadyItsWorking: "{name} ya está abierto. Su copia de trabajo se vuelve a abrir.",
  chooseAProgram2: "Elegir un programa",
} satisfies TranslatedArea<"openWith">;
