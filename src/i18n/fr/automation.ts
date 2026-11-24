import type { TranslatedArea } from "../catalog";

export const automation = {
  // components/AutomationSettings
  whenTheTransferQueueFinishes: "Quand la file d’attente des transferts se termine",
  then: "Ensuite",
  thenHint:
    "Aussi dans la barre d’outils de la file d’attente. Tout sauf une commande est précédé d’un compte à rebours de 30\u00a0secondes, pour que vous puissiez l’arrêter.",
  onlyTheNextTime: "Seulement la prochaine fois",
  onlyTheNextTimeHint: "Revient à «\u00a0Ne rien faire\u00a0» une fois l’action effectuée.",
  command: "Commande",
  commandHint:
    "Pour «\u00a0Exécuter une commande\u00a0», la commande s’exécute sur cet ordinateur. {done}, {failed}, {skipped} et {bytes} contiennent les totaux.",
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
  earlierOutputWasDropped: "Le début de la sortie n’a pas été conservé.",
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
  queueFinishedClosingPorosIn:
    "La file d’attente des transferts est terminée. Fermeture de Poros dans {seconds}.",
  queueFinishedLockingIn:
    "La file d’attente des transferts est terminée. Verrouillage de l’ordinateur dans {seconds}.",
  queueFinishedSleepingIn:
    "La file d’attente des transferts est terminée. Mise en veille de l’ordinateur dans {seconds}.",
  queueFinishedHibernatingIn:
    "La file d’attente des transferts est terminée. Mise en veille prolongée de l’ordinateur dans {seconds}.",
  queueFinishedLoggingOffIn:
    "La file d’attente des transferts est terminée. Fermeture de la session dans {seconds}.",
  queueFinishedShuttingDownIn:
    "La file d’attente des transferts est terminée. Arrêt de l’ordinateur dans {seconds}.",
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
  failedCount: { one: "{count} en échec", many: "{count} en échec", other: "{count} en échec" },
  skippedCount: { one: "{count} ignoré", many: "{count} ignorés", other: "{count} ignorés" },

  // lib/commands
  eachSelectedItemsFullPath:
    "le chemin complet de chaque élément sélectionné, une exécution par élément",
  eachSelectedItemsNameRunning: "le nom de chaque élément sélectionné, une exécution par élément",
  theFullPathsOfAll: "les chemins complets de tous les éléments sélectionnés",
  theNamesOfAllSelected: "les noms de tous les éléments sélectionnés",
  theFolderShown: "le dossier affiché",
  keepPlaceholdersOutOfAnotherShell:
    "Ne placez pas de variables dans une chaîne de commande lue par un autre shell, comme {bashExample} ou {sshExample}\u00a0: ce shell lirait un nom comme une partie de sa commande.",
  cannotTellHowTheShellReads:
    "Poros ne peut pas savoir avec certitude comment le shell lit cette commande\u00a0: elle contient un here-document, {ansiQuoted}, ou un guillemet ou une parenthèse sans fermeture. Chaque variable est insérée sous la forme {parameter} et ainsi de suite, ce qui ne garde un nom en un seul mot qu’en dehors des guillemets.",
  commandWithParameters: "{command} avec {parameters}",
  stoppedAfter: "Arrêtée après {elapsed}",
  endedBySignal: "Terminée par le signal {signal} après {elapsed}",
  endedWithoutStatus: "Terminée sans code de sortie après {elapsed}",
  exitedWithStatus: "Terminée avec le code {status} après {elapsed}",
  finishedIn: "Terminée en {elapsed}",

  // state/commandStore
  stopped: "Arrêtée",
  nothingToRun: "Rien à exécuter",
  allCommandsFinished: {
    one: "{count} commande terminée",
    many: "Les {count} de commandes sont terminées",
    other: "Les {count} commandes sont terminées",
  },
  ofCommandsFailed: {
    one: "{count} commande sur {finished} a échoué",
    many: "{count} de commandes sur {finished} ont échoué",
    other: "{count} commandes sur {finished} ont échoué",
  },

  // state/whenDone
  whenTheQueueFinished: "À la fin de la file d’attente",
  theCommandAfterTheQueue: "La commande lancée après la fin de la file d’attente",
} satisfies TranslatedArea<"automation">;
