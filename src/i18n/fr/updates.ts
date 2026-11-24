import type { TranslatedArea } from "../catalog";

export const updates = {
  // components/SettingsDialog
  downloadingVersion: "Téléchargement de {version}",
  downloadingVersionPercent: "Téléchargement de {version}, {percent}",
  installingVersion: "Installation de {version}",
  checkingForUpdates: "Recherche de mises à jour",
  versionIsAvailable: "{version} est disponible",
  upToDateCheckedAt: "À jour, vérifié à {time}",
  theVersionYouAreRunning: "La version que vous utilisez",
  couldNotListTheReleases: "Impossible de lister les versions\u00a0: {message}",
  noOtherReleaseToInstall: "Aucune autre version à installer à partir de la {version}.",
  switchToAnotherRelease: "Passer à une autre version, à partir de la {version}.",
  versionIsReadyRestart: "{version} est prête. Redémarrez Poros pour l’utiliser.",
  restartingInto: "Redémarrage sur la {version}",
  couldNotInstallVersionBecause: "Impossible d’installer {version}\u00a0: {message}",

  // components/StatusBar
  restartToUse: "Redémarrer pour utiliser {version}",
  restarting: "Redémarrage",
  couldNotInstallVersion: "Impossible d’installer {version}",

  // components/UpdateDialog
  updateAvailable: "Mise à jour disponible",
  isAvailableYouHave: "Poros {version} est disponible. Vous avez la {current}.",
  porosKeepsWorkingWhileThe:
    "Poros continue de fonctionner pendant le téléchargement de la mise à jour, et demande avant de redémarrer.",
  skipThisVersion: "Ignorer cette version",
  later: "Plus tard",
  update: "Mettre à jour",

  // components/UpdateNotice
  hideTheStatusBarShows: "Masquer\u202f; la barre d’état l’affiche de nouveau",
  restartPorosToSwitchTo:
    "Redémarrez Poros pour l’utiliser maintenant, sinon elle prendra effet à la prochaine ouverture de Poros.",
  restartNow: "Redémarrer maintenant",
  of: "{received} sur {total}",
  downloadingPoros: "Téléchargement de Poros {version}",
  installingPoros: "Installation de Poros {version}",
  porosIsReady: "Poros {version} est prêt",
  restartingPoros: "Redémarrage de Poros",
  couldNotInstallPoros: "Impossible d’installer Poros {version}",

  // state/updateStore
  transfersWillStop: {
    one: "{count} transfert de la file d’attente s’arrêtera au redémarrage de Poros.",
    many: "{count} de transferts de la file d’attente s’arrêteront au redémarrage de Poros.",
    other: "{count} transferts de la file d’attente s’arrêteront au redémarrage de Poros.",
  },
  transfersWillStopAndComeBackPaused: {
    one: "{count} transfert de la file d’attente s’arrêtera au redémarrage de Poros et reviendra en pause.",
    many: "{count} de transferts de la file d’attente s’arrêteront au redémarrage de Poros et reviendront en pause.",
    other:
      "{count} transferts de la file d’attente s’arrêteront au redémarrage de Poros et reviendront en pause.",
  },
  unsavedChangesInFilesWill: {
    one: "Les modifications non enregistrées de {count} fichier seront perdues au redémarrage de Poros.",
    many: "Les modifications non enregistrées de {count} de fichiers seront perdues au redémarrage de Poros.",
    other:
      "Les modifications non enregistrées de {count} fichiers seront perdues au redémarrage de Poros.",
  },
  theLatestReleaseDoesNotOfferInAppUpdates:
    "La dernière version sur GitHub ne propose pas encore de mises à jour dans l’application",
  couldNotCheckForUpdates: "Impossible de rechercher des mises à jour\u00a0: {message}",
  heldSavesUploadedAfterTheRestart: {
    one: "Un fichier ouvert dans un autre programme contient un enregistrement pas encore envoyé. Poros propose de l’envoyer après le redémarrage.",
    many: "{count} de fichiers ouverts dans d’autres programmes contiennent des enregistrements pas encore envoyés. Poros propose de les envoyer après le redémarrage.",
    other:
      "{count} fichiers ouverts dans d’autres programmes contiennent des enregistrements pas encore envoyés. Poros propose de les envoyer après le redémarrage.",
  },
  heldSavesWipedByTheRestart: {
    one: "Un fichier ouvert dans un autre programme contient un enregistrement pas encore envoyé. Le redémarrage l’efface, car les fichiers de coffre ne sont pas conservés non chiffrés.",
    many: "{count} de fichiers ouverts dans d’autres programmes contiennent des enregistrements pas encore envoyés. Le redémarrage les efface, car les fichiers de coffre ne sont pas conservés non chiffrés.",
    other:
      "{count} fichiers ouverts dans d’autres programmes contiennent des enregistrements pas encore envoyés. Le redémarrage les efface, car les fichiers de coffre ne sont pas conservés non chiffrés.",
  },
  heldSavesPartlyWipedByTheRestart: {
    one: "Un fichier ouvert dans un autre programme contient un enregistrement pas encore envoyé. Ceux des coffres sont effacés\u202f; Poros propose d’envoyer les autres après le redémarrage.",
    many: "{count} de fichiers ouverts dans d’autres programmes contiennent des enregistrements pas encore envoyés. Ceux des coffres sont effacés\u202f; Poros propose d’envoyer les autres après le redémarrage.",
    other:
      "{count} fichiers ouverts dans d’autres programmes contiennent des enregistrements pas encore envoyés. Ceux des coffres sont effacés\u202f; Poros propose d’envoyer les autres après le redémarrage.",
  },
} satisfies TranslatedArea<"updates">;
