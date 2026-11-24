import type { TranslatedArea } from "../catalog";

export const connect = {
  // components/ConnectDialog
  selectPrivateKey: "Choisir une clé privée",
  savedConnections: "Connexions enregistrées",
  importFromFilezillaWinscpAnd: "Importer depuis FileZilla, WinSCP et d’autres applications",
  importConnections: "Importer des connexions",
  newConnection: "Nouvelle connexion",
  emptyConnectionList: "Les connexions que vous enregistrez apparaissent ici. {importLink}",
  importThemFromAnotherApp: "Les importer depuis une autre application",
  protocol: "Protocole",
  account: "Compte",
  emptyForAmazonS3: "Vide pour Amazon S3",
  port: "Port",
  forAnotherServiceEnterIts:
    "Pour un autre service, saisissez son point de terminaison, par exemple {example}, ou {local} pour un serveur sans TLS.",
  anonymous: "anonymous",
  emptyIfTheServerNeeds: "Vide si le serveur n’en demande pas",
  authentication: "Authentification",
  savedInTheSystemKeychain: "Enregistré dans le trousseau du système",
  privateKeyFile: "Fichier de clé privée",
  sshIdEd25519OrC: "~/.ssh/id_ed25519 ou C:\\keys\\server.ppk",
  opensshPemPkcs8And:
    "Les clés OpenSSH, PEM, PKCS#8 et PuTTY .ppk (v2 et v3) sont prises en charge.",
  usesKeysLoadedInYour:
    "Utilise les clés chargées dans votre agent SSH\u00a0: {variable} sous Linux et macOS, le service OpenSSH Authentication Agent ou Pageant sous Windows.",
  region: "Région",
  fromTheEndpointOrUs: "Selon le point de terminaison, ou us-east-1",
  bucketAddress: "Adresse du compartiment",
  automatic: "Automatique",
  pathEndpointBucket: "Chemin (endpoint/bucket)",
  virtualHostedBucketEndpoint: "Hébergement virtuel (bucket.endpoint)",
  remoteFolderOptional: "Dossier distant (facultatif)",
  jumpHost: "Hôte de rebond",
  noneConnectStraightToThe: "Aucun, se connecter directement au serveur",
  aDeletedConnection: "Une connexion supprimée",
  logsInToThisSaved:
    "Se connecte d’abord à cette connexion enregistrée et atteint le serveur par elle, comme ProxyJump dans OpenSSH.",
  rememberThePassword: "Mémoriser le mot de passe dans le trousseau du système",
  rememberThePassphrase: "Mémoriser la phrase secrète dans le trousseau du système",
  rememberTheSecretAccessKey: "Mémoriser la clé d’accès secrète dans le trousseau du système",
  rememberTheApplicationKey: "Mémoriser la clé d’application dans le trousseau du système",
  activeModeTheServerOpens:
    "Mode actif\u00a0: le serveur ouvre les connexions de données vers cet ordinateur",
  connectWithoutTheProxy: "Se connecter sans le proxy",
  rsyncAndSynchronization: "rsync et synchronisation",
  rsyncOnTheServer: "rsync sur le serveur",
  theCommandThatStartsRsync:
    "La commande qui lance rsync sur ce serveur, par exemple sudo rsync ou /usr/local/bin/rsync. Vide, celle des paramètres de transfert est utilisée.",
  rsyncArgumentsForFolderSynchronizations: "Arguments rsync pour la synchronisation de dossiers",
  rsyncArgumentsForFolderSynchronizationsHint:
    "Les synchronisations avec ce serveur partent des valeurs par défaut des paramètres, auxquelles ces arguments s’ajoutent.",
  password: "Mot de passe",
  keyFile: "Fichier de clé",
  sshAgent: "Agent SSH",
  host: "Hôte",
  username: "Nom d’utilisateur",
  browserSignIn: "Connexion dans le navigateur",
  endpoint: "Point de terminaison",
  saveThisConnectionAndIts: "Enregistrer cette connexion et ses identifiants",
  saveThisConnection: "Enregistrer cette connexion",
  deleteForGood: "Supprimer définitivement",
  saving: "Enregistrement...",
  connecting: "Connexion...",
  shownInSavedConnections: "Affiché dans les connexions enregistrées",
  topFolderOfTheDrive: "Dossier racine du stockage",
  bucketOrBucketFolder: "{bucket} ou {folder}",
  loginFolder: "Dossier de connexion",
  theFolderInTheAddressOrTheTop: "Le dossier de l’adresse, ou la racine",
  homeFolder: "Dossier personnel",
  passphrase: "Phrase secrète",
  passphraseIfTheKeyHasOne: "Phrase secrète (si la clé en a une)",
  hostOrAddress: "{host} ou {address}",

  // components/HostKeyDialog
  theKeyPresentedBy:
    "La clé présentée par {target} ne correspond pas à celle qui est enregistrée. Le serveur a peut-être été réinstallé, ou quelqu’un intercepte peut-être la connexion. Ne continuez que si vous savez pourquoi la clé a changé.",
  hasNotBeenSeenBefore:
    "{target} n’a encore jamais été vu. Comparez l’empreinte avec celle que l’administrateur de votre serveur vous a donnée avant de continuer.",
  fingerprint: "Empreinte",
  connectOnce: "Se connecter une fois",
  theCertificatePresentedBy:
    "Le certificat présenté par {target} ne correspond pas à celui auquel vous avez fait confiance. {problem} Comparez l’empreinte avec celle que l’administrateur de votre serveur vous a donnée avant de continuer.",
  thisComputerDoesNotTrust:
    "Cet ordinateur ne fait pas confiance au certificat présenté par {target}. {problem} Comparez l’empreinte avec celle que l’administrateur de votre serveur vous a donnée avant de continuer.",
  type: "Type",
  keyType: "Type de clé",
  trustAndConnect: "Faire confiance et se connecter",
  certificateChanged: "Certificat modifié",
  untrustedCertificate: "Certificat non approuvé",
  hostKeyChanged: "Clé d’hôte modifiée",
  unknownHost: "Hôte inconnu",
  replaceCertificateAndConnect: "Remplacer le certificat et se connecter",
  replaceKeyAndConnect: "Remplacer la clé et se connecter",

  // components/ImportConnectionsDialog
  importFrom: "Importer depuis {label}",
  importedFrom: {
    one: "{count} connexion importée depuis {label}.",
    many: "{count} de connexions importées depuis {label}.",
    other: "{count} connexions importées depuis {label}.",
  },
  imported: {
    one: "{count} connexion importée.",
    many: "{count} de connexions importées.",
    other: "{count} connexions importées.",
  },
  theseCouldNotBeSaved: "Impossible d’enregistrer celles-ci\u00a0: {failures}",
  passwordsLeftOut: {
    one: "Le trousseau du système n’est pas disponible, donc {count} mot de passe a été écarté\u202f; Poros le demandera à la connexion.",
    many: "Le trousseau du système n’est pas disponible, donc {count} de mots de passe ont été écartés\u202f; Poros les demandera à la connexion.",
    other:
      "Le trousseau du système n’est pas disponible, donc {count} mots de passe ont été écartés\u202f; Poros les demandera à la connexion.",
  },
  porosWorksOutWhichApp: "Poros détermine quelle application a écrit le fichier",
  lookingForOtherApps: "Recherche d’autres applications...",
  porosCopiesTheSavedSites:
    "Poros copie les sites enregistrés de ces applications dans ses propres connexions enregistrées. Rien ne change dans l’autre application.",
  chooseFile: "Choisir un fichier...",
  alreadySaved: "Déjà enregistré",
  leftOutSincePorosCannot: {
    one: "{count} connexion écartée, car Poros ne peut pas l’utiliser telle quelle",
    many: "{count} de connexions écartées, car Poros ne peut pas les utiliser telles quelles",
    other: "{count} connexions écartées, car Poros ne peut pas les utiliser telles quelles",
  },
  putFolderNamesInFront: "Placer le nom des dossiers devant le nom des connexions",
  reading: "Lecture...",
  otherFile: "Autre fichier...",
  importing: "Importation...",
  import: "Importer",
  unnamed: "Sans nom",
  noSavedSitesIn: "Il n’y a aucun site enregistré dans {origins}.",
  noSavedSitesInThatFile: "Il n’y a aucun site enregistré dans ce fichier.",
  importCountConnections: {
    one: "Importer {count} connexion",
    many: "Importer {count} de connexions",
    other: "Importer {count} connexions",
  },
  foundFiles: "Trouvé\u00a0: {files}",
  notFoundInChoose: "Introuvable dans {places}. {choose}",
  throughConnection: "via {name}",
  neededByConnection: "nécessaire pour {name}",
  placeOrPlace: "{first} ou {second}",
  keepThePasswordsInTheKeychain: {
    one: "Conserver le mot de passe dans le trousseau du système",
    many: "Conserver les {count} de mots de passe dans le trousseau du système",
    other: "Conserver les {count} mots de passe dans le trousseau du système",
  },
  sitesFrom: {
    one: "{count} site de {source}",
    many: "{count} de sites de {source}",
    other: "{count} sites de {source}",
  },

  // components/QuickConnectBar
  cloudStorage: "Stockage cloud",
  hostOrUserHostPort: "Hôte ou utilisateur@hôte:port",

  // components/SaveConnectionDialog
  saveConnection: "Enregistrer la connexion",

  // lib/connectionImport
  filezilla: "FileZilla",
  winscp: "WinSCP",
  smartftp: "SmartFTP",
  cyberduckAndMountainDuck: "Cyberduck et Mountain Duck",
  puttyAndKitty: "PuTTY et KiTTY",
  mobaxterm: "MobaXterm",
  opensshConfig: "Configuration OpenSSH",
  gnomeFilesAndKdeDolphin: "GNOME Fichiers et KDE Dolphin",
  listOfAddresses: "Liste d’adresses",
  chooseFileZillaFiles:
    "Choisissez sitemanager.xml, ou un fichier exporté depuis le Gestionnaire de sites (Fichier > Exporter).",
  chooseWinScpFiles:
    "Choisissez WinSCP.ini, ou un fichier issu de Outils > Exporter/Sauvegarder la configuration.",
  chooseSmartFtpFiles:
    "Choisissez la liste issue de Favoris > Outils > Exporter, ou les favoris .xml des anciennes versions.",
  chooseCyberduckFiles: "Choisissez leurs fichiers de signets (.duck).",
  choosePuttyFiles:
    "Choisissez un export .reg de leur clé de registre Sessions, ou les fichiers de session de PuTTY.",
  chooseMobaXtermFiles:
    "Choisissez MobaXterm.ini, ou un fichier .mxtsessions issu de Sessions > Export.",
  chooseOpenSshFiles: "Choisissez un fichier de configuration ssh, par exemple ~/.ssh/config.",
  chooseFileManagerFiles: "Choisissez gtk-3.0/bookmarks ou user-places.xbel.",
  chooseUrlListFiles:
    "Choisissez un fichier texte contenant une adresse sftp:// ou ftp:// par ligne.",

  // lib/protocols
  ftpsExplicitTls: "FTPS (TLS explicite)",
  ftpsImplicitTls: "FTPS (TLS implicite)",
  ftpsImplicit: "FTPS implicite",
  webdavHttp: "WebDAV (HTTP)",
  webdav: "WebDAV",
  webdavHttps: "WebDAV (HTTPS)",
  webdavs: "WebDAVS",
  s3AndCompatible: "S3 et compatibles",
  backblazeB2: "Backblaze B2",
  googleDrive: "Google Drive",
  onedrive: "OneDrive",
  box: "Box",
  cryptomatorVault: "Coffre Cryptomator",
  vault: "Coffre",
  google: "Google",
  microsoft: "Microsoft",
  accessKeyId: "ID de clé d’accès",
  secretAccessKey: "Clé d’accès secrète",
  applicationKeyId: "ID de clé d’application",
  applicationKey: "Clé d’application",

  // state/connectFlow
  theHostKeyWasNot: "La clé d’hôte n’a pas été acceptée.",

  // state/sessionStore
  itsTabWasClosedMeanwhile: "Son onglet a été fermé entre-temps.",
} satisfies TranslatedArea<"connect">;
