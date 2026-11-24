import type { TranslatedArea } from "../catalog";

export const sync = {
  // components/SyncDialog
  chooseTheLocalFolder: "Lokalen Ordner auswählen",
  synchronizeFolders: "Ordner synchronisieren",
  localFolder: "Lokaler Ordner",
  theFolderOnThisComputer: "Der Ordner auf diesem Computer",
  server: "Server",
  noSftpServerConnected: "Kein SFTP-Server verbunden",
  serverFolder: "Ordner auf dem Server",
  direction: "Richtung",
  rsyncOptions: "rsync-Optionen",
  schedule: "Planen...",
  runThisSynchronizationAtSet: "Diese Synchronisierung zu festen Zeiten ausführen",
  fillInBothFoldersOf:
    "Füllen Sie beide Ordner einer gespeicherten Verbindung aus, um dies zu planen",
  comparingContentsOfFiles: {
    one: "Inhalte werden verglichen: {compared} von {count} Datei",
    other: "Inhalte werden verglichen: {compared} von {count} Dateien",
  },
  startingFromTheDefaultsIn:
    "Ausgehend von den Standardwerten in den Einstellungen, mit den für {siteName} gespeicherten rsync-Argumenten. Änderungen hier gelten nur für diese Synchronisierung.",
  startingFromTheDefaultsInSettings:
    "Ausgehend von den Standardwerten in den Einstellungen. Änderungen hier gelten nur für diese Synchronisierung.",
  compareDryRun: "Vergleichen (Probelauf)",
  compare: "Vergleichen",
  listingFoldersLocalAndServer:
    "Ordner werden gelistet: {local} lokale Einträge und {remote} auf dem Server",

  // components/SyncPlanList
  rsync: "rsync {arguments}",
  theOptionsAsRsyncArguments: "Die Optionen als rsync-Argumente",
  dryRunRunningThisChanges:
    "Probelauf: Die Ausführung ändert nichts und schreibt ins Protokoll, was geschähe.",
  unchangedSourceFilesWillBe: {
    one: "{count} unveränderte Quelldatei wird ebenfalls entfernt.",
    other: "{count} unveränderte Quelldateien werden ebenfalls entfernt.",
  },
  includeAll: "Alle einbeziehen",
  action: "Aktion",
  why: "Grund",
  entriesPassedOverBrokenOr: {
    one: "{count} Eintrag übergangen: defekte oder kreisförmige Verknüpfungen, Spezialdateien und Ordner, die nicht gelesen werden konnten",
    other:
      "{count} Einträge übergangen: defekte oder kreisförmige Verknüpfungen, Spezialdateien und Ordner, die nicht gelesen werden konnten",
  },
  include: "{path} einbeziehen",
  settle: "{path} klären",
  leaveAsIs: "So lassen",
  uploadLocalCopy: "Lokale Kopie hochladen",
  downloadServerCopy: "Kopie vom Server herunterladen",
  files: { one: "{count} Datei", other: "{count} Dateien" },
  all: "Alle",
  conflicts: "Konflikte",
  noFilesToCopy: "Keine Dateien zu kopieren.",
  everythingIsInSync: "Alles ist synchron.",
  starting: "Wird gestartet...",
  settleByHand: "Von Hand klären",
  dryRun: "Probelauf",
  synchronize: "Synchronisieren",
  localSide: "Lokal",
  serverSide: "Server",
  sideMissing: "{side}: fehlt",
  sideFolder: "{side}: Ordner",
  sideFolderWithPermissions: "{side}: Ordner, {permissions}",
  sideFile: "{side}: {size}, {modified}",
  sideFileWithPermissions: "{side}: {size}, {modified}, {permissions}",
  sideLink: "{side}: Verknüpfung, {size}, {modified}",
  sideLinkWithPermissions: "{side}: Verknüpfung, {size}, {modified}, {permissions}",

  // components/SyncSettings
  rsync2: "rsync",
  rsyncRunsOnTheServer:
    "rsync läuft über Ihre SSH-Verbindung auf dem Server, daher muss dort rsync installiert sein. rsync-Daemons (rsync://-Adressen auf Port 873) werden nicht unterstützt.",
  useRsync: "rsync verwenden",
  useRsyncHint:
    "Dateien, die auf der anderen Seite schon vorhanden sind, werden aktualisiert, indem nur die geänderten Teile gesendet werden. Ist dies aus oder hat der Server kein rsync, werden Dateien vollständig über SFTP kopiert.",
  forFilesLargerThan: "Für Dateien größer als",
  forFilesLargerThanHint: "Kleinere Dateien sind schneller vollständig kopiert.",
  rsyncOnTheServer: "rsync auf dem Server",
  rsyncOnTheServerHint: "Der Befehl, der es startet, für Server, bei denen es nicht im PATH liegt.",
  folderSynchronization: "Ordnersynchronisierung",
  eachSynchronizationStartsWithThese:
    "Jede Synchronisierung beginnt mit diesen Optionen. Die eigenen rsync-Argumente einer gespeicherten Verbindung kommen hinzu, und Ordner synchronisieren kann jede davon für einen Lauf ändern.",
  typedAsOnRsyncsCommand:
    "So geschrieben wie auf der Befehlszeile von rsync. Sie haben bei jeder Synchronisierung Vorrang vor den Optionen darüber.",

  // lib/sync
  folders: { one: "{count} Ordner", other: "{count} Ordner" },
  files2: { one: "{count} Datei, {size}", other: "{count} Dateien, {size}" },
  deleteItems: { one: "{count} Element löschen", other: "{count} Elemente löschen" },
  deletionsHeldBackByThe: {
    one: "{count} Löschung durch die Begrenzung zurückgehalten",
    other: "{count} Löschungen durch die Begrenzung zurückgehalten",
  },
  emptyFoldersLeftOut: {
    one: "{count} leerer Ordner ausgelassen",
    other: "{count} leere Ordner ausgelassen",
  },
  wouldCopyFiles: {
    one: "würde {count} Datei kopieren ({size})",
    other: "würde {count} Dateien kopieren ({size})",
  },
  deleteItems2: { one: "{count} Element löschen", other: "{count} Elemente löschen" },
  createFolders: { one: "{count} Ordner anlegen", other: "{count} Ordner anlegen" },
  queuedFiles: {
    one: "{count} Datei eingereiht ({size})",
    other: "{count} Dateien eingereiht ({size})",
  },
  deletedItems: { one: "{count} Element gelöscht", other: "{count} Elemente gelöscht" },
  itemsToDeleteOnceCopying: {
    one: "{count} Element zum Löschen nach dem Kopieren",
    other: "{count} Elemente zum Löschen nach dem Kopieren",
  },
  createdFolders: { one: "{count} Ordner angelegt", other: "{count} Ordner angelegt" },
  createdLinks: { one: "{count} Verknüpfung angelegt", other: "{count} Verknüpfungen angelegt" },
  updatedFiles: { one: "{count} Datei aktualisiert", other: "{count} Dateien aktualisiert" },
  removedUnchangedSourceFiles: {
    one: "{count} unveränderte Quelldatei entfernt",
    other: "{count} unveränderte Quelldateien entfernt",
  },
  localToServer: "Lokal zum Server",
  makesTheServerFolderMatch: "Gleicht den Ordner auf dem Server an den lokalen an.",
  serverToLocal: "Server zum lokalen Ordner",
  makesTheLocalFolderMatch: "Gleicht den lokalen Ordner an den auf dem Server an.",
  bothWays: "In beide Richtungen",
  copiesNewFilesEachWay:
    "Kopiert neue Dateien in jede Richtung. Wo beide Seiten eine Datei haben, gewinnt die neuere.",
  deleteLocal: "Lokal löschen",
  deleteOnServer: "Auf dem Server löschen",
  conflict: "Konflikt",
  plannedUpload: "{total} hochladen",
  plannedDownload: "{total} herunterladen",
  conflictsLeft: {
    one: "{count} Konflikt bleibt, wie er ist",
    other: "{count} Konflikte bleiben, wie sie sind",
  },
  plannedSentences: "{parts}.",
  plannedSeparator: ". ",
  nothingSelected: "Nichts ausgewählt.",
  onlyInTheLocalFolder: "Nur im lokalen Ordner",
  onlyOnTheServer: "Nur auf dem Server",
  sizeOrTimeDiffers: "Größe oder Zeit unterscheiden sich",
  contentsDiffer: "Inhalt unterscheidet sich",
  copiedWithoutComparing: "Ohne Vergleich kopiert",
  newerInTheLocalFolder: "Neuer im lokalen Ordner",
  newerOnTheServer: "Neuer auf dem Server",
  notInTheLocalFolder: "Nicht im lokalen Ordner",
  notOnTheServer: "Nicht auf dem Server",
  typeDiffers: "{here} hier, {there} auf dem Server",
  differsWithTheSameTimeOnBothSides: "Unterschiedlich, bei gleicher Zeit auf beiden Seiten",
  shorterOnTheTarget: "Am Ziel kürzer, nur der Rest wird gesendet",
  sameContentsOnlyTheTimeOrPermissions:
    "Gleicher Inhalt, nur Zeit oder Berechtigungen werden gesetzt",
  nothingHere: "Nichts",
  folderHere: "Ordner",
  linkHere: "Verknüpfung",
  fileHere: "Datei",
  nothingThere: "nichts",
  folderThere: "Ordner",
  linkThere: "Verknüpfung",
  fileThere: "Datei",
  countUnchanged: { one: "{count} unverändert", other: "{count} unverändert" },
  countKeptAsTheyAre: { one: "{count} belassen", other: "{count} belassen" },
  countOnlyInTheLocalFolderNotDeleted: {
    one: "{count} nur im lokalen Ordner, nicht gelöscht",
    other: "{count} nur im lokalen Ordner, nicht gelöscht",
  },
  countOnlyOnTheServerNotDeleted: {
    one: "{count} nur auf dem Server, nicht gelöscht",
    other: "{count} nur auf dem Server, nicht gelöscht",
  },
  countProtectedFromDeletion: {
    one: "{count} vor dem Löschen geschützt",
    other: "{count} vor dem Löschen geschützt",
  },
  countNewNotCreated: { one: "{count} neu, nicht angelegt", other: "{count} neu, nicht angelegt" },
  countOutsideTheSizeLimits: {
    one: "{count} außerhalb der Größengrenzen",
    other: "{count} außerhalb der Größengrenzen",
  },
  countExcluded: { one: "{count} ausgeschlossen", other: "{count} ausgeschlossen" },
  countPassedOver: { one: "{count} übergangen", other: "{count} übergangen" },
  dryRunSummary:
    "Probelauf: {parts}. Nichts wurde geändert, das Protokoll enthält die Einzelheiten.",
  runSummary: "{done}.",
  runSummaryWithFailures: {
    one: "{done}. {count} Änderung ist fehlgeschlagen, das Protokoll enthält die Einzelheiten.",
    other:
      "{done}. {count} Änderungen sind fehlgeschlagen, das Protokoll enthält die Einzelheiten.",
  },
} satisfies TranslatedArea<"sync">;
