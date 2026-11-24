import type { TranslatedArea } from "../catalog";

export const openWith = {
  // components/ExternalEditDialogs
  isGoneFromTheServer: "{name} ist nicht mehr auf dem Server",
  changedOnTheServer: "{name} wurde auf dem Server geändert",
  modified: "Geändert",
  onTheServer: "Auf dem Server",
  yourSavedCopy: "Ihre gespeicherte Kopie",
  keepBothUploadsYourCopy:
    "Beide behalten lädt Ihre Kopie neben die des Servers hoch, unter einem Namen, der dort frei ist.",
  comparingReadsTheServersCopy: "Zum Vergleichen wird die Kopie des Servers über SFTP gelesen",
  dontUpload: "Nicht hochladen",
  uploadItAgain: "Erneut hochladen",
  keepBoth: "Beide behalten",
  upload: "{name} hochladen?",
  uploadEverySaveWithoutAsking: "Jedes Speichern ohne Nachfrage hochladen",
  notNow: "Nicht jetzt",
  defaultProgram: "Standardprogramm",
  setUpForFilesLike: "Für Dateien wie diese eingerichtet",
  builtInEditor: "Eingebauter Editor",
  editTheTextInA: "Den Text hier in einem Tab bearbeiten",
  chooseAProgram: "Programm auswählen...",
  anyProgramOnThisComputer: "Ein beliebiges Programm auf diesem Computer",
  isAProgram: "{name} ist ein Programm",
  open: "{name} öffnen",
  programs: "Programme",
  filesNotUploadedLastTime: "Beim letzten Mal nicht hochgeladene Dateien",
  later: "Später",
  lock: "{label} sperren?",
  close: "{label} schließen?",
  theirSavesKeepBeingUploaded:
    "Was darin gespeichert wird, wird auch nach dem Schließen des Tabs hochgeladen.",
  stopEditingAndClose: "Bearbeitung beenden und schließen",
  uploadSavedFilesBeforeQuitting: "Gespeicherte Dateien vor dem Beenden hochladen?",
  someCouldNotBeUploaded: "Einige konnten nicht hochgeladen werden.",
  quitWithoutUploading: "Ohne Hochladen beenden",
  deleted: "Gelöscht",
  compare: "Vergleichen",
  compareSftpOnly: "Vergleichen (nur SFTP)",
  lockAnyway: "Trotzdem sperren",
  closeAnyway: "Trotzdem schließen",
  uploading2: "Wird hochgeladen...",
  uploadAndLock: "Hochladen und sperren",
  uploadAndClose: "Hochladen und schließen",
  keepEditing: "Weiter bearbeiten",
  uploadAndQuit: "Hochladen und beenden",
  uploadedAs: "{name} als {renamedTo} hochgeladen",
  uploadedName: "{name} hochgeladen",
  couldNotUploadKept:
    "{name} konnte nicht hochgeladen werden: {message}. Die gespeicherte Datei bleibt erhalten. Versuchen Sie es über die Statusleiste erneut.",
  couldNotOpen: "{name} konnte nicht geöffnet werden: {message}",
  someoneDeletedIt:
    "Jemand hat die Datei auf {server} gelöscht, seit Sie sie geöffnet haben. Ihre gespeicherte Fassung wurde noch nicht hochgeladen.",
  someoneChangedIt:
    "Jemand hat die Datei auf {server} geändert, seit Sie sie geöffnet haben. Ihre gespeicherte Fassung wurde noch nicht hochgeladen.",
  serverChangedThere: "{server}, dort geändert",
  savedInProgramUploadNow: "{name} wurde in {program} gespeichert. Jetzt auf {server} hochladen?",
  savedUploadNow: "{name} wurde gespeichert. Jetzt auf {server} hochladen?",
  theProgramThisSystemOpensFilesWith:
    "Das Programm, mit dem dieses System {extension}-Dateien öffnet",
  theProgramThisSystemOpensSuchFilesWith:
    "Das Programm, mit dem dieses System solche Dateien öffnet",
  holdsAProgramNotOpened:
    "{name} enthält ein Programm oder Skript und wurde daher nicht geöffnet. Poros führt Dateien, die es öffnet, nicht aus. Wählen Sie einen Editor, um sie anzusehen.",
  isAProgramOrAScript:
    "{name} ist ein Programm oder Skript. Poros führt Dateien, die es öffnet, nicht aus. Wählen Sie einen Editor, um sie anzusehen.",
  whenPorosLastClosed: {
    one: "Als Poros zuletzt geschlossen wurde, war diese Datei in einem anderen Programm gespeichert, aber nicht hochgeladen. Laden Sie sie hoch, sobald ihr Server verbunden ist, oder verwerfen Sie die Änderungen. Die Statusleiste behält die Liste.",
    other:
      "Als Poros zuletzt geschlossen wurde, waren diese Dateien in einem anderen Programm gespeichert, aber nicht hochgeladen. Laden Sie sie hoch, sobald ihr Server verbunden ist, oder verwerfen Sie die Änderungen. Die Statusleiste behält die Liste.",
  },
  copiesFromVaultsStayUnencrypted:
    "Kopien von Dateien aus Tresoren bleiben bis dahin unverschlüsselt auf diesem Computer.",
  filesFromThisVaultOpen: {
    one: "{count} Datei aus diesem Tresor ist in anderen Programmen geöffnet:",
    other: "{count} Dateien aus diesem Tresor sind in anderen Programmen geöffnet:",
  },
  filesFromThisServerOpen: {
    one: "{count} Datei von diesem Server ist in anderen Programmen geöffnet:",
    other: "{count} Dateien von diesem Server sind in anderen Programmen geöffnet:",
  },
  couldNotBeUploadedLockingWipes: {
    one: "Diese konnte nicht hochgeladen werden. Trotzdem sperren löscht die Änderungen darin.",
    other: "Diese konnten nicht hochgeladen werden. Trotzdem sperren löscht die Änderungen darin.",
  },
  couldNotBeUploadedClosingWipes: {
    one: "Diese konnte nicht hochgeladen werden. Trotzdem schließen löscht die Änderungen in Tresordateien.",
    other:
      "Diese konnten nicht hochgeladen werden. Trotzdem schließen löscht die Änderungen in Tresordateien.",
  },
  couldNotBeUploadedClosingLeaves: {
    one: "Diese konnte nicht hochgeladen werden. Trotzdem schließen lässt sie in der Liste der Statusleiste, um sie später hochzuladen.",
    other:
      "Diese konnten nicht hochgeladen werden. Trotzdem schließen lässt sie in der Liste der Statusleiste, um sie später hochzuladen.",
  },
  lockingEndsEditing: {
    one: "Das Sperren des Tresors beendet die Bearbeitung der Datei daraus und löscht die Arbeitskopien. Was gespeichert wurde, wird zuerst hochgeladen.",
    other:
      "Das Sperren des Tresors beendet die Bearbeitung der Dateien daraus und löscht die Arbeitskopien. Was gespeichert wurde, wird zuerst hochgeladen.",
  },
  closingLocksVaultEndsEditing: {
    one: "Das Schließen des Tabs sperrt seinen Tresor. Das beendet die Bearbeitung der Datei daraus und löscht die Arbeitskopien. Was gespeichert wurde, wird zuerst hochgeladen.",
    other:
      "Das Schließen des Tabs sperrt seinen Tresor. Das beendet die Bearbeitung der Dateien daraus und löscht die Arbeitskopien. Was gespeichert wurde, wird zuerst hochgeladen.",
  },
  savedButNotReachedTheServer: {
    one: "Diese Datei wurde in anderen Programmen gespeichert, hat den Server aber nicht erreicht:",
    other:
      "Diese Dateien wurden in anderen Programmen gespeichert, haben den Server aber nicht erreicht:",
  },
  quittingWithoutUploadingKeepsThem:
    "Beenden ohne Hochladen behält sie, und Poros bietet beim nächsten Start an, sie hochzuladen.",
  workingCopiesOfVaultFilesAreWiped: "Arbeitskopien von Tresordateien werden gelöscht.",

  // components/ExternalEditorSettings
  openWith: "Öffnen mit",
  openCtrlEnterUsesThe:
    "Öffnen (Ctrl+Enter) verwendet das Programm, das für die Art der Datei eingerichtet ist, sonst das des Systems. Eine Datei auf einem Server wird als Kopie auf diesem Computer geöffnet, die bei jedem Speichern im Programm hochgeladen wird.",
  uploadEachSaveWithoutAsking: "Jedes Speichern ohne Nachfrage hochladen",
  uploadEachSaveWithoutAskingHint: "Aus: Poros fragt vor jedem Hochladen.",
  keepCopiesAfterTheProgram: "Kopien nach dem Schließen des Programms behalten",
  keepCopiesAfterTheProgramHint:
    "Gezählt ab dem letzten Speichern oder Hochladen. Kopien von Dateien in Tresoren werden sofort gelöscht.",
  removeProgram: "Programm entfernen",
  remove: "{programLabel} entfernen",
  program: "Programm",
  arguments: "Argumente",
  defaultFor: "Standard für",
  filesThisProgramOpensBy: "Dateien, die dieses Programm standardmäßig öffnet",
  psdXcf: "*.psd, *.xcf",
  nameOfTheNewProgram: "Name des neuen Programms",
  newProgram: "Neues Programm",
  inTheArgumentsStandsFor:
    "{placeholder} steht in den Argumenten für die Datei. Fehlt es, kommt die Datei zuletzt. Anführungszeichen halten Leerzeichen in einem Argument zusammen. Dateien, die zu einem Muster unter Standard für passen, werden in diesem Programm geöffnet.",

  // components/ExternalEditsStatus
  filesOpenInOtherPrograms: "In anderen Programmen geöffnete Dateien",
  stopEditing: "Bearbeitung von {name} beenden?",
  stopAndDiscard: "Beenden und verwerfen",
  openInOtherPrograms: "In anderen Programmen geöffnet",
  settingsForOpeningFiles: "Einstellungen zum Öffnen von Dateien",
  noFilesAreOpenIn:
    "Keine Dateien sind in anderen Programmen geöffnet. Klicken Sie mit der rechten Maustaste auf eine Datei und wählen Sie Öffnen mit.",
  inAVault: "In einem Tresor",
  uploadedAtTime: "um {time} hochgeladen",
  uploadedOnDate: "am {date} hochgeladen",
  of: "{transferred} von {total}",
  decideWhatToUpload: "Entscheiden, was hochgeladen wird...",
  openInAnEditor: "In einem Editor öffnen...",
  uploadNow: "Jetzt hochladen",
  uploadNow2: "{name} jetzt hochladen",
  openAgain: "Erneut öffnen",
  openAgain2: "{name} erneut öffnen",
  showTheWorkingCopysFolder: "Ordner der Arbeitskopie zeigen",
  showTheFolderOf: "Ordner von {name} zeigen",
  stopEditingAndDeleteThe: "Bearbeitung beenden und Arbeitskopie löschen",
  stopEditing2: "Bearbeitung von {name} beenden",
  savedButNotUploadedWhen: "Gespeichert, aber beim letzten Schließen von Poros nicht hochgeladen",
  saved: "gespeichert {date}",
  thisCopyOfAVault:
    "Diese Kopie einer Tresordatei ist auf diesem Computer nicht verschlüsselt. Laden Sie sie hoch oder verwerfen Sie sie.",
  uploadTo: "Auf {label} hochladen",
  connectToFirst: "Zuerst mit {server} verbinden",
  upload2: "{name} hochladen",
  discardTheSavedChanges: "Gespeicherte Änderungen verwerfen",
  discard: "{name} verwerfen",
  discard2: "{name} verwerfen?",
  notEditingAnyFiles: "Keine Dateien in Bearbeitung",
  eachSaveIsUploadedAs:
    "Jedes Speichern wird hochgeladen, sobald das Programm die Datei geschrieben hat.",
  savesWaitForYouTo: "Gespeichertes wartet darauf, dass Sie es hochladen.",
  changesSavedInProgramNotReached:
    "In {program} gespeicherte Änderungen haben {server} nicht erreicht. Beenden löscht die Arbeitskopie mitsamt diesen Änderungen.",
  changesSavedInTheProgramNotReached:
    "Im Programm gespeicherte Änderungen haben {server} nicht erreicht. Beenden löscht die Arbeitskopie mitsamt diesen Änderungen.",
  connectAndUnlockToUpload:
    "Verbinden Sie sich mit {server} und entsperren Sie den Tresor, um die Datei hochzuladen.",
  connectToUpload: "Verbinden Sie sich mit {server}, um die Datei hochzuladen.",
  neverReachedDiscardingWipes:
    "Die darin gespeicherten Änderungen haben {server} nie erreicht. Verwerfen löscht sie unwiederbringlich.",
  neverReachedDiscardingDeletes:
    "Die darin gespeicherten Änderungen haben {server} nie erreicht. Verwerfen löscht sie.",

  // components/openWithMenu
  openIn: "In {programLabel} öffnen",
  setUpPrograms: "Programme einrichten...",

  // lib/externalEditor
  downloading: "Wird heruntergeladen",
  watchingForSaves: "Wartet auf Speichern",
  savedNotUploaded: "Gespeichert, nicht hochgeladen",
  waitingToUpload: "Wartet auf Hochladen",
  changedOnTheServer2: "Auf dem Server geändert",
  uploading: "Wird hochgeladen",
  uploadFailed: "Hochladen fehlgeschlagen",
  notOpenedItIsAProgram: "Nicht geöffnet: Es ist ein Programm",
  openingNamePercent: "{name} wird geöffnet, {percent}",
  uploadingName: "{name} wird hochgeladen",
  nameNotOpenedItIsAProgram: "{name}: nicht geöffnet, es ist ein Programm",
  nameChangedOnTheServer: "{name}: auf dem Server geändert",
  nameUploadFailed: "{name}: Hochladen fehlgeschlagen",
  editedFilesNeedAttention: {
    one: "{count} bearbeitete Datei braucht Aufmerksamkeit",
    other: "{count} bearbeitete Dateien brauchen Aufmerksamkeit",
  },
  editingName: "{name} wird bearbeitet",
  editingFiles: {
    one: "{count} Datei wird bearbeitet",
    other: "{count} Dateien werden bearbeitet",
  },
  filesNotUploaded: {
    one: "{count} Datei nicht hochgeladen",
    other: "{count} Dateien nicht hochgeladen",
  },

  // state/externalEditStore
  isOpenAlreadyItsWorking: "{name} ist bereits geöffnet. Die Arbeitskopie wird erneut geöffnet.",
  chooseAProgram2: "Programm auswählen",
} satisfies TranslatedArea<"openWith">;
