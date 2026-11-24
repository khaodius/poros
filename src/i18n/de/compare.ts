import type { TranslatedArea } from "../catalog";

export const compare = {
  // components/CompareDialog
  compareFiles: "Dateien vergleichen",
  swapSides: "Seiten tauschen",
  onlyChanges: "Nur Änderungen",
  ignoreWhitespace: "Leerraum ignorieren",
  ignoreCase: "Groß-/Kleinschreibung ignorieren",
  onTheLeftOnThe: "{left} links, {right} rechts.",
  readingBothFiles: "Beide Dateien werden gelesen...",
  previousChangeAltUp: "Vorherige Änderung (Alt+Up)",
  previousChange: "Vorherige Änderung",
  nextChangeAltDown: "Nächste Änderung (Alt+Down)",
  nextChange: "Nächste Änderung",
  changes: { one: "{count} Änderung", other: "{count} Änderungen" },
  showUnchangedLines: {
    one: "{count} unveränderte Zeile zeigen",
    other: "{count} unveränderte Zeilen zeigen",
  },
  comparing: "Wird verglichen...",
  noChanges: "Keine Änderungen",
  changeOf: "Änderung {current} von {count}",

  // components/FolderCompareBar
  synchronizeTheseFolders: "Diese Ordner synchronisieren",
  synchronizeTheseFolders2: "Diese Ordner synchronisieren ({unavailable})",
  folderComparison: "Ordnervergleich",
  comparedWith: "Verglichen mit {where}",
  comparedWithAt: "Verglichen mit {where}: {path}",
  again: "Erneut: {action}",
  selectByState: "Nach Zustand auswählen",
  comparisonOptions: "Vergleichsoptionen",
  stopComparing: "Vergleich beenden ({shortcut})",
  rowsByHowTheyCompare: "Zeilen nach Vergleichsergebnis",
  comparingSubfoldersItemsListedSo: {
    one: "Unterordner werden verglichen: bisher {count} Element gelistet",
    other: "Unterordner werden verglichen: bisher {count} Elemente gelistet",
  },
  couldNotCompareSubfolders: "Unterordner konnten nicht verglichen werden: {message}",
  thereAreTooManySubfolders:
    "Es gibt zu viele Unterordner, um alle zu markieren. Die tiefsten bleiben unmarkiert.",
  thePanesAreNoLonger: "{note}. Die Bereiche folgen einander nicht mehr.",
  inAnotherWindow: "{label} in einem anderen Fenster",
  nothingToCompareYet: "noch nichts zu vergleichen",
  stateCount: "{state}: {amount}",
  stateCountHidden: "{state}: {amount}, ausgeblendet",

  // components/folderCompareMenu
  selectNewerAndOnlyHere: "Neuere und nur hier vorhandene auswählen",
  selectDifferent: "Verschiedene auswählen",
  hideIdenticalItems: "Identische Elemente ausblenden",
  compareBySizeOnly: "Nur nach Größe vergleichen",
  ignoreTimeZoneOffsetsWhole: "Zeitzonenversatz ignorieren (volle Stunden)",
  includeHiddenFiles: "Versteckte Dateien einbeziehen",
  browseBothPanesTogether: "In beiden Bereichen gemeinsam navigieren",
  compareFolders: "Ordner vergleichen",
  compareContentsOfSubfolders: "Inhalt der Unterordner vergleichen",

  // hooks/useFolderCompare
  syncOnlyLocalAndSftp: "nur zwischen diesem Computer und einem SFTP-Server",
  syncFromServerWindow: "aus dem Fenster mit dem Tab des Servers",

  // lib/compare
  lfUnix: "LF (Unix)",
  crlfWindows: "CRLF (Windows)",
  lineEndingNone: "keine",
  lineEndingMixed: "gemischt",
  lineEndingsDiffer: "Die Zeilenenden unterscheiden sich: {left} links, {right} rechts.",
  filesIdentical: "Die Dateien sind identisch.",
  filesDifferTooLarge:
    "Die Dateien unterscheiden sich. Sie sind zu groß, um sie nebeneinander zu zeigen.",
  filesDifferNotText:
    "Die Dateien unterscheiden sich. Sie sind kein Text, daher wurden nur ihre Bytes verglichen.",
  linesChanged: { one: "{count} geänderte Zeile", other: "{count} geänderte Zeilen" },
  linesAdded: { one: "{count} hinzugefügte Zeile", other: "{count} hinzugefügte Zeilen" },
  linesRemoved: { one: "{count} entfernte Zeile", other: "{count} entfernte Zeilen" },
  lineCounts: "{counts}.",
  onlyLineEndingsDiffer: "Nur die Zeilenenden unterscheiden sich.",
  filesDifferInIgnored: "Die Dateien unterscheiden sich nur in dem, was ignoriert wird.",
  filesDifferInBytes: "Die Zeilen sind gleich, die Dateien unterscheiden sich in ihren Bytes.",

  // lib/folderCompare
  differsInside: "Unterschiede darin: {found}",
  noDifferencesFoundInsideBut:
    "Keine Unterschiede darin gefunden, aber nicht alles verglichen: {found}",
  identicalInsideFiles: {
    one: "Darin identisch: {count} Datei",
    other: "Darin identisch: {count} Dateien",
  },
  identicalInsideNoFilesOn: "Darin identisch: auf keiner Seite Dateien",
  newerHere: "Hier neuer",
  olderHere: "Hier älter",
  onlyHere: "Nur hier",
  different: "Verschieden",
  identical: "Identisch",
  differentAndNeitherIsNewer: "Verschieden, aber keine Seite ist neuer",
  onBothSidesTheContents: "Auf beiden Seiten. Der Inhalt wird nicht verglichen.",
  neitherListingRefreshed:
    "Die Liste keines der beiden Bereiche konnte aktualisiert werden. Die Markierungen sind daher womöglich veraltet, bis beide neu gelistet sind",
  thisListingNotRefreshed:
    "Die Liste dieses Bereichs konnte nicht aktualisiert werden. Die Markierungen sind daher womöglich veraltet, bis er neu gelistet ist",
  partnerListingNotRefreshed:
    "Die Liste in {partner} konnte nicht aktualisiert werden. Die Markierungen sind daher womöglich veraltet, bis sie neu gelistet ist",
  pausedBothShowResults:
    "Der Vergleich pausiert, solange beide Bereiche Suchergebnisse zeigen. Die Markierungen kehren zurück, wenn die Ergebnisse geschlossen werden",
  pausedThisShowsResults:
    "Der Vergleich pausiert, solange dieser Bereich Suchergebnisse zeigt. Die Markierungen kehren zurück, wenn die Ergebnisse geschlossen werden",
  pausedPartnerShowsResults:
    "Der Vergleich pausiert, solange {partner} Suchergebnisse zeigt. Die Markierungen kehren zurück, wenn die Ergebnisse geschlossen werden",
  summaryNewerHere: "{count} hier neuer",
  summaryOlderHere: "{count} hier älter",
  summaryOnlyHere: "{count} nur hier",
  summaryOnlyThere: "{count} nur im anderen Bereich",
  summaryDifferent: "{count} verschieden",
  summaryIdentical: "{count} identisch",
  summaryNotCompared: "{count} nicht verglichen",

  // state/folderCompare
  noFolderToCompare: "Dieser Bereich hat keinen Ordner zum Vergleichen",
  openASecondPane: "Öffnen Sie einen zweiten Ordnerbereich, mit dem dieser verglichen wird",
  paneShowsResults: "{pane} zeigt Suchergebnisse",
  noFolderNamed: "Kein Ordner namens {name} in {pane}",
  noFolderAbove: "{pane} hat keinen Ordner über diesem",
  paneCouldNotOpen: "{pane} konnte {path} nicht öffnen",
} satisfies TranslatedArea<"compare">;
