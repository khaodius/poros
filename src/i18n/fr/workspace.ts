import type { TranslatedArea } from "../catalog";

export const workspace = {
  // App
  connectToServer: "Se connecter à un serveur",
  synchronizeFolders: "Synchroniser des dossiers",
  scheduledTasks: "Tâches planifiées",
  settingsCtrl: "Paramètres (Ctrl+,)",

  // components/BottomPanel
  queue: "File d’attente",
  failed: "Échecs",
  completed: "Terminés",
  startQueue: "Démarrer la file",
  pauseQueue: "Suspendre la file",
  pauseSelected: "Suspendre la sélection",
  resumeSelected: "Reprendre la sélection",
  moveSelectedToTheTop: "Placer la sélection en haut",
  moveSelectedToTheBottom: "Placer la sélection en bas",
  removeSelected: "Retirer la sélection",
  retryAll: "Tout réessayer",
  retrySelected: "Réessayer la sélection",
  clearFailedTransfers: "Effacer les transferts en échec",
  keepEachTransferInView: "Garder chaque transfert visible quand il se termine",
  follow: "Suivre",
  clearFinishedTransfers: "Effacer les transferts terminés",
  simultaneousTransfers: "Transferts simultanés",
  workers: "Simultanés",
  log: "Journal",

  // components/CloseTabDialog
  close: "Fermer {label}\u202f?",
  transfersIntoOrOutOf:
    "Les transferts vers ou depuis un coffre déverrouillé sont suspendus dans tous les cas\u00a0: fermer l’onglet verrouille le coffre. Reprenez-les une fois le coffre de nouveau déverrouillé.",
  pauseTransfersAndClose: "Suspendre les transferts et fermer",
  keepTransferring: "Continuer les transferts",
  transfersForThisServerStillQueued: {
    one: "{count} transfert pour ce serveur est encore dans la file d’attente. Il peut continuer sur sa propre connexion après la fermeture de l’onglet, ou attendre en pause jusqu’à ce que vous le repreniez.",
    many: "{count} de transferts pour ce serveur sont encore dans la file d’attente. Ils peuvent continuer sur leurs propres connexions après la fermeture de l’onglet, ou attendre en pause jusqu’à ce que vous les repreniez.",
    other:
      "{count} transferts pour ce serveur sont encore dans la file d’attente. Ils peuvent continuer sur leurs propres connexions après la fermeture de l’onglet, ou attendre en pause jusqu’à ce que vous les repreniez.",
  },
  unmountAndClose: "Démonter et fermer",

  // components/DockArea
  dragToResizeDoubleClick: "Faites glisser pour redimensionner, double-cliquez pour égaliser",
  moveToMainWindow: "Déplacer vers la fenêtre principale",
  newWindow: "Nouvelle fenêtre",
  moveToThatWindow: "Déplacer vers cette fenêtre",

  // components/DragGhost
  uploadTo: "Envoyer vers {folder}",
  downloadTo: "Télécharger vers {folder}",
  copyTo: "Copier vers {folder}",
  copyTo2: "Copier vers {baseName}",
  moveTo: "Déplacer vers {baseName}",

  // components/DropMarker
  splitRight: "Diviser à droite",
  splitDown: "Diviser en bas",
  mergeAsTab: "Fusionner en onglet",
  splitLeft: "Diviser à gauche",
  splitUp: "Diviser en haut",

  // components/RestoringPane
  signIn: "Se connecter",
  closeTab: "Fermer l’onglet",
  reopensNameOnceConnected: "Rouvre {name} une fois la connexion établie.",
  opensANewShellOnceConnected: "Ouvre un nouveau shell une fois la connexion établie.",
  reopensOnceConnected: "Se rouvre une fois la connexion établie.",
  openingName: "Ouverture de {name}...",
  openingFile: "Ouverture du fichier...",
  reconnectingTo: "Reconnexion à {label}...",
  signInTo: "Se connecter à {label}",
  couldNotReconnectTo: "Impossible de se reconnecter à {label}",
  porosKeepsNoPasswordForThisServer: "Poros ne conserve aucun mot de passe pour ce serveur.",

  // components/SplashScreen
  loadingPoros: "Chargement de Poros",

  // components/StatusBar
  connectionToLost: "Connexion à {label} perdue",
  connectedTo: "Connecté à {label}",
  serversConnected: {
    one: "{count} serveur connecté",
    many: "{count} de serveurs connectés",
    other: "{count} serveurs connectés",
  },
  notConnected: "Non connecté",
  updateTo: "Mettre à jour vers {version}",
  uploadSpeed: "Vitesse d’envoi",
  downloadSpeed: "Vitesse de téléchargement",
  appVersion: "Poros {version}",
  transfersLeft: {
    one: "{count} transfert restant",
    many: "{count} de transferts restants",
    other: "{count} transferts restants",
  },
  aboutDuration: "environ {duration}",
  queueStatusPaused: "{status} (en pause)",

  // components/TabBar
  openTerminal: "Ouvrir un terminal",
  moveToNewWindow: "Déplacer vers une nouvelle fenêtre",
  closeOtherTabs: "Fermer les autres onglets",
  newTabCtrlT: "Nouvel onglet (Ctrl+T)",
  newTab: "Nouvel onglet",
  disconnected: "Déconnecté",
  unsavedChanges: "Modifications non enregistrées",
  close2: "Fermer {label}",
  closeCtrlW: "Fermer (Ctrl+W)",
  mountAsDrive: "Monter comme lecteur...",
  mountedAsADriveAt: "Monté comme lecteur sur {paths}",
  mountedAsADrive: "Monté comme lecteur",

  // components/TabContent
  local: "Local",
  thisSessionHasEnded: "Cette session est terminée",
  connectionLost: "Connexion perdue",
  reconnectingInS: "Reconnexion dans {secondsLeft}\u00a0s",
  reconnecting: "Reconnexion...",
  reconnectingInSecondsAttempt: "Reconnexion dans {seconds}\u00a0s (tentative {attempt})",
  reconnectingAttempt: "Reconnexion... (tentative {attempt})",
  reconnectNow: "Se reconnecter maintenant",
  reconnect: "Se reconnecter",

  // components/WelcomePanel
  localFiles: "Fichiers locaux",
  import: "Importer",
  importFromFilezillaWinscpAnd: "Importer depuis FileZilla, WinSCP et d’autres applications",
  savedConnections: "Connexions enregistrées",
  serversYouSaveFromThe:
    "Les serveurs que vous enregistrez depuis la boîte de dialogue de connexion, ou avec le bouton de favori d’un onglet de serveur, apparaissent ici. Les sites enregistrés dans FileZilla, WinSCP, PuTTY et d’autres applications peuvent être importés.",
  connecting: "Connexion...",

  // components/WindowControls
  minimize: "Réduire",
  restore: "Restaurer",
  maximize: "Agrandir",

  // components/Workspace
  dragToResizeDoubleClickTo: "Faites glisser pour redimensionner, double-cliquez pour rétablir",

  // hooks/useTabLabel
  closedSession: "Session fermée",

  // state/restoreActions
  couldNotReopen: "Impossible de rouvrir {name}\u00a0: {message}",
  porosNoLongerKnowsHow: "Poros ne sait plus comment l’atteindre.",
} satisfies TranslatedArea<"workspace">;
