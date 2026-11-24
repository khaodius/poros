import type { TranslatedArea } from "../catalog";

export const sync = {
  // components/SyncDialog
  chooseTheLocalFolder: "Choisir le dossier local",
  synchronizeFolders: "Synchroniser des dossiers",
  localFolder: "Dossier local",
  theFolderOnThisComputer: "Le dossier sur cet ordinateur",
  server: "Serveur",
  noSftpServerConnected: "Aucun serveur SFTP connecté",
  serverFolder: "Dossier du serveur",
  direction: "Sens",
  rsyncOptions: "Options rsync",
  schedule: "Planifier...",
  runThisSynchronizationAtSet: "Exécuter cette synchronisation à heures fixes",
  fillInBothFoldersOf:
    "Remplissez les deux dossiers d’une connexion enregistrée pour planifier ceci",
  comparingContentsOfFiles: {
    one: "Comparaison du contenu\u00a0: {compared} sur {count} fichier",
    many: "Comparaison du contenu\u00a0: {compared} sur {count} de fichiers",
    other: "Comparaison du contenu\u00a0: {compared} sur {count} fichiers",
  },
  startingFromTheDefaultsIn:
    "À partir des valeurs par défaut des paramètres, avec les arguments rsync enregistrés pour {siteName}. Les modifications faites ici ne s’appliquent qu’à cette synchronisation.",
  startingFromTheDefaultsInSettings:
    "À partir des valeurs par défaut des paramètres. Les modifications faites ici ne s’appliquent qu’à cette synchronisation.",
  compareDryRun: "Comparer (simulation)",
  compare: "Comparer",
  listingFoldersLocalAndServer:
    "Lecture des dossiers\u00a0: {local} entrées locales et {remote} sur le serveur",

  // components/SyncPlanList
  rsync: "rsync {arguments}",
  theOptionsAsRsyncArguments: "Les options sous forme d’arguments rsync",
  dryRunRunningThisChanges:
    "Simulation\u00a0: l’exécuter ne modifie rien et écrit dans le journal ce qui se passerait.",
  unchangedSourceFilesWillBe: {
    one: "{count} fichier source inchangé sera aussi supprimé.",
    many: "{count} de fichiers source inchangés seront aussi supprimés.",
    other: "{count} fichiers source inchangés seront aussi supprimés.",
  },
  includeAll: "Tout inclure",
  action: "Action",
  why: "Raison",
  entriesPassedOverBrokenOr: {
    one: "{count} entrée écartée\u00a0: liens rompus ou en boucle, fichiers spéciaux et dossiers illisibles",
    many: "{count} d’entrées écartées\u00a0: liens rompus ou en boucle, fichiers spéciaux et dossiers illisibles",
    other:
      "{count} entrées écartées\u00a0: liens rompus ou en boucle, fichiers spéciaux et dossiers illisibles",
  },
  include: "Inclure {path}",
  settle: "Résoudre {path}",
  leaveAsIs: "Laisser tel quel",
  uploadLocalCopy: "Envoyer la copie locale",
  downloadServerCopy: "Télécharger la copie du serveur",
  files: { one: "{count} fichier", many: "{count} de fichiers", other: "{count} fichiers" },
  all: "Tout",
  conflicts: "Conflits",
  noFilesToCopy: "Aucun fichier à copier.",
  everythingIsInSync: "Tout est synchronisé.",
  starting: "Démarrage...",
  settleByHand: "Résoudre à la main",
  dryRun: "Simulation",
  synchronize: "Synchroniser",
  localSide: "Local",
  serverSide: "Serveur",
  sideMissing: "{side}\u00a0: absent",
  sideFolder: "{side}\u00a0: dossier",
  sideFolderWithPermissions: "{side}\u00a0: dossier, {permissions}",
  sideFile: "{side}\u00a0: {size}, {modified}",
  sideFileWithPermissions: "{side}\u00a0: {size}, {modified}, {permissions}",
  sideLink: "{side}\u00a0: lien, {size}, {modified}",
  sideLinkWithPermissions: "{side}\u00a0: lien, {size}, {modified}, {permissions}",

  // components/SyncSettings
  rsync2: "rsync",
  rsyncRunsOnTheServer:
    "rsync s’exécute sur le serveur par votre connexion SSH\u202f; rsync doit donc y être installé. Les démons rsync (adresses rsync:// sur le port 873) ne sont pas pris en charge.",
  useRsync: "Utiliser rsync",
  useRsyncHint:
    "Les fichiers qui existent déjà de l’autre côté sont mis à jour en n’envoyant que les parties modifiées. Si cette option est désactivée, ou si le serveur n’a pas rsync, les fichiers sont copiés en entier par SFTP.",
  forFilesLargerThan: "Pour les fichiers de plus de",
  forFilesLargerThanHint: "Les fichiers plus petits sont plus rapides à copier en entier.",
  rsyncOnTheServer: "rsync sur le serveur",
  rsyncOnTheServerHint: "La commande qui le lance, pour les serveurs où il n’est pas dans le PATH.",
  folderSynchronization: "Synchronisation de dossiers",
  eachSynchronizationStartsWithThese:
    "Chaque synchronisation part de ces options. Les arguments rsync propres à un site enregistré s’y ajoutent, et «\u00a0Synchroniser des dossiers\u00a0» peut modifier chacune d’elles pour une exécution.",
  typedAsOnRsyncsCommand:
    "Saisis comme sur la ligne de commande de rsync\u202f; ils s’appliquent par-dessus les options ci-dessus pour chaque synchronisation.",

  // lib/sync
  folders: { one: "{count} dossier", many: "{count} de dossiers", other: "{count} dossiers" },
  files2: {
    one: "{count} fichier, {size}",
    many: "{count} de fichiers, {size}",
    other: "{count} fichiers, {size}",
  },
  deleteItems: {
    one: "Supprimer {count} élément",
    many: "Supprimer {count} d’éléments",
    other: "Supprimer {count} éléments",
  },
  deletionsHeldBackByThe: {
    one: "{count} suppression retenue par la limite",
    many: "{count} de suppressions retenues par la limite",
    other: "{count} suppressions retenues par la limite",
  },
  emptyFoldersLeftOut: {
    one: "{count} dossier vide écarté",
    many: "{count} de dossiers vides écartés",
    other: "{count} dossiers vides écartés",
  },
  wouldCopyFiles: {
    one: "copierait {count} fichier ({size})",
    many: "copierait {count} de fichiers ({size})",
    other: "copierait {count} fichiers ({size})",
  },
  deleteItems2: {
    one: "supprimerait {count} élément",
    many: "supprimerait {count} d’éléments",
    other: "supprimerait {count} éléments",
  },
  createFolders: {
    one: "créerait {count} dossier",
    many: "créerait {count} de dossiers",
    other: "créerait {count} dossiers",
  },
  queuedFiles: {
    one: "{count} fichier mis en file d’attente ({size})",
    many: "{count} de fichiers mis en file d’attente ({size})",
    other: "{count} fichiers mis en file d’attente ({size})",
  },
  deletedItems: {
    one: "{count} élément supprimé",
    many: "{count} d’éléments supprimés",
    other: "{count} éléments supprimés",
  },
  itemsToDeleteOnceCopying: {
    one: "{count} élément à supprimer une fois la copie terminée",
    many: "{count} d’éléments à supprimer une fois la copie terminée",
    other: "{count} éléments à supprimer une fois la copie terminée",
  },
  createdFolders: {
    one: "{count} dossier créé",
    many: "{count} de dossiers créés",
    other: "{count} dossiers créés",
  },
  createdLinks: {
    one: "{count} lien créé",
    many: "{count} de liens créés",
    other: "{count} liens créés",
  },
  updatedFiles: {
    one: "{count} fichier mis à jour",
    many: "{count} de fichiers mis à jour",
    other: "{count} fichiers mis à jour",
  },
  removedUnchangedSourceFiles: {
    one: "{count} fichier source inchangé supprimé",
    many: "{count} de fichiers source inchangés supprimés",
    other: "{count} fichiers source inchangés supprimés",
  },
  localToServer: "Local vers serveur",
  makesTheServerFolderMatch: "Aligne le dossier du serveur sur le dossier local.",
  serverToLocal: "Serveur vers local",
  makesTheLocalFolderMatch: "Aligne le dossier local sur celui du serveur.",
  bothWays: "Dans les deux sens",
  copiesNewFilesEachWay:
    "Copie les nouveaux fichiers dans chaque sens\u202f; quand les deux côtés ont un fichier, le plus récent l’emporte.",
  deleteLocal: "Supprimer en local",
  deleteOnServer: "Supprimer sur le serveur",
  conflict: "Conflit",
  plannedUpload: "Envoyer {total}",
  plannedDownload: "Télécharger {total}",
  conflictsLeft: {
    one: "{count} conflit laissé tel quel",
    many: "{count} de conflits laissés tels quels",
    other: "{count} conflits laissés tels quels",
  },
  plannedSentences: "{parts}.",
  plannedSeparator: ". ",
  nothingSelected: "Rien de sélectionné.",
  onlyInTheLocalFolder: "Seulement dans le dossier local",
  onlyOnTheServer: "Seulement sur le serveur",
  sizeOrTimeDiffers: "La taille ou la date diffère",
  contentsDiffer: "Le contenu diffère",
  copiedWithoutComparing: "Copié sans comparaison",
  newerInTheLocalFolder: "Plus récent dans le dossier local",
  newerOnTheServer: "Plus récent sur le serveur",
  notInTheLocalFolder: "Absent du dossier local",
  notOnTheServer: "Absent du serveur",
  typeDiffers: "{here} ici, {there} sur le serveur",
  differsWithTheSameTimeOnBothSides: "Diffère, avec la même date des deux côtés",
  shorterOnTheTarget: "Plus court sur la cible\u202f; seule la suite est envoyée",
  sameContentsOnlyTheTimeOrPermissions:
    "Même contenu\u202f; seules la date ou les permissions sont appliquées",
  nothingHere: "Rien",
  folderHere: "Dossier",
  linkHere: "Lien",
  fileHere: "Fichier",
  nothingThere: "rien",
  folderThere: "dossier",
  linkThere: "lien",
  fileThere: "fichier",
  countUnchanged: {
    one: "{count} inchangé",
    many: "{count} inchangés",
    other: "{count} inchangés",
  },
  countKeptAsTheyAre: {
    one: "{count} laissé tel quel",
    many: "{count} laissés tels quels",
    other: "{count} laissés tels quels",
  },
  countOnlyInTheLocalFolderNotDeleted: {
    one: "{count} seulement dans le dossier local, non supprimé",
    many: "{count} seulement dans le dossier local, non supprimés",
    other: "{count} seulement dans le dossier local, non supprimés",
  },
  countOnlyOnTheServerNotDeleted: {
    one: "{count} seulement sur le serveur, non supprimé",
    many: "{count} seulement sur le serveur, non supprimés",
    other: "{count} seulement sur le serveur, non supprimés",
  },
  countProtectedFromDeletion: {
    one: "{count} protégé de la suppression",
    many: "{count} protégés de la suppression",
    other: "{count} protégés de la suppression",
  },
  countNewNotCreated: {
    one: "{count} nouveau, non créé",
    many: "{count} nouveaux, non créés",
    other: "{count} nouveaux, non créés",
  },
  countOutsideTheSizeLimits: {
    one: "{count} hors des limites de taille",
    many: "{count} hors des limites de taille",
    other: "{count} hors des limites de taille",
  },
  countExcluded: { one: "{count} exclu", many: "{count} exclus", other: "{count} exclus" },
  countPassedOver: { one: "{count} écarté", many: "{count} écartés", other: "{count} écartés" },
  dryRunSummary:
    "Simulation\u00a0: {parts}. Rien n’a été modifié\u202f; le journal donne les détails.",
  runSummary: "{done}.",
  runSummaryWithFailures: {
    one: "{done}. {count} modification a échoué\u202f; le journal donne les détails.",
    many: "{done}. {count} de modifications ont échoué\u202f; le journal donne les détails.",
    other: "{done}. {count} modifications ont échoué\u202f; le journal donne les détails.",
  },
} satisfies TranslatedArea<"sync">;
