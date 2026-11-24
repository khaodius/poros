import type { TranslatedArea } from "../catalog";

export const settings = {
  // components/LanguagePicker
  languageSystem: "Système ({language})",

  // components/SettingsDialog
  settingsSections: "Sections des paramètres",
  queue: "File d’attente",
  simultaneousTransfers: "Transferts simultanés",
  simultaneousTransfersHint: "Nombre de fichiers transférés en même temps.",
  startTransfersAsSoonAs: "Démarrer les transferts dès leur mise en file d’attente",
  startTransfersAsSoonAsHint:
    "Désactivé\u00a0: la file attend que vous cliquiez sur Démarrer la file.",
  whenTheTargetFileExists: "Si le fichier cible existe",
  keepFinishedTransfersInThe: "Garder les transferts terminés dans la liste",
  keepUnfinishedTransfersWhenPoros: "Garder les transferts inachevés à la fermeture de Poros",
  keepUnfinishedTransfersWhenPorosHint:
    "Ils reviennent en pause au prochain démarrage. Les mots de passe ne sont jamais enregistrés avec eux.",
  connections: "Connexions",
  giveEachTransferItsOwn: "Donner à chaque transfert sa propre connexion",
  giveEachTransferItsOwnHint:
    "Désactivé\u00a0: les transferts partagent la connexion de navigation, pour les serveurs qui limitent le nombre de connexions.",
  splitLargeFilesAcrossConnections: "Répartir les gros fichiers sur plusieurs connexions",
  splitLargeFilesAcrossConnectionsHint:
    "Les emplacements de transfert libres rejoignent un gros fichier et en transfèrent différentes parties.",
  splitFilesLargerThan: "Répartir les fichiers de plus de",
  connectionsPerFile: "Connexions par fichier",
  copyDirectlyBetweenFtpServers: "Copier directement entre serveurs FTP (FXP)",
  copyDirectlyBetweenFtpServersHint:
    "Les deux serveurs doivent l’autoriser. Sinon, et entre d’autres serveurs, les fichiers transitent par Poros.",
  bandwidth: "Bande passante",
  uploadLimit: "Limite d’envoi",
  uploadLimitHint: "0 signifie illimité. Partagée par tous les transferts.",
  downloadLimit: "Limite de téléchargement",
  buffers: "Tampons",
  requestSize: "Taille des requêtes",
  requestSizeHint:
    "Octets par lecture ou écriture SFTP. La plupart des serveurs acceptent jusqu’à 255\u00a0KiB\u202f; certains plafonnent à 32.",
  requestsInFlight: "Requêtes en cours",
  requestsInFlightHint:
    "Requêtes en attente de réponse par connexion. Une valeur plus élevée masque davantage la latence du réseau.",
  files: "Fichiers",
  keepModificationTimes: "Conserver les dates de modification",
  keepPermissions: "Conserver les permissions",
  keepPermissionsHint:
    "Copie les bits de permission Unix. Sans effet pour les fichiers provenant de Windows.",
  integrity: "Intégrité",
  writeToATemporaryName: "Écrire sous un nom temporaire, puis renommer",
  writeToATemporaryNameHint:
    "Un fichier n’apparaît sous son nom qu’une fois complet et vérifié, et un fichier remplacé n’est jamais laissé à moitié écrit. Les envois vers les serveurs FTP sont écrits directement à leur place.",
  verifyChecksums: "Vérifier les sommes de contrôle",
  verifyChecksumsHint:
    "Compare un SHA-256 des deux copies après chaque fichier, calculé sur les serveurs SFTP par leur propre sha256sum. SHA-1 et MD5 ne servent jamais à cela, car ils peuvent être falsifiés. Relit chaque fichier une fois de plus des deux côtés.",
  readFilesBackWhenThe: "Relire les fichiers quand le serveur ne peut pas calculer leur hachage",
  readFilesBackWhenTheHint:
    "Sur les serveurs SFTP sans sha256sum, et sur les serveurs FTP et WebDAV, chaque fichier terminé est relu et haché ici. Double le trafic avec ces serveurs. Le stockage cloud n’est pas relu.",
  repairOnlyTheDamagedParts: "Réparer uniquement les parties endommagées",
  repairOnlyTheDamagedPartsHint:
    "Après une différence, chaque partie du fichier est comparée séparément et seules les parties qui diffèrent sont renvoyées, jusqu’à la moitié du fichier. Nécessite GNU split sur le serveur, ou la relecture.",
  checkTheTransferredPartBefore: "Vérifier la partie déjà transférée avant de reprendre",
  checkTheTransferredPartBeforeHint:
    "Compare ce qui est déjà présent avec la source, partie par partie, et reprend à la première partie qui diffère. Quand le serveur ne peut pas hacher des parties d’un fichier, les derniers 64\u00a0KiB sont comparés.",
  flushToDiskBeforeFinishing: "Forcer l’écriture sur le disque avant de terminer",
  flushToDiskBeforeFinishingHint:
    "Un fichier ne compte comme terminé qu’une fois stocké, pour qu’une coupure de courant juste après ne puisse pas le perdre. Sur les serveurs qui le permettent\u202f; ralentit les transferts de nombreux petits fichiers.",
  checkTimesAndPermissionsAfter: "Vérifier les dates et permissions après les avoir appliquées",
  checkTimesAndPermissionsAfterHint:
    "Les relit une fois le fichier en place et note dans le journal celles que le serveur ou le système de fichiers n’a pas conservées.",
  checkPublishedChecksumsAndSignatures: "Vérifier les sommes de contrôle et signatures publiées",
  checkPublishedChecksumsAndSignaturesHint:
    "Les téléchargements sont comparés à SHA256SUMS, name.sha256 et aux fichiers semblables placés à côté, avec les signatures .minisig, et avec les signatures .asc et .sig par gpg. Un fichier qui échoue n’est pas enregistré.",
  errors: "Erreurs",
  retries: "Nouvelles tentatives",
  retriesHint:
    "Pour les délais dépassés, les vérifications échouées et les connexions qui retombent aussitôt.",
  waitBetweenRetries: "Attente entre les tentatives",
  trustedMinisignKeys: "Clés minisign de confiance",
  onePublicKeyPerLine:
    "Une clé publique par ligne, comme dans le minisign.pub d’un éditeur. Les signatures d’autres clés sont notées dans le journal et ne sont pas vérifiées.",
  autoTuneReceiveBuffer: "Ajuster automatiquement le tampon de réception",
  autoTuneReceiveBufferHint:
    "Le système agrandit la fenêtre de réception TCP selon la connexion. S’applique aux nouvelles connexions.",
  receiveBufferSize: "Taille du tampon de réception",
  receiveBufferSizeHint: "SO_RCVBUF, utilisé quand l’ajustement automatique est désactivé.",
  autoTuneSendBuffer: "Ajuster automatiquement le tampon d’envoi",
  autoTuneSendBufferHint:
    "Le système dimensionne le tampon d’envoi TCP selon la connexion. S’applique aux nouvelles connexions.",
  sendBufferSize: "Taille du tampon d’envoi",
  sendBufferSizeHint: "SO_SNDBUF, utilisé quand l’ajustement automatique est désactivé.",
  connectionTimeout: "Délai de connexion",
  keepaliveInterval: "Intervalle de maintien de connexion",
  keepaliveIntervalHint:
    "0 désactive le maintien de connexion. S’applique aux nouvelles connexions.",
  compressTraffic: "Compresser le trafic",
  compressTrafficHint:
    "Aide pour le texte sur les liaisons lentes\u202f; ralentit les fichiers déjà compressés. S’applique aux nouvelles connexions.",
  reconnecting: "Reconnexion",
  reconnectLostTabsAutomatically: "Reconnecter automatiquement les onglets perdus",
  reconnectLostTabsAutomaticallyHint:
    "Réessaie à intervalles croissants, sans redemander les mots de passe déjà fournis.",
  keepTryingFor: "Continuer d’essayer pendant",
  keepTryingForHint:
    "Les onglets et les transferts attendent aussi longtemps un serveur perdu\u202f; les transferts gardent entre-temps leurs nouvelles tentatives. 0 abandonne aussitôt.",
  savedConnections: "Connexions enregistrées",
  saveQuickConnectionsAutomatically: "Enregistrer automatiquement les connexions rapides",
  saveQuickConnectionsAutomaticallyHint:
    "Ajoute chaque nouveau serveur atteint depuis la barre de connexion rapide, sans son mot de passe.",
  proxy: "Proxy",
  newSftpConnectionsReachTheir:
    "Les nouvelles connexions SFTP atteignent leur serveur par ce proxy, sauf si une connexion est réglée pour s’en passer. Avec un hôte de rebond, seule la connexion à l’hôte de rebond passe par le proxy. Les connexions FTP et cloud ne l’utilisent pas.",
  type: "Type",
  host: "Hôte",
  port: "Port",
  username: "Nom d’utilisateur",
  usernameHint:
    "Laissez vide pour un proxy sans identifiant. SOCKS4 n’envoie que le nom d’utilisateur.",
  password: "Mot de passe",
  passwordHint: "Conservé dans le trousseau du système.",
  forget: "Oublier",
  lookUpServerNamesThrough: "Résoudre les noms des serveurs par le proxy",
  lookUpServerNamesThroughHint:
    "Le DNS de cet ordinateur ne voit jamais les noms des serveurs auxquels vous vous connectez. Les proxys HTTP le font toujours.",
  proxyPassword: "Mot de passe du proxy",
  saved: "Enregistré",
  doubleClickingAFile: "Double-clic sur un fichier",
  transfersItToTheOther: "Le transfère de l’autre côté",
  opensItInTheBuilt: "L’ouvre dans l’éditeur intégré",
  opensItInItsProgram: "L’ouvre dans son programme",
  asksHowToOpenIt: "Demande comment l’ouvrir",
  doesNothing: "Ne fait rien",
  showHiddenFilesInNew: "Afficher les fichiers cachés dans les nouveaux onglets",
  foldersFirst: "Dossiers en premier",
  foldersFirstHint: "Garde les dossiers au-dessus des fichiers quel que soit l’ordre de tri.",
  dates: "Dates",
  columns: "Colonnes",
  columnsHint:
    "Cliquez avec le bouton droit sur un en-tête de colonne pour afficher ou masquer des colonnes et changer le tri. Faites glisser la ligne avant un en-tête pour redimensionner sa colonne, ou double-cliquez sur la ligne pour l’ajuster.",
  serverFolders: "Dossiers des serveurs",
  showCachedListingsWhileRefreshing: "Afficher le contenu en cache pendant l’actualisation",
  showCachedListingsWhileRefreshingHint:
    "Un dossier déjà ouvert s’affiche aussitôt tel qu’il a été vu la dernière fois, puis se met à jour quand le serveur répond. Le contenu des dossiers est gardé en mémoire uniquement, jamais sur le disque.",
  loadNearbyFoldersAhead: "Charger à l’avance les dossiers voisins",
  loadNearbyFoldersAheadHint:
    "Liste le dossier parent et le dossier sous le curseur pendant que le serveur est inactif, un à la fois. En pause sur FTP pendant les transferts.",
  alsoOnCloudStorage: "Aussi sur le stockage cloud",
  alsoOnCloudStorageHint:
    "Désactivé\u00a0: rien n’est listé à l’avance sur le stockage cloud, où les requêtes sont comptées et limitées.",
  tabsAndWindows: "Onglets et fenêtres",
  askBeforeClosingATab: "Demander avant de fermer un onglet qui a des transferts en attente",
  rememberTabsAndLayout: "Mémoriser les onglets et la disposition",
  rememberTabsAndLayoutHint:
    "Rouvre les fenêtres, volets et onglets ouverts à la dernière fermeture, chacun dans son dossier.",
  reconnectToServersOnStart: "Se reconnecter aux serveurs au démarrage",
  reconnectToServersOnStartHint:
    "Désactivé\u00a0: les onglets de serveur rouverts attendent que vous les connectiez.",
  useTheSystemTitleBar: "Utiliser la barre de titre du système",
  useTheSystemTitleBarHint:
    "Désactivé\u00a0: la barre du haut porte les boutons de la fenêtre et sert à la déplacer.",
  log: "Journal",
  linesToKeep: "Lignes à conserver",
  showTimes: "Afficher les heures",
  wrapLongLines: "Retour automatique à la ligne",
  logEveryTransferredFile: "Journaliser chaque fichier transféré",
  logEveryTransferredFileHint:
    "Désactivé\u00a0: une seule ligne de résumé quand la file d’attente se termine.",
  updates: "Mises à jour",
  checkForUpdatesWhenPoros: "Rechercher des mises à jour au démarrage de Poros",
  checkForUpdatesWhenPorosHint:
    "Cherche une version plus récente sur GitHub. Rien n’est téléchargé tant que vous ne choisissez pas de l’installer.",
  poros: "Poros {version}",
  restartNow: "Redémarrer maintenant",
  updating: "Mise à jour...",
  showUpdate: "Afficher la mise à jour",
  isNotAvailable: "{version} n’est pas disponible\u00a0: {message}",
  installAnotherVersion: "Installer une autre version",
  version: "Version",
  install: "Installer",
  transfers: "Transferts",
  sync: "Synchronisation",
  connection: "Connexion",
  cloudAccounts: "Comptes cloud",
  automation: "Automatisation",
  archives: "Archives",
  interface: "Interface",
  appearance: "Apparence",
  overwriteIfTheSourceIs: "Écraser si la source est plus récente",
  overwriteIfSizeOrDate: "Écraser si la taille ou la date diffère",
  keepBoth: "Conserver les deux",
  checking: "Vérification...",
  checkForUpdates: "Rechercher des mises à jour",
  listing: "Chargement...",
  showVersions: "Afficher les versions",
  language: "Langue",
  languageHint:
    "«\u00a0Système\u00a0» suit la langue de l’ordinateur. Les messages des serveurs et le journal restent en anglais.",

  // components/Stepper
  decrease: "Diminuer «\u00a0{label}\u00a0»",
  increase: "Augmenter «\u00a0{label}\u00a0»",
} satisfies TranslatedArea<"settings">;
