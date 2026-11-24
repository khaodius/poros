import type { TranslatedArea } from "../catalog";

export const connect = {
  // components/ConnectDialog
  selectPrivateKey: "Privaten Schlüssel auswählen",
  savedConnections: "Gespeicherte Verbindungen",
  importFromFilezillaWinscpAnd: "Aus FileZilla, WinSCP und anderen Programmen importieren",
  importConnections: "Verbindungen importieren",
  newConnection: "Neue Verbindung",
  emptyConnectionList: "Gespeicherte Verbindungen erscheinen hier. {importLink}",
  importThemFromAnotherApp: "Aus einem anderen Programm importieren",
  protocol: "Protokoll",
  account: "Konto",
  emptyForAmazonS3: "Leer für Amazon S3",
  port: "Port",
  forAnotherServiceEnterIts:
    "Geben Sie für einen anderen Dienst dessen Endpunkt ein, etwa {example}, oder {local} für einen Server ohne TLS.",
  anonymous: "anonymous",
  emptyIfTheServerNeeds: "Leer, wenn der Server keinen braucht",
  authentication: "Anmeldung",
  savedInTheSystemKeychain: "Im Schlüsselbund des Systems gespeichert",
  privateKeyFile: "Private Schlüsseldatei",
  sshIdEd25519OrC: "~/.ssh/id_ed25519 oder C:\\keys\\server.ppk",
  opensshPemPkcs8And:
    "Unterstützt werden Schlüssel in OpenSSH, PEM, PKCS#8 und PuTTY .ppk (v2 und v3).",
  usesKeysLoadedInYour:
    "Verwendet die Schlüssel, die in Ihrem SSH-Agent geladen sind: {variable} unter Linux und macOS, den Dienst OpenSSH Authentication Agent oder Pageant unter Windows.",
  region: "Region",
  fromTheEndpointOrUs: "Aus dem Endpunkt oder us-east-1",
  bucketAddress: "Bucket-Adresse",
  automatic: "Automatisch",
  pathEndpointBucket: "Pfad (endpoint/bucket)",
  virtualHostedBucketEndpoint: "Virtueller Host (bucket.endpoint)",
  remoteFolderOptional: "Ordner auf dem Server (optional)",
  jumpHost: "Jump-Host",
  noneConnectStraightToThe: "Keiner, direkt mit dem Server verbinden",
  aDeletedConnection: "Eine gelöschte Verbindung",
  logsInToThisSaved:
    "Meldet sich zuerst bei dieser gespeicherten Verbindung an und erreicht den Server über sie, wie ProxyJump in OpenSSH.",
  rememberThePassword: "Passwort im Schlüsselbund des Systems speichern",
  rememberThePassphrase: "Passphrase im Schlüsselbund des Systems speichern",
  rememberTheSecretAccessKey: "Geheimen Zugriffsschlüssel im Schlüsselbund des Systems speichern",
  rememberTheApplicationKey: "Anwendungsschlüssel im Schlüsselbund des Systems speichern",
  activeModeTheServerOpens: "Aktiver Modus: Der Server öffnet Datenverbindungen zu diesem Computer",
  connectWithoutTheProxy: "Ohne Proxy verbinden",
  rsyncAndSynchronization: "rsync und Synchronisierung",
  rsyncOnTheServer: "rsync auf dem Server",
  theCommandThatStartsRsync:
    "Der Befehl, der rsync hier startet, etwa sudo rsync oder /usr/local/bin/rsync. Leer: der aus den Übertragungseinstellungen.",
  rsyncArgumentsForFolderSynchronizations: "rsync-Argumente für Ordnersynchronisierungen",
  rsyncArgumentsForFolderSynchronizationsHint:
    "Synchronisierungen mit diesem Server beginnen mit den Standardwerten aus den Einstellungen, ergänzt um diese.",
  password: "Passwort",
  keyFile: "Schlüsseldatei",
  sshAgent: "SSH-Agent",
  host: "Host",
  username: "Benutzername",
  browserSignIn: "Anmeldung im Browser",
  endpoint: "Endpunkt",
  saveThisConnectionAndIts: "Diese Verbindung und ihre Anmeldung speichern",
  saveThisConnection: "Diese Verbindung speichern",
  deleteForGood: "Endgültig löschen",
  saving: "Wird gespeichert...",
  connecting: "Wird verbunden...",
  shownInSavedConnections: "Erscheint in den gespeicherten Verbindungen",
  topFolderOfTheDrive: "Oberster Ordner des Laufwerks",
  bucketOrBucketFolder: "{bucket} oder {folder}",
  loginFolder: "Ordner nach der Anmeldung",
  theFolderInTheAddressOrTheTop: "Der Ordner in der Adresse oder der oberste",
  homeFolder: "Benutzerordner",
  passphrase: "Passphrase",
  passphraseIfTheKeyHasOne: "Passphrase (falls der Schlüssel eine hat)",
  hostOrAddress: "{host} oder {address}",

  // components/HostKeyDialog
  theKeyPresentedBy:
    "Der Schlüssel, den {target} vorlegt, stimmt nicht mit dem gespeicherten überein. Vielleicht wurde der Server neu installiert, vielleicht fängt jemand die Verbindung ab. Fahren Sie nur fort, wenn Sie wissen, warum sich der Schlüssel geändert hat.",
  hasNotBeenSeenBefore:
    "{target} ist bisher unbekannt. Vergleichen Sie den Fingerabdruck mit dem, den Ihnen Ihr Serveradministrator genannt hat, bevor Sie fortfahren.",
  fingerprint: "Fingerabdruck",
  connectOnce: "Einmal verbinden",
  theCertificatePresentedBy:
    "Das Zertifikat, das {target} vorlegt, stimmt nicht mit dem überein, dem Sie zuvor vertraut haben. {problem} Vergleichen Sie den Fingerabdruck mit dem, den Ihnen Ihr Serveradministrator genannt hat, bevor Sie fortfahren.",
  thisComputerDoesNotTrust:
    "Dieser Computer vertraut dem Zertifikat nicht, das {target} vorlegt. {problem} Vergleichen Sie den Fingerabdruck mit dem, den Ihnen Ihr Serveradministrator genannt hat, bevor Sie fortfahren.",
  type: "Typ",
  keyType: "Schlüsseltyp",
  trustAndConnect: "Vertrauen und verbinden",
  certificateChanged: "Zertifikat geändert",
  untrustedCertificate: "Nicht vertrauenswürdiges Zertifikat",
  hostKeyChanged: "Hostschlüssel geändert",
  unknownHost: "Unbekannter Host",
  replaceCertificateAndConnect: "Zertifikat ersetzen und verbinden",
  replaceKeyAndConnect: "Schlüssel ersetzen und verbinden",

  // components/ImportConnectionsDialog
  importFrom: "Aus {label} importieren",
  importedFrom: {
    one: "{count} Verbindung aus {label} importiert.",
    other: "{count} Verbindungen aus {label} importiert.",
  },
  imported: { one: "{count} Verbindung importiert.", other: "{count} Verbindungen importiert." },
  theseCouldNotBeSaved: "Diese konnten nicht gespeichert werden: {failures}",
  passwordsLeftOut: {
    one: "Der Schlüsselbund des Systems ist nicht verfügbar, daher wurde {count} Passwort ausgelassen. Poros fragt danach, wenn Sie verbinden.",
    other:
      "Der Schlüsselbund des Systems ist nicht verfügbar, daher wurden {count} Passwörter ausgelassen. Poros fragt danach, wenn Sie verbinden.",
  },
  porosWorksOutWhichApp: "Poros erkennt, welches Programm die Datei geschrieben hat",
  lookingForOtherApps: "Andere Programme werden gesucht...",
  porosCopiesTheSavedSites:
    "Poros kopiert die gespeicherten Einträge dieser Programme in seine gespeicherten Verbindungen. Im anderen Programm ändert sich nichts.",
  chooseFile: "Datei auswählen...",
  alreadySaved: "Schon gespeichert",
  leftOutSincePorosCannot: {
    one: "{count} ausgelassen, da Poros sich so nicht damit verbinden kann",
    other: "{count} ausgelassen, da Poros sich so nicht mit ihnen verbinden kann",
  },
  putFolderNamesInFront: "Ordnernamen vor die Verbindungsnamen setzen",
  reading: "Wird gelesen...",
  otherFile: "Andere Datei...",
  importing: "Wird importiert...",
  import: "Importieren",
  unnamed: "Unbenannt",
  noSavedSitesIn: "In {origins} gibt es keine gespeicherten Einträge.",
  noSavedSitesInThatFile: "In dieser Datei gibt es keine gespeicherten Einträge.",
  importCountConnections: {
    one: "{count} Verbindung importieren",
    other: "{count} Verbindungen importieren",
  },
  foundFiles: "Gefunden: {files}",
  notFoundInChoose: "Nicht gefunden in {places}. {choose}",
  throughConnection: "über {name}",
  neededByConnection: "für {name} nötig",
  placeOrPlace: "{first} oder {second}",
  keepThePasswordsInTheKeychain: {
    one: "Das Passwort im Schlüsselbund des Systems speichern",
    other: "Die {count} Passwörter im Schlüsselbund des Systems speichern",
  },
  sitesFrom: { one: "{count} Eintrag aus {source}", other: "{count} Einträge aus {source}" },

  // components/QuickConnectBar
  cloudStorage: "Cloud-Speicher",
  hostOrUserHostPort: "Host oder benutzer@host:port",

  // components/SaveConnectionDialog
  saveConnection: "Verbindung speichern",

  // lib/connectionImport
  filezilla: "FileZilla",
  winscp: "WinSCP",
  smartftp: "SmartFTP",
  cyberduckAndMountainDuck: "Cyberduck und Mountain Duck",
  puttyAndKitty: "PuTTY und KiTTY",
  mobaxterm: "MobaXterm",
  opensshConfig: "OpenSSH-Konfiguration",
  gnomeFilesAndKdeDolphin: "GNOME Dateien und KDE Dolphin",
  listOfAddresses: "Adressliste",
  chooseFileZillaFiles:
    "Wählen Sie sitemanager.xml oder eine aus dem Servermanager exportierte Datei (Datei > Exportieren).",
  chooseWinScpFiles:
    "Wählen Sie WinSCP.ini oder eine Datei aus Tools > Export/Backup Configuration.",
  chooseSmartFtpFiles:
    "Wählen Sie die Liste aus Favorites > Tools > Export oder .xml-Favoriten älterer Versionen.",
  chooseCyberduckFiles: "Wählen Sie die Lesezeichendateien (.duck).",
  choosePuttyFiles:
    "Wählen Sie einen .reg-Export des Registrierungsschlüssels Sessions oder die Sitzungsdateien von PuTTY.",
  chooseMobaXtermFiles:
    "Wählen Sie MobaXterm.ini oder eine .mxtsessions-Datei aus Sessions > Export.",
  chooseOpenSshFiles: "Wählen Sie eine SSH-Konfigurationsdatei, etwa ~/.ssh/config.",
  chooseFileManagerFiles: "Wählen Sie gtk-3.0/bookmarks oder user-places.xbel.",
  chooseUrlListFiles: "Wählen Sie eine Textdatei mit einer sftp://- oder ftp://-Adresse je Zeile.",

  // lib/protocols
  ftpsExplicitTls: "FTPS (explizites TLS)",
  ftpsImplicitTls: "FTPS (implizites TLS)",
  ftpsImplicit: "FTPS implizit",
  webdavHttp: "WebDAV (HTTP)",
  webdav: "WebDAV",
  webdavHttps: "WebDAV (HTTPS)",
  webdavs: "WebDAVS",
  s3AndCompatible: "S3 und kompatible",
  backblazeB2: "Backblaze B2",
  googleDrive: "Google Drive",
  onedrive: "OneDrive",
  box: "Box",
  cryptomatorVault: "Cryptomator-Tresor",
  vault: "Tresor",
  google: "Google",
  microsoft: "Microsoft",
  accessKeyId: "Zugriffsschlüssel-ID",
  secretAccessKey: "Geheimer Zugriffsschlüssel",
  applicationKeyId: "Anwendungsschlüssel-ID",
  applicationKey: "Anwendungsschlüssel",

  // state/connectFlow
  theHostKeyWasNot: "Der Hostschlüssel wurde nicht akzeptiert.",

  // state/sessionStore
  itsTabWasClosedMeanwhile: "Der zugehörige Tab wurde inzwischen geschlossen.",
} satisfies TranslatedArea<"connect">;
