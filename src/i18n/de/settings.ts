import type { TranslatedArea } from "../catalog";

export const settings = {
  // components/LanguagePicker
  languageSystem: "System ({language})",

  // components/SettingsDialog
  settingsSections: "Bereiche der Einstellungen",
  queue: "Warteschlange",
  simultaneousTransfers: "Gleichzeitige Übertragungen",
  simultaneousTransfersHint: "Wie viele Dateien gleichzeitig übertragen werden.",
  startTransfersAsSoonAs: "Übertragungen sofort nach dem Einreihen starten",
  startTransfersAsSoonAsHint:
    "Aus: Die Warteschlange wartet, bis Sie auf Warteschlange starten klicken.",
  whenTheTargetFileExists: "Wenn die Zieldatei schon vorhanden ist",
  keepFinishedTransfersInThe: "Fertige Übertragungen in der Liste behalten",
  keepUnfinishedTransfersWhenPoros: "Unfertige Übertragungen beim Schließen von Poros behalten",
  keepUnfinishedTransfersWhenPorosHint:
    "Sie kehren beim nächsten Start angehalten zurück. Passwörter werden nie mit ihnen gespeichert.",
  connections: "Verbindungen",
  giveEachTransferItsOwn: "Jeder Übertragung eine eigene Verbindung geben",
  giveEachTransferItsOwnHint:
    "Aus: Übertragungen teilen sich die Verbindung zum Durchsuchen, für Server, die Anmeldungen begrenzen.",
  splitLargeFilesAcrossConnections: "Große Dateien auf mehrere Verbindungen aufteilen",
  splitLargeFilesAcrossConnectionsHint:
    "Freie Übertragungsplätze helfen bei einer großen Datei mit und übertragen verschiedene Teile davon.",
  splitFilesLargerThan: "Dateien aufteilen, die größer sind als",
  connectionsPerFile: "Verbindungen pro Datei",
  copyDirectlyBetweenFtpServers: "Direkt zwischen FTP-Servern kopieren (FXP)",
  copyDirectlyBetweenFtpServersHint:
    "Beide Server müssen es erlauben. Sonst, und zwischen anderen Servern, laufen Dateien durch Poros.",
  bandwidth: "Bandbreite",
  uploadLimit: "Upload-Begrenzung",
  uploadLimitHint: "0 bedeutet unbegrenzt. Gilt für alle Übertragungen zusammen.",
  downloadLimit: "Download-Begrenzung",
  buffers: "Puffer",
  requestSize: "Anfragegröße",
  requestSizeHint:
    "Bytes pro SFTP-Lese- oder Schreibvorgang. Die meisten Server akzeptieren bis zu 255 KiB, manche nur 32.",
  requestsInFlight: "Laufende Anfragen",
  requestsInFlightHint: "Offene Anfragen pro Verbindung. Mehr gleicht mehr Netzwerklatenz aus.",
  files: "Dateien",
  keepModificationTimes: "Änderungszeiten beibehalten",
  keepPermissions: "Berechtigungen beibehalten",
  keepPermissionsHint: "Kopiert die Unix-Berechtigungsbits. Wirkt nicht bei Dateien von Windows.",
  integrity: "Integrität",
  writeToATemporaryName: "Unter temporärem Namen schreiben, dann umbenennen",
  writeToATemporaryNameHint:
    "Eine Datei erscheint erst unter ihrem Namen, wenn sie vollständig und geprüft ist, und eine ersetzte Datei bleibt nie halb geschrieben zurück. Uploads auf FTP-Server werden direkt geschrieben.",
  verifyChecksums: "Prüfsummen kontrollieren",
  verifyChecksumsHint:
    "Vergleicht nach jeder Datei einen SHA-256 beider Kopien, auf SFTP-Servern mit deren eigenem sha256sum berechnet. SHA-1 und MD5 werden dafür nie verwendet, da sie sich fälschen lassen. Liest jede Datei auf beiden Seiten ein weiteres Mal.",
  readFilesBackWhenThe: "Dateien zurücklesen, wenn der Server sie nicht hashen kann",
  readFilesBackWhenTheHint:
    "Auf SFTP-Servern ohne sha256sum sowie auf FTP- und WebDAV-Servern wird jede fertige Datei zurückgelesen und hier gehasht. Verdoppelt den Datenverkehr mit diesen Servern. Cloud-Speicher wird nicht zurückgelesen.",
  repairOnlyTheDamagedParts: "Nur die beschädigten Teile reparieren",
  repairOnlyTheDamagedPartsHint:
    "Nach einer Abweichung wird jeder Teil der Datei einzeln verglichen, und nur die abweichenden Teile werden erneut gesendet, bis zur Hälfte der Datei. Braucht GNU split auf dem Server oder das Zurücklesen.",
  checkTheTransferredPartBefore: "Übertragenen Teil vor dem Fortsetzen prüfen",
  checkTheTransferredPartBeforeHint:
    "Vergleicht Teil für Teil, was schon vorhanden ist, mit der Quelle und setzt beim ersten abweichenden Teil fort. Wo der Server Teile einer Datei nicht hashen kann, werden die letzten 64 KiB verglichen.",
  flushToDiskBeforeFinishing: "Vor dem Abschluss auf den Datenträger schreiben",
  flushToDiskBeforeFinishingHint:
    "Eine Datei gilt erst als fertig, wenn sie gespeichert ist, sodass ein Stromausfall direkt danach sie nicht kosten kann. Auf Servern, die das unterstützen. Verlangsamt viele kleine Dateien.",
  checkTimesAndPermissionsAfter: "Zeiten und Berechtigungen nach dem Setzen prüfen",
  checkTimesAndPermissionsAfterHint:
    "Liest sie zurück, sobald eine Datei an ihrem Platz ist, und vermerkt im Protokoll, was der Server oder das Dateisystem nicht behalten hat.",
  checkPublishedChecksumsAndSignatures: "Veröffentlichte Prüfsummen und Signaturen prüfen",
  checkPublishedChecksumsAndSignaturesHint:
    "Downloads werden mit SHA256SUMS, name.sha256 und ähnlichen Dateien daneben verglichen, mit .minisig-Signaturen und über gpg mit .asc- und .sig-Signaturen. Eine Datei, die durchfällt, wird nicht gespeichert.",
  errors: "Fehler",
  retries: "Wiederholungen",
  retriesHint:
    "Bei Zeitüberschreitungen, fehlgeschlagenen Prüfungen und Verbindungen, die gleich wieder abbrechen.",
  waitBetweenRetries: "Wartezeit zwischen Wiederholungen",
  trustedMinisignKeys: "Vertrauenswürdige minisign-Schlüssel",
  onePublicKeyPerLine:
    "Ein öffentlicher Schlüssel pro Zeile, wie in der minisign.pub eines Herausgebers. Signaturen anderer Schlüssel werden im Protokoll vermerkt und nicht geprüft.",
  autoTuneReceiveBuffer: "Empfangspuffer automatisch anpassen",
  autoTuneReceiveBufferHint:
    "Das System vergrößert das TCP-Empfangsfenster passend zur Verbindung. Gilt für neue Verbindungen.",
  receiveBufferSize: "Größe des Empfangspuffers",
  receiveBufferSizeHint: "SO_RCVBUF, verwendet, wenn die automatische Anpassung aus ist.",
  autoTuneSendBuffer: "Sendepuffer automatisch anpassen",
  autoTuneSendBufferHint:
    "Das System bemisst den TCP-Sendepuffer passend zur Verbindung. Gilt für neue Verbindungen.",
  sendBufferSize: "Größe des Sendepuffers",
  sendBufferSizeHint: "SO_SNDBUF, verwendet, wenn die automatische Anpassung aus ist.",
  connectionTimeout: "Zeitlimit für Verbindungen",
  keepaliveInterval: "Keepalive-Intervall",
  keepaliveIntervalHint: "0 schaltet Keepalives aus. Gilt für neue Verbindungen.",
  compressTraffic: "Datenverkehr komprimieren",
  compressTrafficHint:
    "Hilft bei Text über langsame Leitungen, bremst bereits komprimierte Dateien. Gilt für neue Verbindungen.",
  reconnecting: "Erneut verbinden",
  reconnectLostTabsAutomatically: "Unterbrochene Tabs automatisch neu verbinden",
  reconnectLostTabsAutomaticallyHint:
    "Versucht es in wachsenden Abständen erneut, ohne nach bereits eingegebenen Passwörtern zu fragen.",
  keepTryingFor: "Weiter versuchen für",
  keepTryingForHint:
    "So lange warten Tabs und Übertragungen auf einen unterbrochenen Server. Übertragungen behalten in dieser Zeit ihre Wiederholungen. 0 gibt sofort auf.",
  savedConnections: "Gespeicherte Verbindungen",
  saveQuickConnectionsAutomatically: "Schnellverbindungen automatisch speichern",
  saveQuickConnectionsAutomaticallyHint:
    "Fügt jeden neuen Server, den Sie über die Leiste für Schnellverbindungen erreichen, ohne sein Passwort hinzu.",
  proxy: "Proxy",
  newSftpConnectionsReachTheir:
    "Neue SFTP-Verbindungen erreichen ihren Server über diesen Proxy, außer eine Verbindung ist so eingestellt, dass sie ohne ihn verbindet. Mit einem Jump-Host verwendet nur die Verbindung zum Jump-Host den Proxy. FTP- und Cloud-Verbindungen verwenden ihn nicht.",
  type: "Typ",
  host: "Host",
  port: "Port",
  username: "Benutzername",
  usernameHint: "Leer lassen für einen Proxy ohne Anmeldung. SOCKS4 sendet nur den Benutzernamen.",
  password: "Passwort",
  passwordHint: "Im Schlüsselbund des Systems gespeichert.",
  forget: "Vergessen",
  lookUpServerNamesThrough: "Servernamen über den Proxy auflösen",
  lookUpServerNamesThroughHint:
    "Der DNS dieses Computers sieht nie die Namen der Server, mit denen Sie sich verbinden. HTTP-Proxys tun das immer.",
  proxyPassword: "Proxy-Passwort",
  saved: "Gespeichert",
  doubleClickingAFile: "Doppelklick auf eine Datei",
  transfersItToTheOther: "Überträgt sie auf die andere Seite",
  opensItInTheBuilt: "Öffnet sie im eingebauten Editor",
  opensItInItsProgram: "Öffnet sie in ihrem Programm",
  asksHowToOpenIt: "Fragt, wie sie geöffnet wird",
  doesNothing: "Tut nichts",
  showHiddenFilesInNew: "Versteckte Dateien in neuen Tabs zeigen",
  foldersFirst: "Ordner zuerst",
  foldersFirstHint: "Zeigt Ordner bei jeder Sortierung über den Dateien.",
  dates: "Datumsangaben",
  columns: "Spalten",
  columnsHint:
    "Ein Rechtsklick auf eine Spaltenüberschrift blendet Spalten ein oder aus und ändert die Sortierung. Ziehen Sie die Linie vor einer Überschrift, um die Spalte breiter oder schmaler zu machen, oder doppelklicken Sie darauf, um sie anzupassen.",
  serverFolders: "Ordner auf Servern",
  showCachedListingsWhileRefreshing: "Zwischengespeicherte Listen beim Aktualisieren zeigen",
  showCachedListingsWhileRefreshingHint:
    "Ein schon geöffneter Ordner erscheint sofort so, wie er zuletzt aussah, und wird aktualisiert, sobald der Server antwortet. Listen bleiben nur im Arbeitsspeicher, nie auf dem Datenträger.",
  loadNearbyFoldersAhead: "Benachbarte Ordner vorab laden",
  loadNearbyFoldersAheadHint:
    "Listet den übergeordneten Ordner und den Ordner unter dem Cursor, während der Server nichts zu tun hat, einen nach dem anderen. Pausiert bei FTP, solange Übertragungen laufen.",
  alsoOnCloudStorage: "Auch auf Cloud-Speicher",
  alsoOnCloudStorageHint:
    "Aus: Auf Cloud-Speicher, wo Anfragen gezählt und begrenzt werden, wird nichts vorab gelistet.",
  tabsAndWindows: "Tabs und Fenster",
  askBeforeClosingATab: "Vor dem Schließen eines Tabs mit wartenden Übertragungen fragen",
  rememberTabsAndLayout: "Tabs und Anordnung merken",
  rememberTabsAndLayoutHint:
    "Öffnet die Fenster, Bereiche und Tabs vom letzten Beenden wieder, jeden in seinem Ordner.",
  reconnectToServersOnStart: "Beim Start wieder mit Servern verbinden",
  reconnectToServersOnStartHint: "Aus: Wieder geöffnete Server-Tabs warten, bis Sie sie verbinden.",
  useTheSystemTitleBar: "Titelleiste des Systems verwenden",
  useTheSystemTitleBarHint:
    "Aus: Die obere Leiste trägt die Fensterknöpfe und verschiebt das Fenster.",
  log: "Protokoll",
  linesToKeep: "Zu behaltende Zeilen",
  showTimes: "Uhrzeiten zeigen",
  wrapLongLines: "Lange Zeilen umbrechen",
  logEveryTransferredFile: "Jede übertragene Datei protokollieren",
  logEveryTransferredFileHint:
    "Aus: Eine Zeile mit Zusammenfassung, wenn die Warteschlange fertig ist.",
  updates: "Updates",
  checkForUpdatesWhenPoros: "Beim Start von Poros nach Updates suchen",
  checkForUpdatesWhenPorosHint:
    "Sucht auf GitHub nach einer neueren Version. Nichts wird heruntergeladen, bevor Sie die Installation wählen.",
  poros: "Poros {version}",
  restartNow: "Jetzt neu starten",
  updating: "Wird aktualisiert...",
  showUpdate: "Update zeigen",
  isNotAvailable: "{version} ist nicht verfügbar: {message}",
  installAnotherVersion: "Andere Version installieren",
  version: "Version",
  install: "Installieren",
  transfers: "Übertragungen",
  sync: "Synchronisierung",
  connection: "Verbindung",
  cloudAccounts: "Cloud-Konten",
  automation: "Automatisierung",
  archives: "Archive",
  interface: "Oberfläche",
  appearance: "Darstellung",
  overwriteIfTheSourceIs: "Überschreiben, wenn die Quelle neuer ist",
  overwriteIfSizeOrDate: "Überschreiben, wenn Größe oder Datum abweichen",
  keepBoth: "Beide behalten",
  checking: "Wird geprüft...",
  checkForUpdates: "Nach Updates suchen",
  listing: "Wird gelistet...",
  showVersions: "Versionen zeigen",
  language: "Sprache",
  languageHint:
    "System folgt der Sprache des Computers. Meldungen von Servern und das Protokoll bleiben auf Englisch.",

  // components/Stepper
  decrease: "{label} verringern",
  increase: "{label} erhöhen",
} satisfies TranslatedArea<"settings">;
