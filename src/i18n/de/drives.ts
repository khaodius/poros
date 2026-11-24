import type { TranslatedArea } from "../catalog";

export const drives = {
  // components/DriveStatus
  filesNotUploaded: {
    one: "{count} Datei nicht hochgeladen",
    other: "{count} Dateien nicht hochgeladen",
  },
  drivesMountedFromPoros: "Von Poros eingebundene Laufwerke",
  mountedDrives: "Eingebundene Laufwerke",
  driveAtMountPoint: "{label} unter {mountPoint}",
  readOnly: "Schreibgeschützt",
  factAt: "Unter",
  factShows: "Zeigt",
  factActivity: "Aktivität",
  uploads: "Uploads",
  unmount: "Trennen",
  refusedWhileProgramsHaveFiles:
    "Nicht möglich, solange Programme Dateien auf dem Laufwerk geöffnet haben oder Uploads ausstehen",
  forceUnmount: "Trennen erzwingen",
  unmountsAtOnceProgramsLose:
    "Trennt sofort: Programme verlieren die geöffneten Dateien, und was noch nicht hochgeladen ist, bleibt auf diesem Computer",
  notUploaded: "Nicht hochgeladen",
  notUploadedKeptOnThis: "Nicht hochgeladen, auf diesem Computer aufbewahrt",
  keptFileFacts: "{connection}, {size}",
  keptFileFactsWithError: "{connection}, {size}: {error}",
  saveACopy: "Kopie speichern...",
  discardForGood: "Endgültig verwerfen",

  // components/DrivesGoing
  unmountedWhenTheTabCloses: {
    one: "Das Laufwerk unter {paths} wird getrennt, wenn der Tab geschlossen wird.",
    other: "Die Laufwerke unter {paths} werden getrennt, wenn der Tab geschlossen wird.",
  },
  unmountedWhenTheVaultLocks: {
    one: "Das Laufwerk unter {paths} wird getrennt, wenn der Tresor gesperrt wird.",
    other: "Die Laufwerke unter {paths} werden getrennt, wenn der Tresor gesperrt wird.",
  },
  unmountedWhenPorosQuits: {
    one: "Das Laufwerk unter {paths} wird getrennt, wenn Poros beendet wird.",
    other: "Die Laufwerke unter {paths} werden getrennt, wenn Poros beendet wird.",
  },
  busyDrivesLoseOpenFiles:
    "Programme haben noch Dateien darauf geöffnet, oder Uploads stehen aus. Die Programme verlieren die geöffneten Dateien, und was noch nicht auf dem Server ist, bleibt auf diesem Computer, aufgeführt unter den Laufwerken in der Statusleiste.",
  busyDrivesLoseOpenFilesWithVault:
    "Programme haben noch Dateien darauf geöffnet, oder Uploads stehen aus. Die Programme verlieren die geöffneten Dateien, und was noch nicht auf dem Server ist, bleibt auf diesem Computer, aufgeführt unter den Laufwerken in der Statusleiste. Aus einem Tresor aufbewahrte Dateien liegen unverschlüsselt vor, bis Sie sie speichern oder verwerfen.",

  // components/MountDialog
  mountNameAsADrive: "{name} als Laufwerk einbinden",
  theServersRoot: "Stammordner des Servers",
  theVaultsRoot: "Stammordner des Tresors",
  rootAt: "{root}: {path}",
  whatProgramsSaveServer:
    "Was Programme speichern, wird hochgeladen, wenn sie die Datei schließen. Das Laufwerk wird getrennt, wenn der Tab geschlossen oder Poros beendet wird.",
  whatProgramsSaveVault:
    "Was Programme speichern, wird hochgeladen, wenn sie die Datei schließen, und bis dahin unverschlüsselt auf diesem Computer gehalten. Das Laufwerk wird getrennt, wenn der Tab geschlossen, der Tresor gesperrt oder Poros beendet wird.",
  mountAsDrive: "Als Laufwerk einbinden",
  theConnectionHasClosed: "Die Verbindung wurde geschlossen.",
  driveLetter: "Laufwerksbuchstabe",
  folderOnThisComputer: "Ordner auf diesem Computer",
  aFreeLetterTheDrive: "Ein freier Buchstabe, unter dem das Laufwerk erscheint.",
  anEmptyFolderMadeIf:
    "Ein leerer Ordner. Er wird angelegt, falls er fehlt, und danach wieder entfernt.",
  showOnTheDrive: "Auf dem Laufwerk zeigen",
  thisFolder: "Diesen Ordner",
  readOnlyProgramsCanOpen: "Schreibgeschützt: Programme können Dateien öffnen, aber nicht ändern",
  openInFileManagerWhen: "Nach dem Einbinden im Dateimanager öffnen",
  theConnectionIsLostReconnect:
    "Die Verbindung ist unterbrochen. Verbinden Sie erneut, um das Laufwerk einzubinden.",
  mounting: "Wird eingebunden...",
  mount: "Einbinden",

  // components/QuitDialog
  quitPoros: "Poros beenden?",
  unmountAndQuit: "Trennen und beenden",

  // lib/mounts
  restartUnmountsDrivesInUse: {
    one: "Auf einem Laufwerk sind Dateien in Programmen geöffnet oder Uploads stehen aus. Ein Neustart trennt es, und was noch nicht hochgeladen ist, bleibt auf diesem Computer.",
    other:
      "Auf {count} Laufwerken sind Dateien in Programmen geöffnet oder Uploads stehen aus. Ein Neustart trennt sie, und was noch nicht hochgeladen ist, bleibt auf diesem Computer.",
  },
  restartUnmountsDrivesInUseWithVault: {
    one: "Auf einem Laufwerk sind Dateien in Programmen geöffnet oder Uploads stehen aus. Ein Neustart trennt es, und was noch nicht hochgeladen ist, bleibt auf diesem Computer, bei Dateien aus einem Tresor unverschlüsselt.",
    other:
      "Auf {count} Laufwerken sind Dateien in Programmen geöffnet oder Uploads stehen aus. Ein Neustart trennt sie, und was noch nicht hochgeladen ist, bleibt auf diesem Computer, bei Dateien aus einem Tresor unverschlüsselt.",
  },
  driveCount: { one: "{count} Laufwerk", other: "{count} Laufwerke" },
  drivesWithFailedUploads: {
    one: "{drives}, {count} Upload fehlgeschlagen",
    other: "{drives}, {count} Uploads fehlgeschlagen",
  },
  drivesUploading: {
    one: "{drives}, {count} wird hochgeladen",
    other: "{drives}, {count} werden hochgeladen",
  },
  filesOpen: { one: "{count} Datei geöffnet", other: "{count} Dateien geöffnet" },
  uploadsPending: { one: "{count} Upload ausstehend", other: "{count} Uploads ausstehend" },
  sizeRead: "{size} gelesen",
  sizeWritten: "{size} geschrieben",
  idle: "Inaktiv",
  uploadFailed: "Fehlgeschlagen: {error}",
  uploadWaiting: "Wartet",
  uploading: "Wird hochgeladen",
  uploadingPercent: "Wird hochgeladen, {percent}",
  chooseADriveLetter: "Wählen Sie einen Laufwerksbuchstaben.",
  chooseAFolder: "Wählen Sie einen Ordner.",
  driveLetterSuchAs: "Ein Laufwerksbuchstabe, etwa {example}",
  fullPathSuchAs: "Ein vollständiger Pfad, etwa {example}",

  // state/mountActions
  savedACopyAt: "Kopie unter {destination} gespeichert",
} satisfies TranslatedArea<"drives">;
