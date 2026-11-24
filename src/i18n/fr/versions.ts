import type { TranslatedArea } from "../catalog";

export const versions = {
  // components/VersionsDialog
  restoreThisVersion: "Restaurer cette version",
  downloadThisVersion: "Télécharger cette version",
  openInTheEditor: "Ouvrir dans l’éditeur",
  compareWithTheCurrentFile: "Comparer avec le fichier actuel",
  downloadingTheVersionFromTo:
    "Téléchargement de la version du {when} dans {folder}. La file d’attente des transferts montre où il en est.",
  versionsOf: "Versions de {name}",
  loadingVersions: "Chargement des versions...",
  turnOnVersioningForThe:
    "Ce compartiment ne conserve aucune version. Activez la gestion des versions du compartiment pour conserver les fichiers qu’il remplace et supprime.",
  noVersionsOfThisFile: "Aucune version de ce fichier n’a été trouvée.",
  saved: "Enregistrée",
  status: "État",
  storageClass: "Classe de stockage",
  etag: "ETag",
  actions: "Actions",
  version: "Version {versionId}",
  entries: { one: "{count} entrée", many: "{count} d’entrées", other: "{count} entrées" },
  restoreVersion: "Restaurer la version",
  restore: "Restaurer",
  makeTheVersionOf: "Refaire de la version de {name} du {when} le fichier actuel\u202f?",
  thisBringsTheDeletedFile: "Le fichier supprimé revient. La suppression reste dans l’historique.",
  theCurrentContentStaysIn:
    "Le contenu actuel reste dans l’historique, il peut donc être restauré à son tour.",
  removeDeleteMarker: "Retirer le marqueur de suppression",
  deleteVersion: "Supprimer la version",
  removeMarker: "Retirer le marqueur",
  removeTheDeleteMarkerOf:
    "Retirer définitivement le marqueur de suppression de {name} du {when}\u202f? Cette action est irréversible.",
  deleteTheVersionOf:
    "Supprimer définitivement la version de {name} du {when}\u202f? Cette action est irréversible.",
  deleteForGood: "Supprimer définitivement",
  deletedItems: {
    one: "{count} élément supprimé",
    many: "{count} d’éléments supprimés",
    other: "{count} éléments supprimés",
  },
  everythingDeletedUnderAFolder:
    "Tout ce qui a été supprimé dans un dossier disparaît aussi. Les fichiers qui y sont encore ne sont pas touchés.",
  andMore: "et {count} de plus",
  removeThisDeleteMarker: "Retirer ce marqueur de suppression",
  deleteThisVersionForGood: "Supprimer définitivement cette version",
  versionFrom: "Version du {when}",
  currentVersion: "Version actuelle",
  isNoLongerDeleted: "{name} n’est plus supprimé.",
  theseItemsAreNoLongerDeleted: "Ces éléments ne sont plus supprimés.",
  removeWithEveryVersion:
    "Supprimer définitivement {what} et toutes les versions conservées\u202f? Cette action est irréversible.",

  // components/versionMenu
  versions: "Versions...",
  versionsThisBucketKeepsNone: "Versions (ce compartiment n’en conserve aucune)",
  showDeletedFiles: "Afficher les fichiers supprimés",
  showDeletedFilesThisBucket:
    "Afficher les fichiers supprimés (ce compartiment ne conserve pas de versions)",
  restoreItems: {
    one: "Restaurer {count} élément",
    many: "Restaurer {count} d’éléments",
    other: "Restaurer {count} éléments",
  },
  copyPaths: "Copier les chemins",
  copyPath: "Copier le chemin",
  deleteItemsForGood: {
    one: "Supprimer définitivement {count} élément",
    many: "Supprimer définitivement {count} d’éléments",
    other: "Supprimer définitivement {count} éléments",
  },

  // hooks/usePaneVersions
  deletedFilesCannotBeShown: "Impossible d’afficher les fichiers supprimés\u00a0: {message}",

  // lib/versions
  current: "Actuelle",
  deleted: "Supprimée",
  deleteMarker: "Marqueur de suppression",
  restoredCount: { one: "{count} restauré", many: "{count} restaurés", other: "{count} restaurés" },
  findingWhatToRestore: "Recherche de ce qu’il faut restaurer...",
  restoredFoundSoFar: {
    one: "{count} restauré, {total} trouvés jusqu’ici",
    many: "{count} restaurés, {total} trouvés jusqu’ici",
    other: "{count} restaurés, {total} trouvés jusqu’ici",
  },
  restoredOfTotal: {
    one: "{count} sur {total} restauré",
    many: "{count} sur {total} restaurés",
    other: "{count} sur {total} restaurés",
  },
  nothingOfTheFileIsLeft: "Il ne reste rien du fichier après elle.",
  theFileComesBackAsItsNewestVersion: "Le fichier revient sous sa version la plus récente.",
  theFileStaysDeleted:
    "Le fichier reste supprimé\u00a0: un autre marqueur de suppression se trouve sous celui-ci.",
  thisIsTheOnlyVersion: "C’est la seule version\u00a0: le fichier disparaîtra.",
  theVersionBeforeItBecomesCurrent: "La version précédente devient le fichier actuel.",
  theFileWillShowAsDeleted:
    "Le fichier apparaîtra comme supprimé\u00a0: l’entrée précédente est un marqueur de suppression.",
  unknownTime: "Date inconnue",
  versioningIsSuspended:
    "La gestion des versions est suspendue sur ce compartiment\u00a0: ce qu’il a conservé reste, et les nouvelles écritures remplacent la version marquée null.",
  theseKeysMayNotSeeVersioning:
    "Ces clés ne permettent peut-être pas de voir si le compartiment conserve des versions\u202f; certaines actions peuvent donc être refusées.",

  // state/versionActions
  downloadAVersionOfTo: "Télécharger une version de {name} vers",
  restoringItemsIn: {
    one: "Restauration de {count} élément dans {folder}",
    many: "Restauration de {count} d’éléments dans {folder}",
    other: "Restauration de {count} éléments dans {folder}",
  },
  restoringNameIn: "Restauration de {name} dans {folder}",
  deletingItemsForGoodIn: {
    one: "Suppression définitive de {count} élément dans {folder}",
    many: "Suppression définitive de {count} d’éléments dans {folder}",
    other: "Suppression définitive de {count} éléments dans {folder}",
  },
  deletingNameForGoodIn: "Suppression définitive de {name} dans {folder}",
  filesCouldNotBeRestored: {
    one: "{count} fichier n’a pas pu être restauré. Le journal indique pourquoi.",
    many: "{count} de fichiers n’ont pas pu être restaurés. Le journal indique pourquoi.",
    other: "{count} fichiers n’ont pas pu être restaurés. Le journal indique pourquoi.",
  },
  filesCouldNotBeDeleted: {
    one: "{count} fichier n’a pas pu être supprimé. Le journal indique pourquoi.",
    many: "{count} de fichiers n’ont pas pu être supprimés. Le journal indique pourquoi.",
    other: "{count} fichiers n’ont pas pu être supprimés. Le journal indique pourquoi.",
  },
} satisfies TranslatedArea<"versions">;
