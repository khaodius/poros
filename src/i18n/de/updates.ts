import type { TranslatedArea } from "../catalog";

export const updates = {
  // components/SettingsDialog
  downloadingVersion: "{version} wird heruntergeladen",
  downloadingVersionPercent: "{version} wird heruntergeladen, {percent}",
  installingVersion: "{version} wird installiert",
  checkingForUpdates: "Suche nach Updates",
  versionIsAvailable: "{version} ist verfügbar",
  upToDateCheckedAt: "Aktuell, geprüft um {time}",
  theVersionYouAreRunning: "Die Version, die Sie verwenden",
  couldNotListTheReleases: "Die Versionen konnten nicht aufgelistet werden: {message}",
  noOtherReleaseToInstall: "Keine andere Version ab {version} zum Installieren.",
  switchToAnotherRelease: "Zu einer anderen Version ab {version} wechseln.",
  versionIsReadyRestart: "{version} ist bereit. Starten Sie Poros neu, um dazu zu wechseln.",
  restartingInto: "Neustart mit {version}",
  couldNotInstallVersionBecause: "{version} konnte nicht installiert werden: {message}",

  // components/StatusBar
  restartToUse: "Neu starten, um {version} zu verwenden",
  restarting: "Wird neu gestartet",
  couldNotInstallVersion: "{version} konnte nicht installiert werden",

  // components/UpdateDialog
  updateAvailable: "Update verfügbar",
  isAvailableYouHave: "Poros {version} ist verfügbar. Sie haben {current}.",
  porosKeepsWorkingWhileThe:
    "Poros arbeitet weiter, während das Update heruntergeladen wird, und fragt vor dem Neustart.",
  skipThisVersion: "Diese Version überspringen",
  later: "Später",
  update: "Aktualisieren",

  // components/UpdateNotice
  hideTheStatusBarShows: "Ausblenden, die Statusleiste zeigt es wieder",
  restartPorosToSwitchTo:
    "Starten Sie Poros jetzt neu, um zu wechseln, oder es wirkt beim nächsten Öffnen von Poros.",
  restartNow: "Jetzt neu starten",
  of: "{received} von {total}",
  downloadingPoros: "Poros {version} wird heruntergeladen",
  installingPoros: "Poros {version} wird installiert",
  porosIsReady: "Poros {version} ist bereit",
  restartingPoros: "Poros wird neu gestartet",
  couldNotInstallPoros: "Poros {version} konnte nicht installiert werden",

  // state/updateStore
  transfersWillStop: {
    one: "{count} Übertragung in der Warteschlange stoppt, wenn Poros neu startet.",
    other: "{count} Übertragungen in der Warteschlange stoppen, wenn Poros neu startet.",
  },
  transfersWillStopAndComeBackPaused: {
    one: "{count} Übertragung in der Warteschlange stoppt beim Neustart von Poros und kehrt angehalten zurück.",
    other:
      "{count} Übertragungen in der Warteschlange stoppen beim Neustart von Poros und kehren angehalten zurück.",
  },
  unsavedChangesInFilesWill: {
    one: "Ungespeicherte Änderungen in {count} Datei gehen beim Neustart von Poros verloren.",
    other: "Ungespeicherte Änderungen in {count} Dateien gehen beim Neustart von Poros verloren.",
  },
  theLatestReleaseDoesNotOfferInAppUpdates:
    "Die neueste Version auf GitHub bietet noch keine Updates in der App an",
  couldNotCheckForUpdates: "Es konnte nicht nach Updates gesucht werden: {message}",
  heldSavesUploadedAfterTheRestart: {
    one: "Eine in einem anderen Programm geöffnete Datei enthält Gespeichertes, das noch nicht hochgeladen ist. Poros bietet nach dem Neustart an, es hochzuladen.",
    other:
      "{count} in anderen Programmen geöffnete Dateien enthalten Gespeichertes, das noch nicht hochgeladen ist. Poros bietet nach dem Neustart an, es hochzuladen.",
  },
  heldSavesWipedByTheRestart: {
    one: "Eine in einem anderen Programm geöffnete Datei enthält Gespeichertes, das noch nicht hochgeladen ist. Der Neustart löscht es, da Tresordateien nicht unverschlüsselt aufbewahrt werden.",
    other:
      "{count} in anderen Programmen geöffnete Dateien enthalten Gespeichertes, das noch nicht hochgeladen ist. Der Neustart löscht es, da Tresordateien nicht unverschlüsselt aufbewahrt werden.",
  },
  heldSavesPartlyWipedByTheRestart: {
    one: "Eine in einem anderen Programm geöffnete Datei enthält Gespeichertes, das noch nicht hochgeladen ist. Was aus Tresoren stammt, wird gelöscht. Für das Übrige bietet Poros nach dem Neustart das Hochladen an.",
    other:
      "{count} in anderen Programmen geöffnete Dateien enthalten Gespeichertes, das noch nicht hochgeladen ist. Was aus Tresoren stammt, wird gelöscht. Für das Übrige bietet Poros nach dem Neustart das Hochladen an.",
  },
} satisfies TranslatedArea<"updates">;
