import type { TranslatedArea } from "../catalog";

export const compare = {
  // components/CompareDialog
  compareFiles: "Comparer des fichiers",
  swapSides: "Inverser les côtés",
  onlyChanges: "Modifications seules",
  ignoreWhitespace: "Ignorer les espaces",
  ignoreCase: "Ignorer la casse",
  onTheLeftOnThe: "{left} à gauche, {right} à droite.",
  readingBothFiles: "Lecture des deux fichiers...",
  previousChangeAltUp: "Modification précédente (Alt+Up)",
  previousChange: "Modification précédente",
  nextChangeAltDown: "Modification suivante (Alt+Down)",
  nextChange: "Modification suivante",
  changes: {
    one: "{count} modification",
    many: "{count} de modifications",
    other: "{count} modifications",
  },
  showUnchangedLines: {
    one: "Afficher {count} ligne inchangée",
    many: "Afficher {count} de lignes inchangées",
    other: "Afficher {count} lignes inchangées",
  },
  comparing: "Comparaison...",
  noChanges: "Aucune modification",
  changeOf: "Modification {current} sur {count}",

  // components/FolderCompareBar
  synchronizeTheseFolders: "Synchroniser ces dossiers",
  synchronizeTheseFolders2: "Synchroniser ces dossiers ({unavailable})",
  folderComparison: "Comparaison de dossiers",
  comparedWith: "Comparé avec {where}",
  comparedWithAt: "Comparé avec {where}\u00a0: {path}",
  again: "{action} de nouveau",
  selectByState: "Sélectionner par état",
  comparisonOptions: "Options de comparaison",
  stopComparing: "Arrêter la comparaison ({shortcut})",
  rowsByHowTheyCompare: "Lignes selon le résultat de la comparaison",
  comparingSubfoldersItemsListedSo: {
    one: "Comparaison des sous-dossiers\u00a0: {count} élément listé jusqu’ici",
    many: "Comparaison des sous-dossiers\u00a0: {count} d’éléments listés jusqu’ici",
    other: "Comparaison des sous-dossiers\u00a0: {count} éléments listés jusqu’ici",
  },
  couldNotCompareSubfolders: "Impossible de comparer les sous-dossiers\u00a0: {message}",
  thereAreTooManySubfolders:
    "Il y a trop de sous-dossiers pour tous les marquer. Les plus profonds ne sont pas marqués.",
  thePanesAreNoLonger: "{note}. Les deux volets ne sont plus parcourus ensemble.",
  inAnotherWindow: "{label} dans une autre fenêtre",
  nothingToCompareYet: "rien à comparer pour l’instant",
  stateCount: "{state}\u00a0: {amount}",
  stateCountHidden: "{state}\u00a0: {amount}, masqués",

  // components/folderCompareMenu
  selectNewerAndOnlyHere: "Sélectionner les plus récents et ceux présents seulement ici",
  selectDifferent: "Sélectionner les différents",
  hideIdenticalItems: "Masquer les éléments identiques",
  compareBySizeOnly: "Comparer par la taille uniquement",
  ignoreTimeZoneOffsetsWhole: "Ignorer les décalages de fuseau horaire (heures entières)",
  includeHiddenFiles: "Inclure les fichiers cachés",
  browseBothPanesTogether: "Parcourir les deux volets ensemble",
  compareFolders: "Comparer les dossiers",
  compareContentsOfSubfolders: "Comparer le contenu des sous-dossiers",

  // hooks/useFolderCompare
  syncOnlyLocalAndSftp: "entre cet ordinateur et un serveur SFTP uniquement",
  syncFromServerWindow: "depuis la fenêtre qui contient l’onglet du serveur",

  // lib/compare
  lfUnix: "LF (Unix)",
  crlfWindows: "CRLF (Windows)",
  lineEndingNone: "aucune",
  lineEndingMixed: "mixtes",
  lineEndingsDiffer: "Les fins de ligne diffèrent\u00a0: {left} à gauche, {right} à droite.",
  filesIdentical: "Les fichiers sont identiques.",
  filesDifferTooLarge:
    "Les fichiers diffèrent. Ils sont trop volumineux pour être affichés côte à côte.",
  filesDifferNotText:
    "Les fichiers diffèrent. Ce ne sont pas des fichiers texte, seuls leurs octets ont donc été comparés.",
  linesChanged: {
    one: "{count} ligne modifiée",
    many: "{count} de lignes modifiées",
    other: "{count} lignes modifiées",
  },
  linesAdded: {
    one: "{count} ligne ajoutée",
    many: "{count} de lignes ajoutées",
    other: "{count} lignes ajoutées",
  },
  linesRemoved: {
    one: "{count} ligne supprimée",
    many: "{count} de lignes supprimées",
    other: "{count} lignes supprimées",
  },
  lineCounts: "{counts}.",
  onlyLineEndingsDiffer: "Seules les fins de ligne diffèrent.",
  filesDifferInIgnored: "Les fichiers ne diffèrent que par ce qui est ignoré.",
  filesDifferInBytes: "Les lignes sont les mêmes\u202f; les fichiers diffèrent par leurs octets.",

  // lib/folderCompare
  differsInside: "Diffère à l’intérieur ({found})",
  noDifferencesFoundInsideBut:
    "Aucune différence trouvée à l’intérieur, mais tout n’a pas été comparé ({found})",
  identicalInsideFiles: {
    one: "Identique à l’intérieur\u00a0: {count} fichier",
    many: "Identique à l’intérieur\u00a0: {count} de fichiers",
    other: "Identique à l’intérieur\u00a0: {count} fichiers",
  },
  identicalInsideNoFilesOn: "Identique à l’intérieur\u00a0: aucun fichier d’un côté ni de l’autre",
  newerHere: "Plus récent ici",
  olderHere: "Plus ancien ici",
  onlyHere: "Seulement ici",
  different: "Différent",
  identical: "Identique",
  differentAndNeitherIsNewer: "Différent, et aucun n’est plus récent",
  onBothSidesTheContents: "Des deux côtés. Le contenu n’est pas comparé.",
  neitherListingRefreshed:
    "Le contenu d’aucun des deux volets n’a pu être actualisé\u202f; les marques peuvent donc être dépassées jusqu’à ce que les deux soient relus",
  thisListingNotRefreshed:
    "Le contenu de ce volet n’a pas pu être actualisé\u202f; les marques peuvent donc être dépassées jusqu’à ce qu’il soit relu",
  partnerListingNotRefreshed:
    "Le contenu de {partner} n’a pas pu être actualisé\u202f; les marques peuvent donc être dépassées jusqu’à ce qu’il soit relu",
  pausedBothShowResults:
    "La comparaison est suspendue tant que les deux volets affichent des résultats de recherche. Les marques reviennent quand les résultats sont fermés",
  pausedThisShowsResults:
    "La comparaison est suspendue tant que ce volet affiche des résultats de recherche. Les marques reviennent quand les résultats sont fermés",
  pausedPartnerShowsResults:
    "La comparaison est suspendue tant que {partner} affiche des résultats de recherche. Les marques reviennent quand les résultats sont fermés",
  summaryNewerHere: {
    one: "{count} plus récent ici",
    many: "{count} plus récents ici",
    other: "{count} plus récents ici",
  },
  summaryOlderHere: {
    one: "{count} plus ancien ici",
    many: "{count} plus anciens ici",
    other: "{count} plus anciens ici",
  },
  summaryOnlyHere: {
    one: "{count} seulement ici",
    many: "{count} seulement ici",
    other: "{count} seulement ici",
  },
  summaryOnlyThere: {
    one: "{count} seulement dans l’autre volet",
    many: "{count} seulement dans l’autre volet",
    other: "{count} seulement dans l’autre volet",
  },
  summaryDifferent: {
    one: "{count} différent",
    many: "{count} différents",
    other: "{count} différents",
  },
  summaryIdentical: {
    one: "{count} identique",
    many: "{count} identiques",
    other: "{count} identiques",
  },
  summaryNotCompared: {
    one: "{count} non comparé",
    many: "{count} non comparés",
    other: "{count} non comparés",
  },

  // state/folderCompare
  noFolderToCompare: "Ce volet n’a aucun dossier à comparer",
  openASecondPane: "Ouvrez un second volet de dossier pour comparer celui-ci",
  paneShowsResults: "{pane} affiche des résultats de recherche",
  noFolderNamed: "Aucun dossier nommé {name} dans {pane}",
  noFolderAbove: "{pane} n’a aucun dossier au-dessus de celui-ci",
  paneCouldNotOpen: "{pane} n’a pas pu ouvrir {path}",
} satisfies TranslatedArea<"compare">;
