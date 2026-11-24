import type { TranslatedArea } from "../catalog";

export const fileList = {
  // components/FileList
  expandGroup: "Gruppe ausklappen",
  collapseGroup: "Gruppe einklappen",
  newNameFor: "Neuer Name für {name}",
  resize: "Breite von {label} ändern",
  dragToResizeDoubleClick: "Ziehen ändert die Breite, Doppelklick passt sie an",
  nothingMatchesTheFilter: "Nichts passt zum Filter",
  onlyHiddenItemsHere: "Hier gibt es nur versteckte Elemente",
  thisFolderIsEmpty: "Dieser Ordner ist leer",
  deletedTag: "gelöscht",

  // components/PropertiesDialog
  folder: "Ordner",
  brokenLink: "Defekte Verknüpfung",
  file: "Datei",

  // lib/columns
  type: "Typ",
  modified: "Geändert",
  permissions: "Rechte",
  owner: "Besitzer",

  // lib/dateGroups
  inTheFuture: "In der Zukunft",
  today: "Heute",
  yesterday: "Gestern",
  earlierThisWeek: "Anfang dieser Woche",
  lastWeek: "Letzte Woche",
  earlierThisMonth: "Anfang dieses Monats",
  lastMonth: "Letzten Monat",
  earlierThisYear: "Anfang dieses Jahres",
  aLongTimeAgo: "Vor langer Zeit",
  dateUnknown: "Datum unbekannt",
} satisfies TranslatedArea<"fileList">;
