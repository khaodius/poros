import type { TranslatedArea } from "../catalog";

export const compare = {
  // components/CompareDialog
  compareFiles: "Comparar archivos",
  swapSides: "Intercambiar lados",
  onlyChanges: "Solo cambios",
  ignoreWhitespace: "Ignorar espacios en blanco",
  ignoreCase: "Ignorar mayúsculas y minúsculas",
  onTheLeftOnThe: "{left} a la izquierda, {right} a la derecha.",
  readingBothFiles: "Leyendo los dos archivos...",
  previousChangeAltUp: "Cambio anterior (Alt+Up)",
  previousChange: "Cambio anterior",
  nextChangeAltDown: "Cambio siguiente (Alt+Down)",
  nextChange: "Cambio siguiente",
  changes: { one: "{count} cambio", many: "{count} de cambios", other: "{count} cambios" },
  showUnchangedLines: {
    one: "Mostrar {count} línea sin cambios",
    many: "Mostrar {count} de líneas sin cambios",
    other: "Mostrar {count} líneas sin cambios",
  },
  comparing: "Comparando...",
  noChanges: "Sin cambios",
  changeOf: "Cambio {current} de {count}",

  // components/FolderCompareBar
  synchronizeTheseFolders: "Sincronizar estas carpetas",
  synchronizeTheseFolders2: "Sincronizar estas carpetas ({unavailable})",
  folderComparison: "Comparación de carpetas",
  comparedWith: "Comparado con {where}",
  comparedWithAt: "Comparado con {where}: {path}",
  again: "{action} de nuevo",
  selectByState: "Seleccionar por estado",
  comparisonOptions: "Opciones de comparación",
  stopComparing: "Dejar de comparar ({shortcut})",
  rowsByHowTheyCompare: "Filas según la comparación",
  comparingSubfoldersItemsListedSo: {
    one: "Comparando subcarpetas: {count} elemento listado hasta ahora",
    many: "Comparando subcarpetas: {count} de elementos listados hasta ahora",
    other: "Comparando subcarpetas: {count} elementos listados hasta ahora",
  },
  couldNotCompareSubfolders: "No se pudieron comparar las subcarpetas: {message}",
  thereAreTooManySubfolders:
    "Hay demasiadas subcarpetas para marcarlas todas. Las más profundas no se marcan.",
  thePanesAreNoLonger: "{note}. Los paneles ya no se mueven a la par.",
  inAnotherWindow: "{label} en otra ventana",
  nothingToCompareYet: "aún nada que comparar",
  stateCount: "{state}: {amount}",
  stateCountHidden: "{state}: {amount}, no se muestran",

  // components/folderCompareMenu
  selectNewerAndOnlyHere: "Seleccionar más recientes y solo aquí",
  selectDifferent: "Seleccionar distintos",
  hideIdenticalItems: "Ocultar elementos idénticos",
  compareBySizeOnly: "Comparar solo por tamaño",
  ignoreTimeZoneOffsetsWhole: "Ignorar diferencias de zona horaria (horas enteras)",
  includeHiddenFiles: "Incluir archivos ocultos",
  browseBothPanesTogether: "Navegar por los dos paneles a la vez",
  compareFolders: "Comparar carpetas",
  compareContentsOfSubfolders: "Comparar el contenido de las subcarpetas",

  // hooks/useFolderCompare
  syncOnlyLocalAndSftp: "solo entre este equipo y un servidor SFTP",
  syncFromServerWindow: "desde la ventana con la pestaña del servidor",

  // lib/compare
  lfUnix: "LF (Unix)",
  crlfWindows: "CRLF (Windows)",
  lineEndingNone: "ninguno",
  lineEndingMixed: "mixtos",
  lineEndingsDiffer: "Los finales de línea difieren: {left} a la izquierda, {right} a la derecha.",
  filesIdentical: "Los archivos son idénticos.",
  filesDifferTooLarge:
    "Los archivos son distintos. Son demasiado grandes para mostrarlos uno junto al otro.",
  filesDifferNotText:
    "Los archivos son distintos. No son texto, así que solo se compararon sus bytes.",
  linesChanged: {
    one: "{count} línea cambiada",
    many: "{count} de líneas cambiadas",
    other: "{count} líneas cambiadas",
  },
  linesAdded: {
    one: "{count} línea agregada",
    many: "{count} de líneas agregadas",
    other: "{count} líneas agregadas",
  },
  linesRemoved: {
    one: "{count} línea eliminada",
    many: "{count} de líneas eliminadas",
    other: "{count} líneas eliminadas",
  },
  lineCounts: "{counts}.",
  onlyLineEndingsDiffer: "Solo difieren los finales de línea.",
  filesDifferInIgnored: "Los archivos solo difieren en lo que se ignora.",
  filesDifferInBytes: "Las líneas son iguales; los archivos difieren en sus bytes.",

  // lib/folderCompare
  differsInside: "Difiere por dentro: {found}",
  noDifferencesFoundInsideBut:
    "No se encontraron diferencias por dentro, pero no se comparó todo: {found}",
  identicalInsideFiles: {
    one: "Idéntico por dentro: {count} archivo",
    many: "Idéntico por dentro: {count} de archivos",
    other: "Idéntico por dentro: {count} archivos",
  },
  identicalInsideNoFilesOn: "Idéntico por dentro: ningún archivo en ninguno de los lados",
  newerHere: "Más reciente aquí",
  olderHere: "Más antiguo aquí",
  onlyHere: "Solo aquí",
  different: "Distinto",
  identical: "Idéntico",
  differentAndNeitherIsNewer: "Distinto, y ninguno es más reciente",
  onBothSidesTheContents: "En los dos lados. El contenido no se compara.",
  neitherListingRefreshed:
    "No se pudo actualizar el listado de ningún panel, así que las marcas pueden estar desfasadas hasta que se vuelvan a listar los dos",
  thisListingNotRefreshed:
    "No se pudo actualizar el listado de este panel, así que las marcas pueden estar desfasadas hasta que se vuelva a listar",
  partnerListingNotRefreshed:
    "No se pudo actualizar el listado de {partner}, así que las marcas pueden estar desfasadas hasta que se vuelva a listar",
  pausedBothShowResults:
    "La comparación está en pausa mientras los dos paneles muestran resultados de búsqueda. Las marcas vuelven al cerrar los resultados",
  pausedThisShowsResults:
    "La comparación está en pausa mientras este panel muestra resultados de búsqueda. Las marcas vuelven al cerrar los resultados",
  pausedPartnerShowsResults:
    "La comparación está en pausa mientras {partner} muestra resultados de búsqueda. Las marcas vuelven al cerrar los resultados",
  summaryNewerHere: {
    one: "{count} más reciente aquí",
    many: "{count} más recientes aquí",
    other: "{count} más recientes aquí",
  },
  summaryOlderHere: {
    one: "{count} más antiguo aquí",
    many: "{count} más antiguos aquí",
    other: "{count} más antiguos aquí",
  },
  summaryOnlyHere: {
    one: "{count} solo aquí",
    many: "{count} solo aquí",
    other: "{count} solo aquí",
  },
  summaryOnlyThere: {
    one: "{count} solo en el otro panel",
    many: "{count} solo en el otro panel",
    other: "{count} solo en el otro panel",
  },
  summaryDifferent: {
    one: "{count} distinto",
    many: "{count} distintos",
    other: "{count} distintos",
  },
  summaryIdentical: {
    one: "{count} idéntico",
    many: "{count} idénticos",
    other: "{count} idénticos",
  },
  summaryNotCompared: {
    one: "{count} sin comparar",
    many: "{count} sin comparar",
    other: "{count} sin comparar",
  },

  // state/folderCompare
  noFolderToCompare: "Este panel no tiene ninguna carpeta que comparar",
  openASecondPane: "Abre un segundo panel de carpeta con el que comparar este",
  paneShowsResults: "{pane} muestra resultados de búsqueda",
  noFolderNamed: "No hay ninguna carpeta llamada {name} en {pane}",
  noFolderAbove: "{pane} no tiene ninguna carpeta por encima de esta",
  paneCouldNotOpen: "{pane} no pudo abrir {path}",
} satisfies TranslatedArea<"compare">;
