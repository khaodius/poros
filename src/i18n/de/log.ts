import type { TranslatedArea } from "../catalog";

export const log = {
  // components/LogView
  saveLog: "Protokoll speichern",
  levels: "Stufen",
  session: "Sitzung",
  allSessions: "Alle Sitzungen",
  searchLog: "Protokoll durchsuchen",
  clearSearch: "Suche leeren",
  showTimes: "Uhrzeiten zeigen",
  wrapLongLines: "Lange Zeilen umbrechen",
  followNewLines: "Neuen Zeilen folgen",
  copyShownLines: "Angezeigte Zeilen kopieren",
  saveShownLinesToA: "Angezeigte Zeilen in einer Datei speichern",
  clearLog: "Protokoll leeren",
  info: "Info",
  warnings: "Warnungen",
  errors: "Fehler",
  server: "Server",
  closedSession: "Geschlossene Sitzung {id}",
  connectionAndTransferActivityAppears: "Verbindungs- und Übertragungsaktivität erscheint hier.",
  noLinesMatchTheFilters: "Keine Zeilen passen zu den Filtern.",
  logFiles: "Protokolldateien",
} satisfies TranslatedArea<"log">;
