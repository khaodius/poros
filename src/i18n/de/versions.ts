import type { TranslatedArea } from "../catalog";

export const versions = {
  // components/VersionsDialog
  restoreThisVersion: "Diese Version wiederherstellen",
  downloadThisVersion: "Diese Version herunterladen",
  openInTheEditor: "Im Editor öffnen",
  compareWithTheCurrentFile: "Mit der aktuellen Datei vergleichen",
  downloadingTheVersionFromTo:
    "Die Version vom {when} wird nach {folder} heruntergeladen. Die Warteschlange zeigt den Fortschritt.",
  versionsOf: "Versionen von {name}",
  loadingVersions: "Versionen werden geladen...",
  turnOnVersioningForThe:
    "Dieser Bucket bewahrt keine Versionen auf. Schalten Sie die Versionierung für den Bucket ein, um die Dateien zu behalten, die er ersetzt und löscht.",
  noVersionsOfThisFile: "Für diese Datei wurden keine Versionen gefunden.",
  saved: "Gespeichert",
  status: "Status",
  storageClass: "Speicherklasse",
  etag: "ETag",
  actions: "Aktionen",
  version: "Version {versionId}",
  entries: { one: "{count} Eintrag", other: "{count} Einträge" },
  restoreVersion: "Version wiederherstellen",
  restore: "Wiederherstellen",
  makeTheVersionOf: "Die Version von {name} vom {when} wieder zur aktuellen Datei machen?",
  thisBringsTheDeletedFile:
    "Das holt die gelöschte Datei zurück. Das Löschen bleibt im Verlauf erhalten.",
  theCurrentContentStaysIn:
    "Der aktuelle Inhalt bleibt im Verlauf und lässt sich seinerseits wiederherstellen.",
  removeDeleteMarker: "Löschmarkierung entfernen",
  deleteVersion: "Version löschen",
  removeMarker: "Markierung entfernen",
  removeTheDeleteMarkerOf:
    "Die Löschmarkierung von {name} vom {when} endgültig entfernen? Das kann nicht rückgängig gemacht werden.",
  deleteTheVersionOf:
    "Die Version von {name} vom {when} endgültig löschen? Das kann nicht rückgängig gemacht werden.",
  deleteForGood: "Endgültig löschen",
  deletedItems: { one: "{count} gelöschtes Element", other: "{count} gelöschte Elemente" },
  everythingDeletedUnderAFolder:
    "Alles, was unter einem Ordner gelöscht wurde, wird mit entfernt. Dateien, die noch darin sind, bleiben unberührt.",
  andMore: { one: "und {count} weiteres", other: "und {count} weitere" },
  removeThisDeleteMarker: "Diese Löschmarkierung entfernen",
  deleteThisVersionForGood: "Diese Version endgültig löschen",
  versionFrom: "Version vom {when}",
  versionFromUnknownTime: "Version mit unbekanntem Datum",
  currentVersion: "Aktuelle Version",
  isNoLongerDeleted: "{name} ist nicht mehr gelöscht.",
  theseItemsAreNoLongerDeleted: "Diese Elemente sind nicht mehr gelöscht.",
  removeWithEveryVersion:
    "{what} und jede davon aufbewahrte Version endgültig entfernen? Das kann nicht rückgängig gemacht werden.",

  // components/versionMenu
  versions: "Versionen...",
  versionsThisBucketKeepsNone: "Versionen (dieser Bucket bewahrt keine auf)",
  showDeletedFiles: "Gelöschte Dateien zeigen",
  showDeletedFilesThisBucket:
    "Gelöschte Dateien zeigen (dieser Bucket bewahrt keine Versionen auf)",
  restoreItems: {
    one: "{count} Element wiederherstellen",
    other: "{count} Elemente wiederherstellen",
  },
  copyPaths: "Pfade kopieren",
  copyPath: "Pfad kopieren",
  deleteItemsForGood: {
    one: "{count} Element endgültig löschen",
    other: "{count} Elemente endgültig löschen",
  },

  // hooks/usePaneVersions
  deletedFilesCannotBeShown: "Gelöschte Dateien können nicht gezeigt werden: {message}",

  // lib/versions
  current: "Aktuell",
  deleted: "Gelöscht",
  deleteMarker: "Löschmarkierung",
  restoredCount: { one: "{count} wiederhergestellt", other: "{count} wiederhergestellt" },
  findingWhatToRestore: "Wiederherzustellendes wird gesucht...",
  restoredFoundSoFar: {
    one: "{count} wiederhergestellt, bisher {total} gefunden",
    other: "{count} wiederhergestellt, bisher {total} gefunden",
  },
  restoredOfTotal: {
    one: "{count} von {total} wiederhergestellt",
    other: "{count} von {total} wiederhergestellt",
  },
  nothingOfTheFileIsLeft: "Danach bleibt nichts von der Datei übrig.",
  theFileComesBackAsItsNewestVersion: "Die Datei kehrt als ihre neueste Version zurück.",
  theFileStaysDeleted:
    "Die Datei bleibt gelöscht: Unter dieser liegt eine weitere Löschmarkierung.",
  thisIsTheOnlyVersion: "Dies ist die einzige Version: Die Datei ist danach weg.",
  theVersionBeforeItBecomesCurrent: "Die Version davor wird zur aktuellen Datei.",
  theFileWillShowAsDeleted:
    "Die Datei erscheint als gelöscht: Der Eintrag davor ist eine Löschmarkierung.",
  unknownTime: "Unbekannte Zeit",
  versioningIsSuspended:
    "Die Versionierung ist für diesen Bucket ausgesetzt: Was er aufbewahrt hat, bleibt, und neue Schreibvorgänge ersetzen die als null markierte Version.",
  theseKeysMayNotSeeVersioning:
    "Diese Schlüssel sehen womöglich nicht, ob der Bucket Versionen aufbewahrt, daher werden manche Aktionen vielleicht abgelehnt.",

  // state/versionActions
  downloadAVersionOfTo: "Eine Version von {name} herunterladen nach",
  restoringItemsIn: {
    one: "{count} Element in {folder} wird wiederhergestellt",
    other: "{count} Elemente in {folder} werden wiederhergestellt",
  },
  restoringNameIn: "{name} in {folder} wird wiederhergestellt",
  deletingItemsForGoodIn: {
    one: "{count} Element in {folder} wird endgültig gelöscht",
    other: "{count} Elemente in {folder} werden endgültig gelöscht",
  },
  deletingNameForGoodIn: "{name} in {folder} wird endgültig gelöscht",
  filesCouldNotBeRestored: {
    one: "{count} Datei konnte nicht wiederhergestellt werden. Das Protokoll nennt den Grund.",
    other:
      "{count} Dateien konnten nicht wiederhergestellt werden. Das Protokoll nennt die Gründe.",
  },
  filesCouldNotBeDeleted: {
    one: "{count} Datei konnte nicht gelöscht werden. Das Protokoll nennt den Grund.",
    other: "{count} Dateien konnten nicht gelöscht werden. Das Protokoll nennt die Gründe.",
  },
} satisfies TranslatedArea<"versions">;
