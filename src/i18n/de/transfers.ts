import type { TranslatedArea } from "../catalog";

export const transfers = {
  // components/TransferLookSettings
  timeLeft: "noch {duration}",

  // components/TransferView
  copySourcePaths: "Quellpfade kopieren",
  copySourcePath: "Quellpfad kopieren",
  moveToTop: "Ganz nach oben",
  moveToBottom: "Ganz nach unten",
  progress: "Fortschritt",
  speed: "Tempo",
  status: "Status",
  dragFilesBetweenTabsOr:
    "Ziehen Sie Dateien zwischen Tabs oder verwenden Sie Hochladen und Herunterladen im Menü einer Datei.",
  transfersThatFailAfterTheir:
    "Übertragungen, die nach ihren Wiederholungen scheitern, landen hier.",
  finishedTransfersAreListedHere: "Fertige Übertragungen erscheinen hier.",
  done: "Fertig",
  scheduleThese: "Als Aufgabe planen...",
  scheduleThis: "Als Aufgabe planen...",
  connectionLostWaitingForTheServer: "Verbindung unterbrochen, wartet auf den Server",
  waitingToRetry: "Wartet auf Wiederholung (Versuch {attempt})",
  queued: "Wartet",
  listingFolder: "Ordner wird aufgelistet",
  updatingWithRsyncSent: "Wird mit rsync aktualisiert, {size} gesendet",
  copyingDirectlyBetweenTheServers: "Wird direkt zwischen den Servern kopiert",
  transferringOnConnections: {
    one: "Überträgt über {count} Verbindung",
    other: "Überträgt über {count} Verbindungen",
  },
  transferring: "Überträgt",
  paused: "Angehalten",
  alreadyExistsWaitingForYourChoice: "Schon vorhanden, wartet auf Ihre Wahl",
  doneCopiedDirectlyBetweenTheServers: "Fertig, direkt zwischen den Servern kopiert",
  doneWithRsyncSent: "Fertig mit rsync, {sent} von {size} gesendet",
  skippedBecause: "Übersprungen: {error}",
  skipped: "Übersprungen",
  failedBecause: "Fehlgeschlagen: {error}",
  failedUnknownError: "Fehlgeschlagen: unbekannter Fehler",
  rowTitle: "{source}\nnach {target}",
  rowTitleQueuedByTask: "{source}\nnach {target}\nEingereiht von der Aufgabe {task}",

  // lib/progressBars
  sizeOfSize: "{transferred} von {size}",

  // state/transferActions
  nothingToTransferOnlyRegular:
    "Nichts zu übertragen: Nur reguläre Dateien und Ordner können kopiert werden.",
  unlockTheVaultToCopy: "Entsperren Sie den Tresor, um Dateien hineinzukopieren.",
  couldNotCheckWhetherThis: "Es konnte nicht geprüft werden, ob dieser Ordner ein Tresor ist.",
  isALockedVaultOpen:
    "{baseName} ist ein gesperrter Tresor. Öffnen Sie ihn, um Dateien hineinzukopieren.",
  dropLocalFilesOnA: "Legen Sie lokale Dateien auf einem Server-Tab ab, um sie hochzuladen.",
  copyingTo: "Wird nach {folder} kopiert",
  movedTo: "Nach {folder} verschoben",
  copiedTo: "Nach {folder} kopiert",
  connectToAServerTo: "Verbinden Sie sich mit einem Server, um darauf hochzuladen.",
  openALocalTabTo: "Öffnen Sie einen lokalen Tab, um dorthin herunterzuladen.",
  dropFilesOnAServer: "Legen Sie Dateien auf einem Server-Tab ab, um sie hochzuladen.",
  local: "Lokal",
} satisfies TranslatedArea<"transfers">;
