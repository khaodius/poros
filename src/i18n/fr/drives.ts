import type { TranslatedArea } from "../catalog";

export const drives = {
  // components/DriveStatus
  filesNotUploaded: {
    one: "{count} fichier non envoyé",
    many: "{count} de fichiers non envoyés",
    other: "{count} fichiers non envoyés",
  },
  drivesMountedFromPoros: "Lecteurs montés depuis Poros",
  mountedDrives: "Lecteurs montés",
  driveAtMountPoint: "{label} sur {mountPoint}",
  readOnly: "Lecture seule",
  factAt: "Emplacement",
  factShows: "Affiche",
  factActivity: "Activité",
  uploads: "Envois",
  unmount: "Démonter",
  refusedWhileProgramsHaveFiles:
    "Refusé tant que des programmes ont des fichiers ouverts sur le lecteur ou que des envois sont en attente",
  forceUnmount: "Forcer le démontage",
  unmountsAtOnceProgramsLose:
    "Démonte immédiatement\u00a0: les programmes perdent les fichiers qu’ils ont ouverts, et ce qui n’est pas encore envoyé est conservé sur cet ordinateur",
  notUploaded: "Non envoyé",
  notUploadedKeptOnThis: "Non envoyé, conservé sur cet ordinateur",
  keptFileFacts: "{connection}, {size}",
  keptFileFactsWithError: "{connection}, {size}\u00a0: {error}",
  saveACopy: "Enregistrer une copie...",
  discardForGood: "Abandonner définitivement",

  // components/DrivesGoing
  unmountedWhenTheTabCloses: {
    one: "Le lecteur sur {paths} est démonté à la fermeture de l’onglet.",
    many: "Les lecteurs sur {paths} sont démontés à la fermeture de l’onglet.",
    other: "Les lecteurs sur {paths} sont démontés à la fermeture de l’onglet.",
  },
  unmountedWhenTheVaultLocks: {
    one: "Le lecteur sur {paths} est démonté au verrouillage du coffre.",
    many: "Les lecteurs sur {paths} sont démontés au verrouillage du coffre.",
    other: "Les lecteurs sur {paths} sont démontés au verrouillage du coffre.",
  },
  unmountedWhenPorosQuits: {
    one: "Le lecteur sur {paths} est démonté quand Poros se ferme.",
    many: "Les lecteurs sur {paths} sont démontés quand Poros se ferme.",
    other: "Les lecteurs sur {paths} sont démontés quand Poros se ferme.",
  },
  busyDrivesLoseOpenFiles:
    "Des programmes y ont encore des fichiers ouverts, ou des envois sont en attente. Ils perdent les fichiers ouverts, et ce qui n’est pas encore sur le serveur est conservé sur cet ordinateur, dans la liste sous les lecteurs de la barre d’état.",
  busyDrivesLoseOpenFilesWithVault:
    "Des programmes y ont encore des fichiers ouverts, ou des envois sont en attente. Ils perdent les fichiers ouverts, et ce qui n’est pas encore sur le serveur est conservé sur cet ordinateur, dans la liste sous les lecteurs de la barre d’état. Les fichiers conservés depuis un coffre sont stockés non chiffrés jusqu’à ce que vous les enregistriez ou les abandonniez.",

  // components/MountDialog
  mountNameAsADrive: "Monter {name} comme lecteur",
  theServersRoot: "La racine du serveur",
  theVaultsRoot: "La racine du coffre",
  rootAt: "{root}\u00a0: {path}",
  whatProgramsSaveServer:
    "Ce que les programmes enregistrent est envoyé quand ils ferment le fichier. Le lecteur est démonté à la fermeture de l’onglet ou de Poros.",
  whatProgramsSaveVault:
    "Ce que les programmes enregistrent est envoyé quand ils ferment le fichier, et conservé non chiffré sur cet ordinateur jusque-là. Le lecteur est démonté à la fermeture de l’onglet, au verrouillage du coffre ou à la fermeture de Poros.",
  mountAsDrive: "Monter comme lecteur",
  theConnectionHasClosed: "La connexion a été fermée.",
  driveLetter: "Lettre de lecteur",
  folderOnThisComputer: "Dossier sur cet ordinateur",
  aFreeLetterTheDrive: "Une lettre libre sous laquelle le lecteur apparaît.",
  anEmptyFolderMadeIf: "Un dossier vide, créé s’il n’existe pas et supprimé ensuite.",
  showOnTheDrive: "Afficher sur le lecteur",
  thisFolder: "Ce dossier",
  readOnlyProgramsCanOpen:
    "Lecture seule\u00a0: les programmes peuvent ouvrir les fichiers mais pas les modifier",
  openInFileManagerWhen: "Ouvrir dans le gestionnaire de fichiers une fois monté",
  theConnectionIsLostReconnect: "La connexion est perdue. Reconnectez-vous pour monter le lecteur.",
  mounting: "Montage...",
  mount: "Monter",

  // components/QuitDialog
  quitPoros: "Quitter Poros\u202f?",
  unmountAndQuit: "Démonter et quitter",

  // lib/mounts
  restartUnmountsDrivesInUse: {
    one: "Un lecteur a des fichiers ouverts dans des programmes ou des envois en attente. Le redémarrage le démonte, et ce qui n’est pas encore envoyé est conservé sur cet ordinateur.",
    many: "{count} de lecteurs ont des fichiers ouverts dans des programmes ou des envois en attente. Le redémarrage les démonte, et ce qui n’est pas encore envoyé est conservé sur cet ordinateur.",
    other:
      "{count} lecteurs ont des fichiers ouverts dans des programmes ou des envois en attente. Le redémarrage les démonte, et ce qui n’est pas encore envoyé est conservé sur cet ordinateur.",
  },
  restartUnmountsDrivesInUseWithVault: {
    one: "Un lecteur a des fichiers ouverts dans des programmes ou des envois en attente. Le redémarrage le démonte, et ce qui n’est pas encore envoyé est conservé sur cet ordinateur, non chiffré pour les fichiers d’un coffre.",
    many: "{count} de lecteurs ont des fichiers ouverts dans des programmes ou des envois en attente. Le redémarrage les démonte, et ce qui n’est pas encore envoyé est conservé sur cet ordinateur, non chiffré pour les fichiers d’un coffre.",
    other:
      "{count} lecteurs ont des fichiers ouverts dans des programmes ou des envois en attente. Le redémarrage les démonte, et ce qui n’est pas encore envoyé est conservé sur cet ordinateur, non chiffré pour les fichiers d’un coffre.",
  },
  driveCount: { one: "{count} lecteur", many: "{count} de lecteurs", other: "{count} lecteurs" },
  drivesWithFailedUploads: {
    one: "{drives}, {count} envoi en échec",
    many: "{drives}, {count} d’envois en échec",
    other: "{drives}, {count} envois en échec",
  },
  drivesUploading: {
    one: "{drives}, {count} envoi en cours",
    many: "{drives}, {count} d’envois en cours",
    other: "{drives}, {count} envois en cours",
  },
  filesOpen: {
    one: "{count} fichier ouvert",
    many: "{count} de fichiers ouverts",
    other: "{count} fichiers ouverts",
  },
  uploadsPending: {
    one: "{count} envoi en attente",
    many: "{count} d’envois en attente",
    other: "{count} envois en attente",
  },
  sizeRead: "{size} lus",
  sizeWritten: "{size} écrits",
  idle: "Inactif",
  uploadFailed: "Échec\u00a0: {error}",
  uploadWaiting: "En attente",
  uploading: "Envoi",
  uploadingPercent: "Envoi, {percent}\u00a0%",
  chooseADriveLetter: "Choisissez une lettre de lecteur.",
  chooseAFolder: "Choisissez un dossier.",
  driveLetterSuchAs: "Une lettre de lecteur, par exemple {example}",
  fullPathSuchAs: "Un chemin complet, par exemple {example}",

  // state/mountActions
  savedACopyAt: "Copie enregistrée dans {destination}",
} satisfies TranslatedArea<"drives">;
