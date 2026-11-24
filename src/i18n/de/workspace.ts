import type { TranslatedArea } from "../catalog";

export const workspace = {
  // App
  connectToServer: "Mit Server verbinden",
  synchronizeFolders: "Ordner synchronisieren",
  scheduledTasks: "Geplante Aufgaben",
  settingsCtrl: "Einstellungen (Ctrl+,)",

  // components/BottomPanel
  queue: "Warteschlange",
  failed: "Fehlgeschlagen",
  completed: "Erledigt",
  startQueue: "Warteschlange starten",
  pauseQueue: "Warteschlange anhalten",
  pauseSelected: "Auswahl anhalten",
  resumeSelected: "Auswahl fortsetzen",
  moveSelectedToTheTop: "Auswahl ganz nach oben",
  moveSelectedToTheBottom: "Auswahl ganz nach unten",
  removeSelected: "Auswahl entfernen",
  retryAll: "Alle wiederholen",
  retrySelected: "Auswahl wiederholen",
  clearFailedTransfers: "Fehlgeschlagene Übertragungen entfernen",
  keepEachTransferInView: "Jede Übertragung im Blick behalten, wenn sie fertig wird",
  follow: "Folgen",
  clearFinishedTransfers: "Erledigte Übertragungen entfernen",
  simultaneousTransfers: "Gleichzeitige Übertragungen",
  workers: "Parallel",
  log: "Protokoll",

  // components/CloseTabDialog
  close: "{label} schließen?",
  transfersIntoOrOutOf:
    "Übertragungen in einen oder aus einem entsperrten Tresor werden in jedem Fall angehalten: Das Schließen des Tabs sperrt den Tresor. Setzen Sie sie fort, sobald der Tresor wieder entsperrt ist.",
  pauseTransfersAndClose: "Übertragungen anhalten und schließen",
  keepTransferring: "Weiter übertragen",
  transfersForThisServerStillQueued: {
    one: "{count} Übertragung für diesen Server ist noch in der Warteschlange. Sie kann nach dem Schließen des Tabs über eine eigene Verbindung weiterlaufen oder angehalten warten, bis Sie sie fortsetzen.",
    other:
      "{count} Übertragungen für diesen Server sind noch in der Warteschlange. Sie können nach dem Schließen des Tabs über eigene Verbindungen weiterlaufen oder angehalten warten, bis Sie sie fortsetzen.",
  },
  unmountAndClose: "Trennen und schließen",

  // components/DockArea
  dragToResizeDoubleClick: "Ziehen ändert die Größe, Doppelklick gleicht aus",
  moveToMainWindow: "In das Hauptfenster verschieben",
  newWindow: "Neues Fenster",
  moveToThatWindow: "In dieses Fenster verschieben",

  // components/DragGhost
  uploadTo: "Nach {folder} hochladen",
  downloadTo: "Nach {folder} herunterladen",
  copyTo: "Nach {folder} kopieren",
  copyTo2: "Nach {baseName} kopieren",
  moveTo: "Nach {baseName} verschieben",

  // components/DropMarker
  splitRight: "Rechts teilen",
  splitDown: "Unten teilen",
  mergeAsTab: "Als Tab zusammenführen",
  splitLeft: "Links teilen",
  splitUp: "Oben teilen",

  // components/RestoringPane
  signIn: "Anmelden",
  closeTab: "Tab schließen",
  reopensNameOnceConnected: "Öffnet {name} wieder, sobald die Verbindung steht.",
  opensANewShellOnceConnected: "Öffnet eine neue Shell, sobald die Verbindung steht.",
  reopensOnceConnected: "Öffnet wieder, sobald die Verbindung steht.",
  openingName: "{name} wird geöffnet...",
  openingFile: "Datei wird geöffnet...",
  reconnectingTo: "Verbindung zu {label} wird wiederhergestellt...",
  signInTo: "Bei {label} anmelden",
  couldNotReconnectTo: "Verbindung zu {label} konnte nicht wiederhergestellt werden",
  porosKeepsNoPasswordForThisServer: "Poros bewahrt für diesen Server kein Passwort auf.",

  // components/SplashScreen
  loadingPoros: "Poros wird geladen",

  // components/StatusBar
  connectionToLost: "Verbindung zu {label} unterbrochen",
  connectedTo: "Verbunden mit {label}",
  serversConnected: { one: "{count} Server verbunden", other: "{count} Server verbunden" },
  notConnected: "Nicht verbunden",
  updateTo: "Update auf {version}",
  uploadSpeed: "Upload-Geschwindigkeit",
  downloadSpeed: "Download-Geschwindigkeit",
  appVersion: "Poros {version}",
  transfersLeft: { one: "{count} Übertragung übrig", other: "{count} Übertragungen übrig" },
  aboutDuration: "etwa {duration}",
  queueStatusPaused: "{status} (angehalten)",

  // components/TabBar
  openTerminal: "Terminal öffnen",
  moveToNewWindow: "In neues Fenster verschieben",
  closeOtherTabs: "Andere Tabs schließen",
  newTabCtrlT: "Neuer Tab (Ctrl+T)",
  newTab: "Neuer Tab",
  disconnected: "Getrennt",
  unsavedChanges: "Ungespeicherte Änderungen",
  close2: "{label} schließen",
  closeCtrlW: "Schließen (Ctrl+W)",
  mountAsDrive: "Als Laufwerk einbinden...",
  mountedAsADriveAt: "Als Laufwerk eingebunden unter {paths}",
  mountedAsADrive: "Als Laufwerk eingebunden",

  // components/TabContent
  local: "Lokal",
  thisSessionHasEnded: "Diese Sitzung ist beendet",
  connectionLost: "Verbindung unterbrochen",
  reconnectingInS: "Neue Verbindung in {secondsLeft} s",
  reconnecting: "Wird neu verbunden...",
  reconnectingInSecondsAttempt: "Neue Verbindung in {seconds} s (Versuch {attempt})",
  reconnectingAttempt: "Wird neu verbunden... (Versuch {attempt})",
  reconnectNow: "Jetzt neu verbinden",
  reconnect: "Neu verbinden",

  // components/WelcomePanel
  localFiles: "Lokale Dateien",
  import: "Importieren",
  importFromFilezillaWinscpAnd: "Aus FileZilla, WinSCP und anderen Programmen importieren",
  savedConnections: "Gespeicherte Verbindungen",
  serversYouSaveFromThe:
    "Server, die Sie im Verbindungsdialog oder über die Lesezeichen-Schaltfläche eines Server-Tabs speichern, erscheinen hier. In FileZilla, WinSCP, PuTTY und anderen Programmen gespeicherte Einträge lassen sich importieren.",
  connecting: "Wird verbunden...",

  // components/WindowControls
  minimize: "Minimieren",
  restore: "Wiederherstellen",
  maximize: "Maximieren",

  // components/Workspace
  dragToResizeDoubleClickTo: "Ziehen ändert die Größe, Doppelklick setzt sie zurück",

  // hooks/useTabLabel
  closedSession: "Geschlossene Sitzung",

  // state/restoreActions
  couldNotReopen: "{name} konnte nicht wieder geöffnet werden: {message}",
  porosNoLongerKnowsHow: "Poros weiß nicht mehr, wie es erreichbar ist.",
} satisfies TranslatedArea<"workspace">;
