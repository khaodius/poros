import type { TranslatedArea } from "../catalog";

export const versions = {
  // components/VersionsDialog
  restoreThisVersion: "Restaurar esta versión",
  downloadThisVersion: "Descargar esta versión",
  openInTheEditor: "Abrir en el editor",
  compareWithTheCurrentFile: "Comparar con el archivo actual",
  downloadingTheVersionFromTo:
    "Descargando la versión del {when} en {folder}. La cola de transferencias muestra el progreso.",
  versionsOf: "Versiones de {name}",
  loadingVersions: "Cargando versiones...",
  turnOnVersioningForThe:
    "Este bucket no conserva versiones. Activa el control de versiones del bucket para conservar los archivos que reemplaza y elimina.",
  noVersionsOfThisFile: "No se encontraron versiones de este archivo.",
  saved: "Guardada",
  status: "Estado",
  storageClass: "Clase de almacenamiento",
  etag: "ETag",
  actions: "Acciones",
  version: "Versión {versionId}",
  entries: { one: "{count} entrada", many: "{count} de entradas", other: "{count} entradas" },
  restoreVersion: "Restaurar versión",
  restore: "Restaurar",
  makeTheVersionOf: "¿Convertir otra vez la versión de {name} del {when} en el archivo actual?",
  thisBringsTheDeletedFile:
    "Esto recupera el archivo eliminado. La eliminación queda en el historial.",
  theCurrentContentStaysIn:
    "El contenido actual queda en el historial, así que también se puede restaurar.",
  removeDeleteMarker: "Quitar marcador de eliminación",
  deleteVersion: "Eliminar versión",
  removeMarker: "Quitar marcador",
  removeTheDeleteMarkerOf:
    "¿Quitar definitivamente el marcador de eliminación de {name} del {when}? Esta acción no se puede deshacer.",
  deleteTheVersionOf:
    "¿Eliminar definitivamente la versión de {name} del {when}? Esta acción no se puede deshacer.",
  deleteForGood: "Eliminar definitivamente",
  deletedItems: {
    one: "{count} elemento eliminado",
    many: "{count} de elementos eliminados",
    other: "{count} elementos eliminados",
  },
  everythingDeletedUnderAFolder:
    "También desaparece todo lo eliminado dentro de una carpeta. Los archivos que siguen en ella no se tocan.",
  andMore: { one: "y {count} más", many: "y {count} más", other: "y {count} más" },
  removeThisDeleteMarker: "Quitar este marcador de eliminación",
  deleteThisVersionForGood: "Eliminar definitivamente esta versión",
  versionFrom: "Versión del {when}",
  versionFromUnknownTime: "Versión de fecha desconocida",
  currentVersion: "Versión actual",
  isNoLongerDeleted: "{name} ya no está eliminado.",
  theseItemsAreNoLongerDeleted: "Estos elementos ya no están eliminados.",
  removeWithEveryVersion:
    "¿Quitar definitivamente {what} y todas las versiones que se conservan? Esta acción no se puede deshacer.",

  // components/versionMenu
  versions: "Versiones...",
  versionsThisBucketKeepsNone: "Versiones (este bucket no conserva ninguna)",
  showDeletedFiles: "Mostrar archivos eliminados",
  showDeletedFilesThisBucket: "Mostrar archivos eliminados (este bucket no conserva versiones)",
  restoreItems: {
    one: "Restaurar {count} elemento",
    many: "Restaurar {count} de elementos",
    other: "Restaurar {count} elementos",
  },
  copyPaths: "Copiar rutas",
  copyPath: "Copiar ruta",
  deleteItemsForGood: {
    one: "Eliminar definitivamente {count} elemento",
    many: "Eliminar definitivamente {count} de elementos",
    other: "Eliminar definitivamente {count} elementos",
  },

  // hooks/usePaneVersions
  deletedFilesCannotBeShown: "No se pueden mostrar los archivos eliminados: {message}",

  // lib/versions
  current: "Actual",
  deleted: "Eliminado",
  deleteMarker: "Marcador de eliminación",
  restoredCount: {
    one: "{count} restaurado",
    many: "{count} restaurados",
    other: "{count} restaurados",
  },
  findingWhatToRestore: "Buscando lo que hay que restaurar...",
  restoredFoundSoFar: {
    one: "{count} restaurado; encontrados hasta ahora: {total}",
    many: "{count} restaurados; encontrados hasta ahora: {total}",
    other: "{count} restaurados; encontrados hasta ahora: {total}",
  },
  restoredOfTotal: {
    one: "{count} de {total} restaurado",
    many: "{count} de {total} restaurados",
    other: "{count} de {total} restaurados",
  },
  nothingOfTheFileIsLeft: "Después de esto no queda nada del archivo.",
  theFileComesBackAsItsNewestVersion: "El archivo vuelve con su versión más reciente.",
  theFileStaysDeleted:
    "El archivo sigue eliminado: hay otro marcador de eliminación debajo de este.",
  thisIsTheOnlyVersion: "Esta es la única versión: el archivo desaparecerá.",
  theVersionBeforeItBecomesCurrent: "La versión anterior pasa a ser el archivo actual.",
  theFileWillShowAsDeleted:
    "El archivo aparecerá como eliminado: la entrada anterior es un marcador de eliminación.",
  unknownTime: "Hora desconocida",
  versioningIsSuspended:
    "El control de versiones está suspendido en este bucket: lo que conservó se queda, y las nuevas escrituras reemplazan la versión marcada como null.",
  theseKeysMayNotSeeVersioning:
    "Puede que estas claves no vean si el bucket conserva versiones, así que algunas acciones podrían rechazarse.",

  // state/versionActions
  downloadAVersionOfTo: "Descargar una versión de {name} en",
  restoringItemsIn: {
    one: "Restaurando {count} elemento en {folder}",
    many: "Restaurando {count} de elementos en {folder}",
    other: "Restaurando {count} elementos en {folder}",
  },
  restoringNameIn: "Restaurando {name} en {folder}",
  deletingItemsForGoodIn: {
    one: "Eliminando definitivamente {count} elemento en {folder}",
    many: "Eliminando definitivamente {count} de elementos en {folder}",
    other: "Eliminando definitivamente {count} elementos en {folder}",
  },
  deletingNameForGoodIn: "Eliminando definitivamente {name} en {folder}",
  filesCouldNotBeRestored: {
    one: "No se pudo restaurar {count} archivo. El registro indica el motivo.",
    many: "No se pudieron restaurar {count} de archivos. El registro indica el motivo.",
    other: "No se pudieron restaurar {count} archivos. El registro indica el motivo.",
  },
  filesCouldNotBeDeleted: {
    one: "No se pudo eliminar {count} archivo. El registro indica el motivo.",
    many: "No se pudieron eliminar {count} de archivos. El registro indica el motivo.",
    other: "No se pudieron eliminar {count} archivos. El registro indica el motivo.",
  },
} satisfies TranslatedArea<"versions">;
