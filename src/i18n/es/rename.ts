import type { TranslatedArea } from "../catalog";

export const rename = {
  // components/BulkRenameDialog
  nothingToRename: "Nada a lo que cambiar el nombre",
  renameItems: {
    one: "Cambiar el nombre de {count} elemento",
    many: "Cambiar el nombre de {count} de elementos",
    other: "Cambiar el nombre de {count} elementos",
  },
  appliedInThisOrderEach: "Se aplican en este orden, cada uno al resultado del anterior.",
  findAndReplace: "Buscar y reemplazar",
  find: "Buscar",
  regularExpression: "Expresión regular",
  recentPatterns: "Patrones recientes",
  recent: "Recientes",
  replaceWith: "Reemplazar por",
  replaceWith1OrName: "Reemplazar por; $1 o $<name> para los grupos",
  patternIs: "El patrón es",
  matchCase: "Coincidir mayúsculas y minúsculas",
  everyMatch: "Todas las coincidencias",
  lookIn: "Buscar en",
  template: "Plantilla",
  templateForTheNewName: "Plantilla para el nuevo nombre",
  insertAToken: "Insertar un elemento",
  dateAs: "Fecha como",
  timeAs: "Hora como",
  theDateAndTimeAre:
    "La fecha y la hora son las de la última modificación del elemento. Sin {ext}, los elementos conservan su extensión.",
  counter: "Contador",
  step: "Incremento",
  digits: "Dígitos",
  countsIn: "Cuenta en",
  listOrder: "Orden de la lista",
  nameOrder: "Orden por nombre",
  theTemplatePlacesTheCounter: "La plantilla coloca el contador donde está {n}.",
  addToEachName: "Agregar a cada nombre",
  separator: "Separador",
  case: "Mayúsculas",
  extension: "Extensión",
  newExtension: "Nueva extensión",
  newExtensionSuchAsJpg: "Nueva extensión, como jpg",
  trimAndTidy: "Recortar y ordenar",
  trimSpacesAtTheEnds: "Quitar los espacios de los extremos",
  makeRepeatedSpacesOne: "Convertir los espacios repetidos en uno",
  replaceSpacesWith: "Reemplazar los espacios por",
  characterThatReplacesSpaces: "Carácter que reemplaza a los espacios",
  rowsShown: "Filas mostradas",
  oldAndNewNames: "Nombres anteriores y nuevos",
  noNamesChangeYet: "Todavía no cambia ningún nombre.",
  couldNotRefreshThisFolder:
    "No se pudo actualizar esta carpeta. Puede que este listado esté desfasado.",
  resetRules: "Restablecer reglas",
  ofItemsWillBeRenamed: {
    one: "{ready} de {count} elemento cambiará de nombre.",
    many: "{ready} de {count} de elementos cambiarán de nombre.",
    other: "{ready} de {count} elementos cambiarán de nombre.",
  },
  ofItemsWillBeRenamedSomeUnchanged: {
    one: "{ready} de {count} elemento cambiará de nombre; sin cambios: {unchanged}.",
    many: "{ready} de {count} de elementos cambiarán de nombre; sin cambios: {unchanged}.",
    other: "{ready} de {count} elementos cambiarán de nombre; sin cambios: {unchanged}.",
  },
  text: "Texto",
  regex: "Regex",
  both: "Ambos",
  unchanged: "Sin cambios",
  cannotBeRenamed: "No se puede cambiar el nombre",
  nameUnchanged: "Sin cambios",
  ready: "Listo",
  lowercase: "minúsculas",
  uppercase: "MAYÚSCULAS",
  titleCase: "Tipo Título",
  sentenceCase: "Tipo oración",
  beforeTheName: "Antes del nombre",
  afterTheName: "Después del nombre",
  all: "Todos",
  changed: "Cambiados",
  problems: "Problemas",
  checkingTheFolder: "Comprobando la carpeta...",
  theseItemsAreNoLongerInTheFolder: "Estos elementos ya no están en la carpeta.",
  selectedItemsLeftOut: {
    one: "{count} elemento seleccionado ya no está en la carpeta y se omite.",
    many: "{count} de elementos seleccionados ya no están en la carpeta y se omiten.",
    other: "{count} elementos seleccionados ya no están en la carpeta y se omiten.",
  },

  // components/RenameReportDialog
  renamingStopped: "Cambio de nombre detenido",
  renamedOfItemsIn: {
    one: "{count} de {total} en {folder} tiene su nuevo nombre.",
    many: "{count} de {total} en {folder} tienen su nuevo nombre.",
    other: "{count} de {total} en {folder} tienen su nuevo nombre.",
  },
  theOthersKeepTheirOldNamesButOne:
    "Los demás conservan sus nombres anteriores, salvo el de abajo.",
  theOthersKeepTheirOldNames: "Los demás conservan sus nombres anteriores.",
  items: { one: "{count} elemento", many: "{count} de elementos", other: "{count} elementos" },
  couldNotBeRenamedTo: "No se pudo cambiar el nombre de {from} a {to}: {message}",
  isNowNamedBecause:
    "{from} ahora se llama {temporary}, porque no se le pudo devolver su nombre: {message}. Cámbiale el nombre a mano.",
  whatBecameOfEachRename: "Qué pasó con cada cambio de nombre",
  noItems: "Ningún elemento.",
  renamed: "Renombrado",
  notRenamed: "Sin renombrar",
  failed: "Con error",
  leftUnderAnotherName: "Quedó con otro nombre",

  // components/RenameTable
  newName: "Nuevo nombre",
  status: "Estado",
  noName: "Sin nombre",

  // lib/bulkRename
  theNameIsEmpty: "El nombre está vacío",
  theNameIsTooLongInBytes: {
    one: "El nombre es demasiado largo: {length} de {count} byte",
    many: "El nombre es demasiado largo: {length} de {count} de bytes",
    other: "El nombre es demasiado largo: {length} de {count} bytes",
  },
  theNameIsTooLongInCharacters: {
    one: "El nombre es demasiado largo: {length} de {count} carácter",
    many: "El nombre es demasiado largo: {length} de {count} de caracteres",
    other: "El nombre es demasiado largo: {length} de {count} caracteres",
  },
  nowNamed: "Ahora se llama {temporary}: {message}",
  nameWithoutTheExtension: "Nombre sin la extensión",
  dateModified: "Fecha de modificación",
  timeModified: "Hora de modificación",
  nameOfTheFolder: "Nombre de la carpeta",
  isNotAName: '"{name}" no es un nombre',
  namesCannotContainControlCharacters: "Los nombres no pueden contener caracteres de control",
  namesCannotContain: "Los nombres no pueden contener {character}",
  namesCannotEndWithASpaceOrADot: "Los nombres no pueden terminar en un espacio ni en un punto",
  isAReservedNameOnWindows: "{name} es un nombre reservado en Windows",
  keepsThisName: "{name} conserva este nombre",
  keepsANameDifferingInCase:
    "{name} conserva un nombre que solo difiere en mayúsculas y minúsculas",
  keepsANameDifferingInAccents:
    "{name} conserva un nombre que solo difiere en cómo se guardan sus tildes",
  getsTheSameName: "{name} recibe el mismo nombre",
  getsANameDifferingInCase: "{name} recibe un nombre que solo difiere en mayúsculas y minúsculas",
  getsANameDifferingInAccents:
    "{name} recibe un nombre que solo difiere en cómo se guardan sus tildes",
  anotherSelectedItemHasTheSameName:
    "Otro elemento seleccionado tiene el mismo nombre; cámbiales el nombre de uno en uno",
  isSelectedTooAndCountsAsTheSameName:
    "{name} también está seleccionado y aquí cuenta como el mismo nombre; cámbiales el nombre de uno en uno",
  noModifiedTimeFor: "El elemento no tiene hora de modificación para {date} o {time}",
  anItemThatIsNotSelectedHasThisName: "Un elemento que no está seleccionado tiene este nombre",
  notSelectedAndHasANameDifferingInCase:
    "{name} no está seleccionado y tiene un nombre que solo difiere en mayúsculas y minúsculas",
  notSelectedAndHasANameDifferingInAccents:
    "{name} no está seleccionado y tiene un nombre que solo difiere en cómo se guardan sus tildes",
  problemSharedName: {
    one: "{count} elemento tiene el mismo nombre que otro elemento seleccionado",
    many: "{count} de elementos tienen el mismo nombre que otro elemento seleccionado",
    other: "{count} elementos tienen el mismo nombre que otro elemento seleccionado",
  },
  problemEmpty: {
    one: "{count} nombre está vacío",
    many: "{count} de nombres están vacíos",
    other: "{count} nombres están vacíos",
  },
  problemInvalid: {
    one: "{count} nombre no se puede usar aquí",
    many: "{count} de nombres no se pueden usar aquí",
    other: "{count} nombres no se pueden usar aquí",
  },
  problemTooLong: {
    one: "{count} nombre es demasiado largo",
    many: "{count} de nombres son demasiado largos",
    other: "{count} nombres son demasiado largos",
  },
  problemUndated: {
    one: "{count} elemento no tiene hora de modificación",
    many: "{count} de elementos no tienen hora de modificación",
    other: "{count} elementos no tienen hora de modificación",
  },
  problemDuplicate: {
    one: "{count} elemento compartiría nombre",
    many: "{count} de elementos compartirían nombre",
    other: "{count} elementos compartirían nombre",
  },
  problemTaken: {
    one: "{count} nombre ya está en uso",
    many: "{count} de nombres ya están en uso",
    other: "{count} nombres ya están en uso",
  },
  theRegularExpressionIsNotValid: "La expresión regular no es válida: {error}",
  problemsSentence: "{problems}.",

  // state/bulkRename
  renamingCancelledRenamed: {
    one: "Cambio de nombre cancelado. Renombrados: {renamed} de {count} elemento.",
    many: "Cambio de nombre cancelado. Renombrados: {renamed} de {count} de elementos.",
    other: "Cambio de nombre cancelado. Renombrados: {renamed} de {count} elementos.",
  },
  renamingItemsIn: {
    one: "Cambiando el nombre de {count} elemento en {folder}",
    many: "Cambiando el nombre de {count} de elementos en {folder}",
    other: "Cambiando el nombre de {count} elementos en {folder}",
  },
} satisfies TranslatedArea<"rename">;
