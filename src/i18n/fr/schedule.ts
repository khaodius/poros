import type { TranslatedArea } from "../catalog";

export const schedule = {
  // components/SchedulerDialog
  scheduledTasks: "Tâches planifiées",
  tasks: "Tâches",
  newTask: "Nouvelle tâche",
  shownInTheListOf: "Affiché dans la liste des tâches",
  turnedOn: "Activée",
  task: "Tâche",
  savedConnection: "Connexion enregistrée",
  ifPorosIsClosedAt: "Si Poros est fermé à ce moment-là, exécuter au prochain démarrage",
  nextRun: "Prochaine exécution",
  runningNow: "En cours d’exécution",
  lastRun: "Dernière exécution",
  chooseTheLocalFolder: "Choisir le dossier local",
  localFolder: "Dossier local",
  theFolderOnThisComputer: "Le dossier sur cet ordinateur",
  serverFolder: "Dossier du serveur",
  direction: "Sens",
  conflictsAreLeftAloneAnd: "{hint} Les conflits sont laissés tels quels et signalés.",
  rsyncOptions: "Options rsync",
  typedAsOnRsyncsCommand:
    "Saisis comme sur la ligne de commande de rsync\u202f; ils s’appliquent par-dessus les options ci-dessus à chaque exécution de la tâche.",
  command: "Commande",
  inFolderOptional: "Dans le dossier (facultatif)",
  theLoginFolder: "Le dossier de connexion",
  when: "Quand",
  dateAndTime: "Date et heure",
  every: "intervalle de",
  repeatEvery: "Répéter avec un intervalle de",
  unit: "Unité",
  from: "à partir du",
  starting: "Début",
  at: "à",
  timeOfDay: "Heure",
  daysOfTheWeek: "Jours de la semaine",
  onDays: "Certains jours",
  repeat: "Répéter",
  once: "Une fois",
  synchronizeFolders: "Synchroniser des dossiers",
  transferFiles: "Transférer des fichiers",
  runACommand: "Exécuter une commande",
  uploadTo: "Envoyer vers",
  downloadFrom: "Télécharger depuis",
  copyTo: "Copier vers",
  tasksRunWhilePorosIsOpen:
    "Les tâches s’exécutent pendant que Poros est ouvert, ou depuis poros-cli avec cron ou un minuteur. Celles que vous enregistrez apparaissent ici.",
  turnedOff: "Désactivée",
  chooseAConnection: "Choisir une connexion",
  noSavedConnections: "Aucune connexion enregistrée",
  noSavedSftpConnections: "Aucune connexion SFTP enregistrée",
  aDeletedConnection: "Une connexion supprimée",
  usesSynchronizingAndRunningCommands:
    "{name} utilise {label}\u202f; la synchronisation et l’exécution de commandes nécessitent une connexion SFTP.",
  thisTasksSavedConnectionWas:
    "La connexion enregistrée de cette tâche a été supprimée. Choisissez-en une autre.",
  deleteForGood: "Supprimer définitivement",
  starting2: "Démarrage...",
  runNow: "Exécuter maintenant",
  saving: "Enregistrement...",
  thisConnectionsPasswordIsNotSaved:
    "Le mot de passe de cette connexion n’est pas enregistré dans le trousseau du système, la tâche ne peut donc pas se connecter.",
  thePasswordOfIsNot:
    "Le mot de passe de {names} n’est pas enregistré dans le trousseau du système, la tâche ne peut donc pas se connecter.",
  thePasswordsOfAreNot:
    "Les mots de passe de {names} ne sont pas enregistrés dans le trousseau du système, la tâche ne peut donc pas se connecter.",
  saveItInTheConnections: "Enregistrez-le dans les paramètres de la connexion.",
  saveThemInTheConnections: "Enregistrez-les dans les paramètres des connexions.",
  noneTheTimeHasPassed: "Aucune, l’heure est passée",

  // components/TransferTaskFields
  aFullPathOnThe: "Un chemin complet sur le serveur\u202f; terminez un dossier par /",
  aRunNowGoesInto: "Une exécution lancée maintenant irait dans {folder}.",
  addFiles: "Ajouter des fichiers",
  addFolders: "Ajouter des dossiers",
  addTheFilesAndFolders:
    "Ajoutez les fichiers et dossiers que la tâche envoie. Un dossier part avec tout son contenu.",
  addTheServerPathsThe:
    "Ajoutez les chemins du serveur que la tâche copie, par exemple /var/www/site. Un dossier part avec tout son contenu.",
  betweenServers: "Entre serveurs",
  chooseTheFolderToDownload: "Choisir le dossier où télécharger",
  copiesFilesAndFoldersFrom: "Copie des fichiers et dossiers de cet ordinateur vers le serveur.",
  copiesFilesAndFoldersFromOne:
    "Copie des fichiers et dossiers d’une connexion enregistrée vers une autre.",
  copiesFilesAndFoldersFromThe: "Copie des fichiers et dossiers du serveur vers cet ordinateur.",
  copyFrom: "Copier depuis",
  filesAndFoldersOnThe: "Fichiers et dossiers sur le serveur",
  filesAndFoldersOnThis: "Fichiers et dossiers sur cet ordinateur",
  inAFolderOfIts: "Dans un dossier dédié (facultatif)",
  subfolderExample: "sauvegarde-{date}",
  intoTheFolderOnThe: "Dans le dossier du serveur",
  intoTheFolderOnThis: "Dans le dossier de cet ordinateur",
  itemsToCopy: "Éléments à copier",
  remove: "Retirer {path}",
  removeFromTheTask: "Retirer de la tâche",
  serverPathToAdd: "Chemin du serveur à ajouter",
  theFolderToDownloadInto: "Le dossier où télécharger",
  whenAFileIsAlready: "Si un fichier existe déjà",
  eachRunCanGetAFolder:
    "Chaque exécution peut recevoir un dossier nommé d’après son heure\u00a0: {placeholders}.",

  // lib/schedule
  overwriteIfTheSourceIs: "Écraser si la source est plus récente",
  overwriteIfSizeOrDate: "Écraser si la taille ou la date diffère",
  keepBoth: "Conserver les deux",
  everyDay: "Tous les jours",
  weekdays: "En semaine",
  weekends: "Le week-end",
  onceAt: "Une fois, le {date}",
  everyMinutes: {
    one: "Toutes les minutes",
    many: "Toutes les {count} de minutes",
    other: "Toutes les {count}\u00a0minutes",
  },
  everyHours: {
    one: "Toutes les heures",
    many: "Toutes les {count} d’heures",
    other: "Toutes les {count}\u00a0heures",
  },
  everyDays: {
    one: "Tous les jours",
    many: "Tous les {count} de jours",
    other: "Tous les {count}\u00a0jours",
  },
  daysAt: "{days} à {time}",
  unitMinutes: "minutes",
  unitHours: "heures",
  unitDays: "jours",
  nameRun: "Exécuter {program}",
  nameUploadOne: "Envoyer {name}",
  nameDownloadOne: "Télécharger {name}",
  nameCopyOne: "Copier {name}",
  nameUploadItems: {
    one: "Envoyer {count} élément",
    many: "Envoyer {count} d’éléments",
    other: "Envoyer {count} éléments",
  },
  nameDownloadItems: {
    one: "Télécharger {count} élément",
    many: "Télécharger {count} d’éléments",
    other: "Télécharger {count} éléments",
  },
  nameCopyItems: {
    one: "Copier {count} élément",
    many: "Copier {count} d’éléments",
    other: "Copier {count} éléments",
  },
  nameSync: "Synchroniser {name}",
  patternTooLong: {
    one: "Le modèle de dossier dépasse {count} caractère",
    many: "Le modèle de dossier dépasse {count} de caractères",
    other: "Le modèle de dossier dépasse {count} caractères",
  },
  patternLoneBrace:
    "Le modèle de dossier contient une accolade sans son partenaire\u202f; écrivez les variables sous la forme {example}",
  patternUnknown:
    "Le modèle de dossier contient {found}, qui n’est pas une variable\u202f; utilisez {known}",
  patternBadName:
    "Le modèle de dossier donne «\u00a0{name}\u00a0», qui ne peut pas être un nom de dossier",

  // lib/scheduleDraft
  porosNoLongerKnowsThe:
    "Poros ne connaît plus le serveur de ces transferts. Planifiez plutôt les fichiers depuis un onglet.",
  filesInAnUnlockedVault:
    "Les fichiers d’un coffre déverrouillé ne peuvent pas être planifiés\u00a0: une tâche ne pourrait pas déverrouiller le coffre.",
  saveTheConnectionToFirst:
    "Enregistrez d’abord la connexion à {label}\u00a0: une tâche planifiée se connecte avec une connexion enregistrée.",
  selectTheTransfersToSchedule: "Sélectionnez les transferts à planifier.",
  selectTransfersThatGoThe:
    "Sélectionnez des transferts qui vont dans le même sens entre les mêmes emplacements pour les planifier.",
  theSelectedTransfersGoTo:
    "Les transferts sélectionnés vont dans des dossiers différents. Sélectionnez-en qui vont dans un même dossier.",
  selectTheFilesToSchedule: "Sélectionnez les fichiers à planifier.",
  blockedNoServer: "aucun serveur",
  blockedInVault: "pas dans un coffre",
  blockedUnsaved: "enregistrez d’abord la connexion",

  // state/scheduleActions
  blockedNoTab: "aucun onglet",
} satisfies TranslatedArea<"schedule">;
