import type { TranslatedArea } from "../catalog";

export const fileList = {
  // components/FileList
  expandGroup: "Expandir grupo",
  collapseGroup: "Contraer grupo",
  newNameFor: "Nuevo nombre para {name}",
  resize: "Cambiar el tamaño de {label}",
  dragToResizeDoubleClick: "Arrastra para cambiar el tamaño, haz doble clic para ajustar",
  nothingMatchesTheFilter: "Nada coincide con el filtro",
  onlyHiddenItemsHere: "Aquí solo hay elementos ocultos",
  thisFolderIsEmpty: "Esta carpeta está vacía",
  deletedTag: "eliminado",

  // components/PropertiesDialog
  folder: "Carpeta",
  brokenLink: "Vínculo roto",
  file: "Archivo",

  // lib/columns
  type: "Tipo",
  modified: "Modificado",
  permissions: "Permisos",
  owner: "Propietario",

  // lib/dateGroups
  inTheFuture: "En el futuro",
  today: "Hoy",
  yesterday: "Ayer",
  earlierThisWeek: "Antes esta semana",
  lastWeek: "La semana pasada",
  earlierThisMonth: "Antes este mes",
  lastMonth: "El mes pasado",
  earlierThisYear: "Antes este año",
  aLongTimeAgo: "Hace mucho tiempo",
  dateUnknown: "Fecha desconocida",
} satisfies TranslatedArea<"fileList">;
