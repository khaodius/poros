import type { TranslatedArea } from "../catalog";

export const schedule = {
  // components/SchedulerDialog
  scheduledTasks: "Geplante Aufgaben",
  tasks: "Aufgaben",
  newTask: "Neue Aufgabe",
  shownInTheListOf: "Erscheint in der Liste der Aufgaben",
  turnedOn: "Eingeschaltet",
  task: "Aufgabe",
  savedConnection: "Gespeicherte Verbindung",
  ifPorosIsClosedAt: "Wenn Poros zu der Zeit geschlossen ist, beim nächsten Start ausführen",
  nextRun: "Nächste Ausführung",
  runningNow: "Läuft gerade",
  lastRun: "Letzte Ausführung",
  chooseTheLocalFolder: "Lokalen Ordner auswählen",
  localFolder: "Lokaler Ordner",
  theFolderOnThisComputer: "Der Ordner auf diesem Computer",
  serverFolder: "Ordner auf dem Server",
  direction: "Richtung",
  conflictsAreLeftAloneAnd: "{hint} Konflikte bleiben unberührt und werden gemeldet.",
  rsyncOptions: "rsync-Optionen",
  typedAsOnRsyncsCommand:
    "So geschrieben wie auf der Befehlszeile von rsync. Sie haben bei jeder Ausführung der Aufgabe Vorrang vor den Optionen darüber.",
  command: "Befehl",
  inFolderOptional: "Im Ordner (optional)",
  theLoginFolder: "Der Ordner nach der Anmeldung",
  when: "Wann",
  dateAndTime: "Datum und Uhrzeit",
  every: "alle",
  repeatEvery: "Wiederholen alle",
  unit: "Einheit",
  from: "ab",
  starting: "Beginn",
  at: "um",
  timeOfDay: "Uhrzeit",
  daysOfTheWeek: "Wochentage",
  onDays: "An Tagen",
  repeat: "Wiederholen",
  once: "Einmal",
  synchronizeFolders: "Ordner synchronisieren",
  transferFiles: "Dateien übertragen",
  runACommand: "Befehl ausführen",
  uploadTo: "Hochladen auf",
  downloadFrom: "Herunterladen von",
  copyTo: "Kopieren nach",
  tasksRunWhilePorosIsOpen:
    "Aufgaben laufen, solange Poros geöffnet ist, oder über poros-cli mit cron oder einem Timer. Gespeicherte Aufgaben erscheinen hier.",
  turnedOff: "Ausgeschaltet",
  chooseAConnection: "Verbindung auswählen",
  noSavedConnections: "Keine gespeicherten Verbindungen",
  noSavedSftpConnections: "Keine gespeicherten SFTP-Verbindungen",
  aDeletedConnection: "Eine gelöschte Verbindung",
  usesSynchronizingAndRunningCommands:
    "{name} verwendet {label}. Synchronisieren und Befehle ausführen brauchen eine SFTP-Verbindung.",
  thisTasksSavedConnectionWas:
    "Die gespeicherte Verbindung dieser Aufgabe wurde gelöscht. Wählen Sie eine andere.",
  deleteForGood: "Endgültig löschen",
  starting2: "Wird gestartet...",
  runNow: "Jetzt ausführen",
  saving: "Wird gespeichert...",
  thisConnectionsPasswordIsNotSaved:
    "Das Passwort dieser Verbindung ist nicht im Schlüsselbund des Systems gespeichert, daher kann sich die Aufgabe nicht anmelden.",
  thePasswordOfIsNot:
    "Das Passwort von {names} ist nicht im Schlüsselbund des Systems gespeichert, daher kann sich die Aufgabe nicht anmelden.",
  thePasswordsOfAreNot:
    "Die Passwörter von {names} sind nicht im Schlüsselbund des Systems gespeichert, daher kann sich die Aufgabe nicht anmelden.",
  saveItInTheConnections: "Speichern Sie es in den Einstellungen der Verbindung.",
  saveThemInTheConnections: "Speichern Sie sie in den Einstellungen der Verbindungen.",
  noneTheTimeHasPassed: "Keine, der Zeitpunkt ist vorbei",

  // components/TransferTaskFields
  aFullPathOnThe: "Ein vollständiger Pfad auf dem Server. Beenden Sie einen Ordner mit /",
  aRunNowGoesInto: "Eine Ausführung jetzt landet in {folder}.",
  addFiles: "Dateien hinzufügen",
  addFolders: "Ordner hinzufügen",
  addTheFilesAndFolders:
    "Fügen Sie die Dateien und Ordner hinzu, die die Aufgabe hochlädt. Ein Ordner kommt mit seinem gesamten Inhalt.",
  addTheServerPathsThe:
    "Fügen Sie die Serverpfade hinzu, die die Aufgabe kopiert, etwa /var/www/site. Ein Ordner kommt mit seinem gesamten Inhalt.",
  betweenServers: "Zwischen Servern",
  chooseTheFolderToDownload: "Ordner zum Herunterladen auswählen",
  copiesFilesAndFoldersFrom: "Kopiert Dateien und Ordner von diesem Computer auf den Server.",
  copiesFilesAndFoldersFromOne:
    "Kopiert Dateien und Ordner von einer gespeicherten Verbindung zu einer anderen.",
  copiesFilesAndFoldersFromThe: "Kopiert Dateien und Ordner vom Server auf diesen Computer.",
  copyFrom: "Kopieren von",
  filesAndFoldersOnThe: "Dateien und Ordner auf dem Server",
  filesAndFoldersOnThis: "Dateien und Ordner auf diesem Computer",
  inAFolderOfIts: "In einem eigenen Ordner (optional)",
  subfolderExample: "sicherung-{date}",
  intoTheFolderOnThe: "In den Ordner auf dem Server",
  intoTheFolderOnThis: "In den Ordner auf diesem Computer",
  itemsToCopy: "Zu kopierende Elemente",
  remove: "{path} entfernen",
  removeFromTheTask: "Aus der Aufgabe entfernen",
  serverPathToAdd: "Hinzuzufügender Serverpfad",
  theFolderToDownloadInto: "Der Ordner, in den heruntergeladen wird",
  whenAFileIsAlready: "Wenn eine Datei schon vorhanden ist",
  eachRunCanGetAFolder:
    "Jede Ausführung kann einen nach ihrer Zeit benannten Ordner erhalten: {placeholders}.",

  // lib/schedule
  overwriteIfTheSourceIs: "Überschreiben, wenn die Quelle neuer ist",
  overwriteIfSizeOrDate: "Überschreiben, wenn Größe oder Datum abweichen",
  keepBoth: "Beide behalten",
  everyDay: "Jeden Tag",
  weekdays: "Werktags",
  weekends: "Am Wochenende",
  onceAt: "Einmal, {date}",
  everyMinutes: { one: "Jede Minute", other: "Alle {count} Minuten" },
  everyHours: { one: "Jede Stunde", other: "Alle {count} Stunden" },
  everyDays: { one: "Jeden Tag", other: "Alle {count} Tage" },
  daysAt: "{days} um {time}",
  unitMinutes: "Minuten",
  unitHours: "Stunden",
  unitDays: "Tage",
  nameRun: "{program} ausführen",
  nameUploadOne: "{name} hochladen",
  nameDownloadOne: "{name} herunterladen",
  nameCopyOne: "{name} kopieren",
  nameUploadItems: { one: "{count} Element hochladen", other: "{count} Elemente hochladen" },
  nameDownloadItems: {
    one: "{count} Element herunterladen",
    other: "{count} Elemente herunterladen",
  },
  nameCopyItems: { one: "{count} Element kopieren", other: "{count} Elemente kopieren" },
  nameSync: "{name} synchronisieren",
  patternTooLong: {
    one: "Das Ordnermuster ist länger als {count} Zeichen",
    other: "Das Ordnermuster ist länger als {count} Zeichen",
  },
  patternLoneBrace:
    "Im Ordnermuster steht eine Klammer ohne Gegenstück. Schreiben Sie Platzhalter als {example}",
  patternUnknown:
    "Das Ordnermuster enthält {found}, das kein Platzhalter ist. Verwenden Sie {known}",
  patternBadName: "Das Ordnermuster ergibt „{name}“, was kein Ordnername sein kann",

  // lib/scheduleDraft
  porosNoLongerKnowsThe:
    "Poros kennt den Server dieser Übertragungen nicht mehr. Planen Sie die Dateien stattdessen aus einem Tab.",
  filesInAnUnlockedVault:
    "Dateien in einem entsperrten Tresor können nicht geplant werden: Eine Aufgabe könnte den Tresor nicht entsperren.",
  saveTheConnectionToFirst:
    "Speichern Sie zuerst die Verbindung zu {label}: Eine geplante Aufgabe meldet sich über eine gespeicherte Verbindung an.",
  selectTheTransfersToSchedule: "Wählen Sie die zu planenden Übertragungen aus.",
  selectTransfersThatGoThe:
    "Wählen Sie Übertragungen aus, die zwischen denselben Orten in dieselbe Richtung gehen, um sie zu planen.",
  theSelectedTransfersGoTo:
    "Die ausgewählten Übertragungen gehen in verschiedene Ordner. Wählen Sie solche aus, die in einen Ordner gehen.",
  selectTheFilesToSchedule: "Wählen Sie die zu planenden Dateien aus.",
  blockedNoServer: "kein Server",
  blockedInVault: "nicht in einem Tresor",
  blockedUnsaved: "zuerst die Verbindung speichern",

  // state/scheduleActions
  blockedNoTab: "kein Tab",
} satisfies TranslatedArea<"schedule">;
