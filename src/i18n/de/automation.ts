import type { TranslatedArea } from "../catalog";

export const automation = {
  // components/AutomationSettings
  whenTheTransferQueueFinishes: "Wenn die Warteschlange fertig ist",
  then: "Danach",
  thenHint:
    "Auch in der Werkzeugleiste der Warteschlange. Alles außer einem Befehl zählt zuerst 30 Sekunden herunter, damit Sie es abbrechen können.",
  onlyTheNextTime: "Nur beim nächsten Mal",
  onlyTheNextTimeHint: "Springt zurück auf Nichts tun, sobald es geschehen ist.",
  command: "Befehl",
  commandHint:
    "Für Befehl ausführen. Läuft auf diesem Computer. {done}, {failed}, {skipped} und {bytes} enthalten die Summen.",
  aCommandOrScript: "Ein Befehl oder Skript",
  showANotification: "Benachrichtigung anzeigen",
  onlyWhenPorosIsIn: "Nur wenn Poros im Hintergrund ist",
  playASound: "Ton abspielen",
  serverCommands: "Serverbefehle",
  listedUnderCommandsWhenYou:
    "Erscheinen unter Befehle, wenn Sie in einem Serverbereich mit der rechten Maustaste klicken. Sie laufen über die Shell des Servers im angezeigten Ordner, mit diesen Ersetzungen:",
  scheduledTasks: "Geplante Aufgaben",
  synchronizeFoldersOrRunCommands: "Ordner synchronisieren oder Befehle zu festen Zeiten ausführen",
  synchronizeFoldersOrRunCommandsHint:
    "Aufgaben laufen, solange Poros geöffnet ist, im Hintergrund und über gespeicherte Verbindungen.",
  output: "Ausgabe",
  openAWindowWithWhat:
    "Fenster mit der Ausgabe des Befehls öffnen. Aus: Der Befehl läuft ohne Fenster",
  reloadTheServersFoldersWhen: "Ordner des Servers neu laden, wenn der Befehl fertig ist",
  deleteCommand: "Befehl löschen",
  nameOfTheNewCommand: "Name des neuen Befehls",
  newCommand: "Neuer Befehl",

  // components/CommandOutputDialog
  earlierOutputWasDropped: "Frühere Ausgabe wurde verworfen.",
  theCommandPrintedNothing: "Der Befehl hat nichts ausgegeben.",
  running: "Läuft",
  commandIn: "{command} in {folder}",
  commandsIn: { one: "{count} Befehl in {folder}", other: "{count} Befehle in {folder}" },
  theLoginFolder: "dem Ordner nach der Anmeldung",

  // components/RunCommandDialog
  runACommand: "Befehl ausführen",
  runACommandOnThe: "Befehl auf dem Server ausführen",
  commandToRunIn: "Befehl, der in {folder} ausgeführt wird",
  refreshTheServersFoldersWhen: "Ordner des Servers aktualisieren, wenn er fertig ist",
  selectFilesOrFoldersTo: "Wählen Sie Dateien oder Ordner aus, um die Platzhalter zu füllen.",
  run: "Ausführen",

  // components/WhenDoneControl
  whatHappensWhenTheQueue: "Was geschieht, wenn die Warteschlange fertig ist",
  whenDone: "Danach",
  whenTheQueueFinishes: "Wenn die Warteschlange fertig ist",

  // components/WhenDoneCountdown
  seconds: { one: "{count} Sekunde", other: "{count} Sekunden" },

  // components/commandMenu
  commands: "Befehle",
  runACommand2: "Befehl ausführen...",
  editCommands: "Befehle bearbeiten...",

  // lib/automation
  queueFinishedClosingPorosIn: "Die Warteschlange ist fertig. Poros wird in {seconds} geschlossen.",
  queueFinishedLockingIn: "Die Warteschlange ist fertig. Der Computer wird in {seconds} gesperrt.",
  queueFinishedSleepingIn:
    "Die Warteschlange ist fertig. Der Computer wechselt in {seconds} in den Energiesparmodus.",
  queueFinishedHibernatingIn:
    "Die Warteschlange ist fertig. Der Computer wechselt in {seconds} in den Ruhezustand.",
  queueFinishedLoggingOffIn: "Die Warteschlange ist fertig. Sie werden in {seconds} abgemeldet.",
  queueFinishedShuttingDownIn:
    "Die Warteschlange ist fertig. Der Computer wird in {seconds} heruntergefahren.",
  filesTransferredIn: {
    one: "{count} Datei ({size}) in {duration} übertragen",
    other: "{count} Dateien ({size}) in {duration} übertragen",
  },
  doNothing: "Nichts tun",
  closePoros: "Poros schließen",
  lockTheComputer: "Computer sperren",
  sleep: "Energie sparen",
  hibernate: "Ruhezustand",
  logOff: "Abmelden",
  shutDown: "Herunterfahren",
  closingPoros: "Poros wird geschlossen",
  lockingTheComputer: "Computer wird gesperrt",
  puttingTheComputerToSleep: "Computer wechselt in den Energiesparmodus",
  hibernatingTheComputer: "Computer wechselt in den Ruhezustand",
  loggingOff: "Benutzer wird abgemeldet",
  shuttingDownTheComputer: "Computer wird heruntergefahren",
  confirmCloseNow: "Jetzt schließen",
  confirmLockNow: "Jetzt sperren",
  confirmSleepNow: "Jetzt Energie sparen",
  confirmHibernateNow: "Jetzt Ruhezustand",
  confirmLogOffNow: "Jetzt abmelden",
  confirmShutDownNow: "Jetzt herunterfahren",
  transfersFinishedWithErrors: "Übertragungen mit Fehlern beendet",
  transfersFinished: "Übertragungen beendet",
  failedCount: { one: "{count} fehlgeschlagen", other: "{count} fehlgeschlagen" },
  skippedCount: { one: "{count} übersprungen", other: "{count} übersprungen" },

  // lib/commands
  eachSelectedItemsFullPath:
    "der vollständige Pfad jedes ausgewählten Elements, einmal je Element ausgeführt",
  eachSelectedItemsNameRunning:
    "der Name jedes ausgewählten Elements, einmal je Element ausgeführt",
  theFullPathsOfAll: "die vollständigen Pfade aller ausgewählten Elemente",
  theNamesOfAllSelected: "die Namen aller ausgewählten Elemente",
  theFolderShown: "der angezeigte Ordner",
  keepPlaceholdersOutOfAnotherShell:
    "Setzen Sie keine Platzhalter in eine Befehlszeichenkette, die eine weitere Shell liest, etwa {bashExample} oder {sshExample}: Diese Shell würde einen Namen als Teil ihres Befehls lesen.",
  cannotTellHowTheShellReads:
    "Poros kann nicht sicher erkennen, wie die Shell diesen Befehl liest: Er enthält ein Here-Document, {ansiQuoted} oder ein offenes Anführungszeichen oder eine offene Klammer. Jeder Platzhalter wird als {parameter} usw. eingesetzt. So bleibt ein Name nur außerhalb von Anführungszeichen ein einzelnes Wort.",
  commandWithParameters: "{command} mit {parameters}",
  stoppedAfter: "Nach {elapsed} gestoppt",
  endedBySignal: "Nach {elapsed} durch Signal {signal} beendet",
  endedWithoutStatus: "Nach {elapsed} ohne Exit-Status beendet",
  exitedWithStatus: "Nach {elapsed} mit Status {status} beendet",
  finishedIn: "Nach {elapsed} fertig",

  // state/commandStore
  stopped: "Gestoppt",
  nothingToRun: "Nichts auszuführen",
  allCommandsFinished: { one: "{count} Befehl fertig", other: "Alle {count} Befehle fertig" },
  ofCommandsFailed: {
    one: "{count} von {finished} Befehlen fehlgeschlagen",
    other: "{count} von {finished} Befehlen fehlgeschlagen",
  },

  // state/whenDone
  whenTheQueueFinished: "Nach dem Ende der Warteschlange",
  theCommandAfterTheQueue: "Der Befehl nach dem Ende der Warteschlange",
} satisfies TranslatedArea<"automation">;
