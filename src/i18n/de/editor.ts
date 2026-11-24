import type { TranslatedArea } from "../catalog";

export const editor = {
  // components/EditorPane
  lfLinuxMacos: "LF (Linux, macOS)",
  crlfWindows: "CRLF (Windows)",
  unsavedChanges: "Ungespeicherte Änderungen",
  findAndReplaceCtrlF: "Suchen und ersetzen (Ctrl+F)",
  saveCtrlS: "Speichern (Ctrl+S)",
  reloadTheirs: "Geänderte Datei laden",
  couldNotSave: "Speichern fehlgeschlagen: {saveError}",
  opening: "{name} wird geöffnet...",
  couldNotOpen: "{name} konnte nicht geöffnet werden",
  encodingUsedWhenSaving: "Codierung beim Speichern",
  lineEndingsUsedWhenSaving: "Zeilenenden beim Speichern",
  stopWrappingLongLines: "Lange Zeilen nicht mehr umbrechen",
  wrapLongLines: "Lange Zeilen umbrechen",
  reload: "{name} neu laden?",
  discardAndReload: "Verwerfen und neu laden",
  yourUnsavedChangesWillBe: "Ihre ungespeicherten Änderungen gehen verloren.",
  utf8WithBom: "UTF-8 mit BOM",
  latin1: "Latin-1",
  wasDeletedAfterYouOpened: "{name} wurde {where} gelöscht, nachdem Sie die Datei geöffnet haben.",
  changedSinceYouOpenedIt: "{name} wurde {where} geändert, seit Sie die Datei geöffnet haben.",
  saveItAgain: "Erneut speichern",
  saving: "Wird gespeichert...",
  saved: "Gespeichert",
  plainText: "Nur-Text",
  reloadFromTheServer: "Vom Server neu laden",
  reloadFromDisk: "Vom Datenträger neu laden",
  lineColumn: "Zeile {line}, Spalte {column}",
  lineColumnSelected: "Zeile {line}, Spalte {column} ({selected} ausgewählt)",
  anOlderVersionIsReadOnly: "Eine ältere Version ist schreibgeschützt",
  encodingOfThisOlderVersion: "Codierung dieser älteren Version",
  lineEndingsOfThisOlderVersion: "Zeilenenden dieser älteren Version",
  olderVersionReadOnly: "Ältere Version, schreibgeschützt",

  // components/UnsavedChangesDialog
  someFilesCouldNotBe:
    "Einige Dateien konnten nicht gespeichert werden. Ihre Tabs nennen den Grund.",
  changesYouDontSaveAre: "Nicht gespeicherte Änderungen gehen verloren.",
  dontSave: "Nicht speichern",
  saveAll: "Alle speichern",
  saveChangesTo: "Änderungen an {name} speichern?",
  saveChangesToFiles: {
    one: "Änderungen an {count} Datei speichern?",
    other: "Änderungen an {count} Dateien speichern?",
  },
} satisfies TranslatedArea<"editor">;
