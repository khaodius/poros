import type { TranslatedArea } from "../catalog";

export const transfers = {
  // components/TransferLookSettings
  timeLeft: "Reste {duration}",

  // components/TransferView
  copySourcePaths: "Copier les chemins source",
  copySourcePath: "Copier le chemin source",
  moveToTop: "Placer en haut",
  moveToBottom: "Placer en bas",
  progress: "Progression",
  speed: "Vitesse",
  status: "État",
  dragFilesBetweenTabsOr:
    "Faites glisser des fichiers entre les onglets, ou utilisez Envoyer et Télécharger dans le menu d’un fichier.",
  transfersThatFailAfterTheir:
    "Les transferts qui échouent après toutes leurs tentatives arrivent ici.",
  finishedTransfersAreListedHere: "Les transferts terminés sont listés ici.",
  done: "Terminé",
  scheduleThese: "Planifier ceux-ci...",
  scheduleThis: "Planifier celui-ci...",
  connectionLostWaitingForTheServer: "Connexion perdue, en attente du serveur",
  waitingToRetry: "Attente avant un nouvel essai (tentative {attempt})",
  queued: "En attente",
  listingFolder: "Lecture du dossier",
  updatingWithRsyncSent: "Mise à jour avec rsync, {size} envoyés",
  copyingDirectlyBetweenTheServers: "Copie directe entre les serveurs",
  transferringOnConnections: {
    one: "Transfert sur {count} connexion",
    many: "Transfert sur {count} de connexions",
    other: "Transfert sur {count} connexions",
  },
  transferring: "Transfert",
  paused: "En pause",
  alreadyExistsWaitingForYourChoice: "Existe déjà, en attente de votre choix",
  doneCopiedDirectlyBetweenTheServers: "Terminé, copié directement entre les serveurs",
  doneWithRsyncSent: "Terminé avec rsync, {sent} envoyés sur {size}",
  skippedBecause: "Ignoré\u00a0: {error}",
  skipped: "Ignoré",
  failedBecause: "Échec\u00a0: {error}",
  failedUnknownError: "Échec\u00a0: erreur inconnue",
  rowTitle: "{source}\nvers {target}",
  rowTitleQueuedByTask: "{source}\nvers {target}\nMis en file d’attente par la tâche {task}",

  // lib/progressBars
  sizeOfSize: "{transferred} sur {size}",

  // state/transferActions
  nothingToTransferOnlyRegular:
    "Rien à transférer\u00a0: seuls les fichiers ordinaires et les dossiers peuvent être copiés.",
  unlockTheVaultToCopy: "Déverrouillez le coffre pour y copier des fichiers.",
  couldNotCheckWhetherThis: "Impossible de vérifier si ce dossier est un coffre.",
  isALockedVaultOpen: "{baseName} est un coffre verrouillé. Ouvrez-le pour y copier des fichiers.",
  dropLocalFilesOnA: "Déposez des fichiers locaux sur un onglet de serveur pour les envoyer.",
  copyingTo: "Copie vers {folder}",
  movedTo: "Déplacé vers {folder}",
  copiedTo: "Copié vers {folder}",
  connectToAServerTo: "Connectez-vous à un serveur pour y envoyer des fichiers.",
  openALocalTabTo: "Ouvrez un onglet local où télécharger.",
  dropFilesOnAServer: "Déposez des fichiers sur un onglet de serveur pour les envoyer.",
  local: "Local",
} satisfies TranslatedArea<"transfers">;
