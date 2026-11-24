import type { TranslatedArea } from "../catalog";

export const dialogs = {
  // components/ConflictDialog
  fileAlreadyExists: "El archivo ya existe",
  alreadyExistsIn: "{name} ya existe en {folder}.",
  modified: "Modificado",
  existingFile: "Archivo existente",
  overwriteIfNewer: "Sobrescribir si es más reciente",
  keepBoth: "Conservar ambos",
  localFile: "Archivo local",
  serverFile: "Archivo del servidor",
  doTheSameForAny: "Hacer lo mismo con los demás conflictos de estas transferencias",
  doTheSameForOthers: {
    one: "Hacer lo mismo con el otro conflicto y con los que surjan",
    many: "Hacer lo mismo con los otros {count} de conflictos y con los que surjan",
    other: "Hacer lo mismo con los otros {count} conflictos y con los que surjan",
  },

  // components/DeleteDialog
  nothingToDelete: "Nada que eliminar",
  deleteItem: "Eliminar elemento",
  deleteItems: {
    one: "Eliminar {count} elemento",
    many: "Eliminar {count} de elementos",
    other: "Eliminar {count} elementos",
  },
  checkingTheFolder: "Comprobando la carpeta...",
  theseItemsAreNoLonger: "Estos elementos ya no están en la carpeta.",

  // components/PropertiesDialog
  sizeWithContents: "{size} ({contents})",
  items: { one: "{count} elemento", many: "{count} de elementos", other: "{count} elementos" },
  files: { one: "{count} archivo", many: "{count} de archivos", other: "{count} archivos" },
  folders: { one: "{count} carpeta", many: "{count} de carpetas", other: "{count} carpetas" },
  countingFiles: {
    one: "Contando: {count} archivo, {size}",
    many: "Contando: {count} de archivos, {size}",
    other: "Contando: {count} archivos, {size}",
  },
  propertiesOf: "Propiedades de {name}",
  propertiesOfItems: {
    one: "Propiedades de {count} elemento",
    many: "Propiedades de {count} de elementos",
    other: "Propiedades de {count} elementos",
  },
  items2: "Elementos",
  links: { one: "{count} vínculo", many: "{count} de vínculos", other: "{count} vínculos" },
  type: "Tipo",
  location: "Ubicación",
  pointsTo: "Apunta a",
  calculate: "Calcular",
  accessed: "Último acceso",
  mixedUnchanged: "Mixto, sin cambios",
  permissions: "Permisos",
  linksHaveNoPermissionsOf:
    "Los vínculos no tienen permisos propios, así que conservan los suyos. Los cambios de propietario y grupo se aplican a los propios vínculos.",
  andMoreListedInThe: {
    one: "y {count} más, que aparece en el registro",
    many: "y {count} más, que aparecen en el registro",
    other: "y {count} más, que aparecen en el registro",
  },
  changedItems: {
    one: "Se cambió {count} elemento",
    many: "Se cambiaron {count} de elementos",
    other: "Se cambiaron {count} elementos",
  },
  octal: "Octal",
  mixed: "Mixto",
  questionMarksAreLeftAs: "Los signos de interrogación quedan como los tenga cada elemento",
  owner: "Propietario",
  group: "Grupo",
  folders2: "Carpetas",
  files2: "Archivos",
  oneItemCouldNotBe: "No se pudo cambiar un elemento:",
  itemsCouldNotBeChanged: {
    one: "No se pudo cambiar {count} elemento:",
    many: "No se pudieron cambiar {count} de elementos:",
    other: "No se pudieron cambiar {count} elementos:",
  },
  applying: "Aplicando...",
  linkToAFolder: "Vínculo a una carpeta",
  linkToAFile: "Vínculo a un archivo",
  specialFile: "Archivo especial",
  extensionFile: "Archivo {extension}",
  nameWithId: "{name} ({id})",
  measuringFolderSize: "Midiendo el tamaño de la carpeta",
  changingPermissionsOf: "Cambiando los permisos de {what}",
  applyToEverythingInside: {
    one: "Aplicar a todo lo que hay dentro de la carpeta",
    many: "Aplicar a todo lo que hay dentro de las carpetas",
    other: "Aplicar a todo lo que hay dentro de las carpetas",
  },
  permissionOfClass: "{who}: {permission}",

  // lib/deletion
  deletedCount: {
    one: "{count} eliminado",
    many: "{count} eliminados",
    other: "{count} eliminados",
  },
  countingToDelete: "Contando lo que hay que eliminar...",
  deletedFoundSoFar: {
    one: "{count} eliminado; encontrados hasta ahora: {found}",
    many: "{count} eliminados; encontrados hasta ahora: {found}",
    other: "{count} eliminados; encontrados hasta ahora: {found}",
  },
  deletedOfTotal: {
    one: "Eliminado {count} de {total}",
    many: "Eliminados {count} de {total}",
    other: "Eliminados {count} de {total}",
  },

  // lib/permissions
  setUserId: "Establecer ID de usuario",
  runsAsTheFilesOwner: "Se ejecuta como el propietario del archivo",
  setGroupId: "Establecer ID de grupo",
  runsAsTheGroupNew: "Se ejecuta como el grupo; los archivos nuevos lo heredan",
  sticky: "Bit sticky",
  onlyOwnersMayDeleteOr: "Solo los propietarios pueden eliminar entradas o cambiarles el nombre",
  others: "Otros",
  read: "Lectura",
  write: "Escritura",
  execute: "Ejecución",
} satisfies TranslatedArea<"dialogs">;
