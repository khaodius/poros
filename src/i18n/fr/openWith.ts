import type { TranslatedArea } from "../catalog";

export const openWith = {
  // components/ExternalEditDialogs
  isGoneFromTheServer: "{name} n’est plus sur le serveur",
  changedOnTheServer: "{name} a été modifié sur le serveur",
  modified: "Modifié",
  onTheServer: "Sur le serveur",
  yourSavedCopy: "Votre copie enregistrée",
  keepBothUploadsYourCopy:
    "«\u00a0Conserver les deux\u00a0» envoie votre copie à côté de celle du serveur, sous un nom qui n’y est pas pris.",
  comparingReadsTheServersCopy: "La comparaison lit la copie du serveur par SFTP",
  dontUpload: "Ne pas envoyer",
  uploadItAgain: "L’envoyer de nouveau",
  keepBoth: "Conserver les deux",
  upload: "Envoyer {name}\u202f?",
  uploadEverySaveWithoutAsking: "Envoyer chaque enregistrement sans demander",
  notNow: "Pas maintenant",
  defaultProgram: "Programme par défaut",
  setUpForFilesLike: "Configuré pour les fichiers de ce type",
  builtInEditor: "Éditeur intégré",
  editTheTextInA: "Modifier le texte dans un onglet ici",
  chooseAProgram: "Choisir un programme...",
  anyProgramOnThisComputer: "N’importe quel programme de cet ordinateur",
  isAProgram: "{name} est un programme",
  open: "Ouvrir {name}",
  programs: "Programmes",
  filesNotUploadedLastTime: "Fichiers non envoyés la dernière fois",
  later: "Plus tard",
  lock: "Verrouiller {label}\u202f?",
  close: "Fermer l’onglet {label}\u202f?",
  theirSavesKeepBeingUploaded:
    "Leurs enregistrements continuent d’être envoyés après la fermeture de l’onglet.",
  stopEditingAndClose: "Arrêter la modification et fermer",
  uploadSavedFilesBeforeQuitting: "Envoyer les fichiers enregistrés avant de quitter\u202f?",
  someCouldNotBeUploaded: "Certains n’ont pas pu être envoyés.",
  quitWithoutUploading: "Quitter sans envoyer",
  deleted: "Supprimé",
  compare: "Comparer",
  compareSftpOnly: "Comparer (SFTP uniquement)",
  lockAnyway: "Verrouiller quand même",
  closeAnyway: "Fermer quand même",
  uploading2: "Envoi...",
  uploadAndLock: "Envoyer et verrouiller",
  uploadAndClose: "Envoyer et fermer",
  keepEditing: "Continuer la modification",
  uploadAndQuit: "Envoyer et quitter",
  uploadedAs: "{name} envoyé sous le nom {renamedTo}",
  uploadedName: "{name} envoyé",
  couldNotUploadKept:
    "Impossible d’envoyer {name}\u00a0: {message}. Le fichier enregistré est conservé\u202f; réessayez depuis la barre d’état.",
  couldNotOpen: "Impossible d’ouvrir {name}\u00a0: {message}",
  someoneDeletedIt:
    "Quelqu’un l’a supprimé de {server} depuis que vous l’avez ouvert. Votre enregistrement n’a pas encore été envoyé.",
  someoneChangedIt:
    "Quelqu’un l’a modifié sur {server} depuis que vous l’avez ouvert. Votre enregistrement n’a pas encore été envoyé.",
  serverChangedThere: "{server}, modifié là-bas",
  savedInProgramUploadNow:
    "{name} a été enregistré dans {program}. L’envoyer maintenant vers {server}\u202f?",
  savedUploadNow: "{name} a été enregistré. L’envoyer maintenant vers {server}\u202f?",
  theProgramThisSystemOpensFilesWith:
    "Le programme avec lequel ce système ouvre les fichiers {extension}",
  theProgramThisSystemOpensSuchFilesWith:
    "Le programme avec lequel ce système ouvre ce type de fichiers",
  holdsAProgramNotOpened:
    "{name} contient un programme ou un script, il n’a donc pas été ouvert. Poros n’exécute pas les fichiers qu’il ouvre\u00a0: choisissez un éditeur pour l’examiner.",
  isAProgramOrAScript:
    "{name} est un programme ou un script. Poros n’exécute pas les fichiers qu’il ouvre\u00a0: choisissez un éditeur pour l’examiner.",
  whenPorosLastClosed: {
    one: "À la dernière fermeture de Poros, ce fichier avait été enregistré dans un autre programme mais pas envoyé. Envoyez-le une fois son serveur connecté, ou abandonnez les modifications. La barre d’état garde la liste.",
    many: "À la dernière fermeture de Poros, ces fichiers avaient été enregistrés dans un autre programme mais pas envoyés. Envoyez-les une fois leur serveur connecté, ou abandonnez les modifications. La barre d’état garde la liste.",
    other:
      "À la dernière fermeture de Poros, ces fichiers avaient été enregistrés dans un autre programme mais pas envoyés. Envoyez-les une fois leur serveur connecté, ou abandonnez les modifications. La barre d’état garde la liste.",
  },
  copiesFromVaultsStayUnencrypted:
    "Les copies des fichiers de coffres restent non chiffrées sur cet ordinateur jusque-là.",
  filesFromThisVaultOpen: {
    one: "{count} fichier de ce coffre est ouvert dans d’autres programmes\u00a0:",
    many: "{count} de fichiers de ce coffre sont ouverts dans d’autres programmes\u00a0:",
    other: "{count} fichiers de ce coffre sont ouverts dans d’autres programmes\u00a0:",
  },
  filesFromThisServerOpen: {
    one: "{count} fichier de ce serveur est ouvert dans d’autres programmes\u00a0:",
    many: "{count} de fichiers de ce serveur sont ouverts dans d’autres programmes\u00a0:",
    other: "{count} fichiers de ce serveur sont ouverts dans d’autres programmes\u00a0:",
  },
  couldNotBeUploadedLockingWipes: {
    one: "Celui-ci n’a pas pu être envoyé. Verrouiller quand même efface les modifications qu’il contient.",
    many: "Ceux-ci n’ont pas pu être envoyés. Verrouiller quand même efface les modifications qu’ils contiennent.",
    other:
      "Ceux-ci n’ont pas pu être envoyés. Verrouiller quand même efface les modifications qu’ils contiennent.",
  },
  couldNotBeUploadedClosingWipes: {
    one: "Celui-ci n’a pas pu être envoyé. Fermer quand même efface les modifications des fichiers du coffre.",
    many: "Ceux-ci n’ont pas pu être envoyés. Fermer quand même efface les modifications des fichiers du coffre.",
    other:
      "Ceux-ci n’ont pas pu être envoyés. Fermer quand même efface les modifications des fichiers du coffre.",
  },
  couldNotBeUploadedClosingLeaves: {
    one: "Celui-ci n’a pas pu être envoyé. Fermer quand même le laisse dans la liste de la barre d’état, pour l’envoyer plus tard.",
    many: "Ceux-ci n’ont pas pu être envoyés. Fermer quand même les laisse dans la liste de la barre d’état, pour les envoyer plus tard.",
    other:
      "Ceux-ci n’ont pas pu être envoyés. Fermer quand même les laisse dans la liste de la barre d’état, pour les envoyer plus tard.",
  },
  lockingEndsEditing: {
    one: "Verrouiller le coffre met fin à la modification du fichier qui en provient et efface les copies de travail. Ce qui a été enregistré est d’abord envoyé.",
    many: "Verrouiller le coffre met fin à la modification des fichiers qui en proviennent et efface les copies de travail. Ce qui a été enregistré est d’abord envoyé.",
    other:
      "Verrouiller le coffre met fin à la modification des fichiers qui en proviennent et efface les copies de travail. Ce qui a été enregistré est d’abord envoyé.",
  },
  closingLocksVaultEndsEditing: {
    one: "Fermer l’onglet verrouille son coffre, ce qui met fin à la modification du fichier qui en provient et efface les copies de travail. Ce qui a été enregistré est d’abord envoyé.",
    many: "Fermer l’onglet verrouille son coffre, ce qui met fin à la modification des fichiers qui en proviennent et efface les copies de travail. Ce qui a été enregistré est d’abord envoyé.",
    other:
      "Fermer l’onglet verrouille son coffre, ce qui met fin à la modification des fichiers qui en proviennent et efface les copies de travail. Ce qui a été enregistré est d’abord envoyé.",
  },
  savedButNotReachedTheServer: {
    one: "Ce fichier a été enregistré dans d’autres programmes mais n’a pas atteint le serveur\u00a0:",
    many: "Ces fichiers ont été enregistrés dans d’autres programmes mais n’ont pas atteint le serveur\u00a0:",
    other:
      "Ces fichiers ont été enregistrés dans d’autres programmes mais n’ont pas atteint le serveur\u00a0:",
  },
  quittingWithoutUploadingKeepsThem:
    "Quitter sans envoyer les conserve, et Poros propose de les envoyer au prochain démarrage.",
  workingCopiesOfVaultFilesAreWiped: "Les copies de travail des fichiers du coffre sont effacées.",

  // components/ExternalEditorSettings
  openWith: "Ouvrir avec",
  openCtrlEnterUsesThe:
    "Ouvrir (Ctrl+Enter) utilise le programme configuré pour le type du fichier, sinon celui du système. Un fichier du serveur s’ouvre depuis une copie sur cet ordinateur, envoyée chaque fois que le programme l’enregistre.",
  uploadEachSaveWithoutAsking: "Envoyer chaque enregistrement sans demander",
  uploadEachSaveWithoutAskingHint: "Désactivé\u00a0: demande avant chaque envoi.",
  keepCopiesAfterTheProgram: "Garder les copies après la fermeture du programme",
  keepCopiesAfterTheProgramHint:
    "Compté depuis le dernier enregistrement ou envoi. Les copies des fichiers des coffres sont effacées aussitôt.",
  removeProgram: "Retirer le programme",
  remove: "Retirer {programLabel}",
  program: "Programme",
  arguments: "Arguments",
  defaultFor: "Par défaut pour",
  filesThisProgramOpensBy: "Fichiers que ce programme ouvre par défaut",
  psdXcf: "*.psd, *.xcf",
  nameOfTheNewProgram: "Nom du nouveau programme",
  newProgram: "Nouveau programme",
  inTheArgumentsStandsFor:
    "{placeholder} dans les arguments représente le fichier, et le fichier vient en dernier s’il est omis. Les guillemets gardent les espaces dans un même argument. Les fichiers qui correspondent à un motif sous «\u00a0Par défaut pour\u00a0» s’ouvrent dans ce programme.",

  // components/ExternalEditsStatus
  filesOpenInOtherPrograms: "Fichiers ouverts dans d’autres programmes",
  stopEditing: "Arrêter de modifier {name}\u202f?",
  stopAndDiscard: "Arrêter et abandonner",
  openInOtherPrograms: "Ouverts dans d’autres programmes",
  settingsForOpeningFiles: "Paramètres d’ouverture des fichiers",
  noFilesAreOpenIn:
    "Aucun fichier n’est ouvert dans d’autres programmes. Cliquez avec le bouton droit sur un fichier et choisissez Ouvrir avec.",
  inAVault: "Dans un coffre",
  uploadedAtTime: "envoyé à {time}",
  uploadedOnDate: "envoyé le {date}",
  of: "{transferred} sur {total}",
  decideWhatToUpload: "Choisir quoi envoyer...",
  openInAnEditor: "Ouvrir dans un éditeur...",
  uploadNow: "Envoyer maintenant",
  uploadNow2: "Envoyer {name} maintenant",
  openAgain: "Rouvrir",
  openAgain2: "Rouvrir {name}",
  showTheWorkingCopysFolder: "Afficher le dossier de la copie de travail",
  showTheFolderOf: "Afficher le dossier de {name}",
  stopEditingAndDeleteThe: "Arrêter la modification et supprimer la copie de travail",
  stopEditing2: "Arrêter de modifier {name}",
  savedButNotUploadedWhen: "Enregistrés mais non envoyés à la dernière fermeture de Poros",
  saved: "enregistré le {date}",
  thisCopyOfAVault:
    "Cette copie d’un fichier de coffre n’est pas chiffrée sur cet ordinateur. Envoyez-la ou abandonnez-la.",
  uploadTo: "Envoyer vers {label}",
  connectToFirst: "Connectez-vous d’abord à {server}",
  upload2: "Envoyer {name}",
  discardTheSavedChanges: "Abandonner les modifications enregistrées",
  discard: "Abandonner {name}",
  discard2: "Abandonner {name}\u202f?",
  notEditingAnyFiles: "Aucun fichier en cours de modification",
  eachSaveIsUploadedAs: "Chaque enregistrement est envoyé dès que le programme l’a écrit.",
  savesWaitForYouTo: "Les enregistrements attendent que vous les envoyiez.",
  changesSavedInProgramNotReached:
    "Les modifications enregistrées dans {program} n’ont pas atteint {server}. Arrêter supprime la copie de travail avec elles.",
  changesSavedInTheProgramNotReached:
    "Les modifications enregistrées dans le programme n’ont pas atteint {server}. Arrêter supprime la copie de travail avec elles.",
  connectAndUnlockToUpload: "Connectez-vous à {server} et déverrouillez son coffre pour l’envoyer.",
  connectToUpload: "Connectez-vous à {server} pour l’envoyer.",
  neverReachedDiscardingWipes:
    "Les modifications qui y ont été enregistrées n’ont jamais atteint {server}. Les abandonner les efface.",
  neverReachedDiscardingDeletes:
    "Les modifications qui y ont été enregistrées n’ont jamais atteint {server}. Les abandonner les supprime.",

  // components/openWithMenu
  openIn: "Ouvrir dans {programLabel}",
  setUpPrograms: "Configurer les programmes...",

  // lib/externalEditor
  downloading: "Téléchargement",
  watchingForSaves: "En attente d’enregistrements",
  savedNotUploaded: "Enregistré, non envoyé",
  waitingToUpload: "En attente d’envoi",
  changedOnTheServer2: "Modifié sur le serveur",
  uploading: "Envoi",
  uploadFailed: "Échec de l’envoi",
  notOpenedItIsAProgram: "Non ouvert\u00a0: c’est un programme",
  openingNamePercent: "Ouverture de {name}, {percent}",
  uploadingName: "Envoi de {name}",
  nameNotOpenedItIsAProgram: "{name}\u00a0: non ouvert, c’est un programme",
  nameChangedOnTheServer: "{name}\u00a0: modifié sur le serveur",
  nameUploadFailed: "{name}\u00a0: échec de l’envoi",
  editedFilesNeedAttention: {
    one: "{count} fichier modifié demande votre attention",
    many: "{count} de fichiers modifiés demandent votre attention",
    other: "{count} fichiers modifiés demandent votre attention",
  },
  editingName: "Modification de {name}",
  editingFiles: {
    one: "Modification de {count} fichier",
    many: "Modification de {count} de fichiers",
    other: "Modification de {count} fichiers",
  },
  filesNotUploaded: {
    one: "{count} fichier non envoyé",
    many: "{count} de fichiers non envoyés",
    other: "{count} fichiers non envoyés",
  },

  // state/externalEditStore
  isOpenAlreadyItsWorking: "{name} est déjà ouvert. Sa copie de travail se rouvre.",
  chooseAProgram2: "Choisir un programme",
} satisfies TranslatedArea<"openWith">;
