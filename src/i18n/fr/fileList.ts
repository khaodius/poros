import type { TranslatedArea } from "../catalog";

export const fileList = {
  // components/FileList
  expandGroup: "Développer le groupe",
  collapseGroup: "Réduire le groupe",
  newNameFor: "Nouveau nom de {name}",
  resize: "Redimensionner «\u00a0{label}\u00a0»",
  dragToResizeDoubleClick: "Faites glisser pour redimensionner, double-cliquez pour ajuster",
  nothingMatchesTheFilter: "Rien ne correspond au filtre",
  onlyHiddenItemsHere: "Seulement des éléments cachés ici",
  thisFolderIsEmpty: "Ce dossier est vide",
  deletedTag: "supprimé",

  // components/PropertiesDialog
  folder: "Dossier",
  brokenLink: "Lien rompu",
  file: "Fichier",

  // lib/columns
  type: "Type",
  modified: "Modifié",
  permissions: "Permissions",
  owner: "Propriétaire",

  // lib/dateGroups
  inTheFuture: "Dans le futur",
  today: "Aujourd’hui",
  yesterday: "Hier",
  earlierThisWeek: "Plus tôt cette semaine",
  lastWeek: "La semaine dernière",
  earlierThisMonth: "Plus tôt ce mois-ci",
  lastMonth: "Le mois dernier",
  earlierThisYear: "Plus tôt cette année",
  aLongTimeAgo: "Il y a longtemps",
  dateUnknown: "Date inconnue",
} satisfies TranslatedArea<"fileList">;
