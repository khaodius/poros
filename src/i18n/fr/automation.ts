import type { TranslatedArea } from "../catalog";

export const automation = {
  // components/AutomationSettings
  whenTheTransferQueueFinishes: "Quand la file d’attente des transferts se termine",
  then: "Ensuite",
  thenHint:
    "Aussi dans la barre d’outils de la file d’attente. Tout sauf une commande est précédé d’un compte à rebours de 30\u00a0secondes, pour que vous puissiez l’arrêter.",
  onlyTheNextTime: "Seulement la prochaine fois",
  onlyTheNextTimeHint: "Revient à Ne rien faire une fois l’action effectuée.",
  command: "Commande",
  commandHint:
    "Pour Exécuter une commande\u202f; s’exécute sur cet ordinateur. {done}, {failed}, {skipped} et {bytes} contiennent les totaux.",
  aCommandOrScript: "Une commande ou un script",
  showANotification: "Afficher une notification",
  onlyWhenPorosIsIn: "Seulement quand Poros est en arrière-plan",
  playASound: "Émettre un son",
  serverCommands: "Commandes du serveur",
  listedUnderCommandsWhenYou:
    "Proposées sous Commandes quand vous cliquez avec le bouton droit dans un volet de serveur. Elles s’exécutent dans le shell du serveur, dans le dossier affiché, avec ces valeurs remplies\u00a0:",
  scheduledTasks: "Tâches planifiées",
  synchronizeFoldersOrRunCommands:
    "Synchroniser des dossiers ou exécuter des commandes à heures fixes",
  synchronizeFoldersOrRunCommandsHint:
    "Les tâches s’exécutent pendant que Poros est ouvert, en arrière-plan, par des connexions enregistrées.",
  output: "Sortie",
  openAWindowWithWhat:
    "Ouvrir une fenêtre avec ce qu’affiche la commande\u202f; désactivé, elle s’exécute sans rien afficher",
  reloadTheServersFoldersWhen: "Recharger les dossiers du serveur quand la commande se termine",
  deleteCommand: "Supprimer la commande",
  nameOfTheNewCommand: "Nom de la nouvelle commande",
  newCommand: "Nouvelle commande",

  // components/CommandOutputDialog
  earlierOutputWasDropped: "La sortie antérieure a été abandonnée.",
  theCommandPrintedNothing: "La commande n’a rien affiché.",
  running: "En cours",
  commandIn: "{command} dans {folder}",
  commandsIn: {
    one: "{count} commande dans {folder}",
    many: "{count} de commandes dans {folder}",
    other: "{count} commandes dans {folder}",
  },
  theLoginFolder: "le dossier de connexion",

  // components/RunCommandDialog
  runACommand: "Exécuter une commande",
  runACommandOnThe: "Exécuter une commande sur le serveur",
  commandToRunIn: "Commande à exécuter dans {folder}",
  refreshTheServersFoldersWhen: "Actualiser les dossiers du serveur quand elle se termine",
  selectFilesOrFoldersTo: "Sélectionnez des fichiers ou des dossiers pour remplir les variables.",
  run: "Exécuter",

  // components/WhenDoneControl
  whatHappensWhenTheQueue: "Ce qui se passe quand la file d’attente se termine",
  whenDone: "À la fin",
  whenTheQueueFinishes: "Quand la file d’attente se termine",

  // components/WhenDoneCountdown
  theTransferQueueHasFinished:
    "La file d’attente des transferts est terminée. {title} dans {seconds}.",
  seconds: {
    one: "{count}\u00a0seconde",
    many: "{count} de secondes",
    other: "{count}\u00a0secondes",
  },

  // components/commandMenu
  commands: "Commandes",
  runACommand2: "Exécuter une commande...",
  editCommands: "Modifier les commandes...",

  // lib/automation
  filesTransferredIn: {
    one: "{count} fichier transféré ({size}) en {duration}",
    many: "{count} de fichiers transférés ({size}) en {duration}",
    other: "{count} fichiers transférés ({size}) en {duration}",
  },
  doNothing: "Ne rien faire",
  closePoros: "Fermer Poros",
  lockTheComputer: "Verrouiller l’ordinateur",
  sleep: "Mettre en veille",
  hibernate: "Mettre en veille prolongée",
  logOff: "Fermer la session",
  shutDown: "Arrêter l’ordinateur",
  closingPoros: "Fermeture de Poros",
  lockingTheComputer: "Verrouillage de l’ordinateur",
  puttingTheComputerToSleep: "Mise en veille de l’ordinateur",
  hibernatingTheComputer: "Mise en veille prolongée de l’ordinateur",
  loggingOff: "Fermeture de la session",
  shuttingDownTheComputer: "Arrêt de l’ordinateur",
  confirmCloseNow: "Fermer maintenant",
  confirmLockNow: "Verrouiller maintenant",
  confirmSleepNow: "Mettre en veille maintenant",
  confirmHibernateNow: "Mettre en veille prolongée maintenant",
  confirmLogOffNow: "Fermer la session maintenant",
  confirmShutDownNow: "Arrêter maintenant",
  transfersFinishedWithErrors: "Transferts terminés avec des erreurs",
  transfersFinished: "Transferts terminés",
  failedCount: "échecs\u00a0: {count}",
  skippedCount: "ignorés\u00a0: {count}",

  // lib/commands
  eachSelectedItemsFullPath:
    "le chemin complet de chaque élément sélectionné, une exécution par élément",
  eachSelectedItemsNameRunning: "le nom de chaque élément sélectionné, une exécution par élément",
  theFullPathsOfAll: "les chemins complets de tous les éléments sélectionnés",
  theNamesOfAllSelected: "les noms de tous les éléments sélectionnés",
  theFolderShown: "le dossier affiché",
  stoppedAfter: "Arrêtée après {elapsed}",
  endedBySignal: "Terminée par le signal {signal} après {elapsed}",
  endedWithoutStatus: "Terminée sans code de sortie après {elapsed}",
  exitedWithStatus: "Terminée avec le code {status} après {elapsed}",
  finishedIn: "Terminée en {elapsed}",

  // state/commandStore
  stopped: "Arrêtée",
  nothingToRun: "Rien à exécuter",
  allCommandsFinished: "Les {finished} commandes sont terminées",
  ofCommandsFailed: "Commandes en échec\u00a0: {failed} sur {finished}",

  // state/whenDone
  whenTheQueueFinished: "À la fin de la file d’attente",
  theCommandAfterTheQueue: "La commande lancée après la fin de la file d’attente",
} satisfies TranslatedArea<"automation">;
