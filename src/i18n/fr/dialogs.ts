import type { TranslatedArea } from "../catalog";

export const dialogs = {
  // components/ConflictDialog
  fileAlreadyExists: "Le fichier existe déjà",
  alreadyExistsIn: "{name} existe déjà dans {folder}.",
  modified: "Modifié",
  existingFile: "Fichier existant",
  overwriteIfNewer: "Écraser si plus récent",
  keepBoth: "Conserver les deux",
  localFile: "Fichier local",
  serverFile: "Fichier du serveur",
  doTheSameForAny: "Faire de même pour les autres conflits de ces transferts",
  doTheSameForOthers: {
    one: "Faire de même pour l’autre conflit et ceux qui suivront",
    many: "Faire de même pour les {count} autres conflits et ceux qui suivront",
    other: "Faire de même pour les {count} autres conflits et ceux qui suivront",
  },

  // components/DeleteDialog
  nothingToDelete: "Rien à supprimer",
  deleteItem: "Supprimer l’élément",
  deleteItems: {
    one: "Supprimer {count} élément",
    many: "Supprimer {count} d’éléments",
    other: "Supprimer {count} éléments",
  },
  checkingTheFolder: "Vérification du dossier...",
  theseItemsAreNoLonger: "Ces éléments ne sont plus dans le dossier.",

  // components/PropertiesDialog
  sizeWithContents: "{size} ({contents})",
  items: { one: "{count} élément", many: "{count} d’éléments", other: "{count} éléments" },
  files: { one: "{count} fichier", many: "{count} de fichiers", other: "{count} fichiers" },
  folders: { one: "{count} dossier", many: "{count} de dossiers", other: "{count} dossiers" },
  countingFiles: {
    one: "Décompte\u00a0: {count} fichier, {size}",
    many: "Décompte\u00a0: {count} de fichiers, {size}",
    other: "Décompte\u00a0: {count} fichiers, {size}",
  },
  propertiesOf: "Propriétés de {name}",
  propertiesOfItems: {
    one: "Propriétés de {count} élément",
    many: "Propriétés de {count} d’éléments",
    other: "Propriétés de {count} éléments",
  },
  items2: "Éléments",
  links: { one: "{count} lien", many: "{count} de liens", other: "{count} liens" },
  type: "Type",
  location: "Emplacement",
  pointsTo: "Pointe vers",
  calculate: "Calculer",
  accessed: "Dernier accès",
  mixedUnchanged: "Mixte, inchangé",
  permissions: "Permissions",
  linksHaveNoPermissionsOf:
    "Les liens n’ont pas de permissions propres\u202f; les changements de permissions ne les modifient donc pas. Les changements de propriétaire et de groupe s’appliquent aux liens eux-mêmes.",
  andMoreListedInThe: {
    one: "et {count} autre, indiqué dans le journal",
    many: "et {count} d’autres, indiqués dans le journal",
    other: "et {count} autres, indiqués dans le journal",
  },
  changedItems: {
    one: "{count} élément modifié",
    many: "{count} d’éléments modifiés",
    other: "{count} éléments modifiés",
  },
  octal: "Octal",
  mixed: "Mixte",
  questionMarksAreLeftAs: "Les points d’interrogation restent tels que chaque élément les a",
  owner: "Propriétaire",
  group: "Groupe",
  folders2: "Dossiers",
  files2: "Fichiers",
  oneItemCouldNotBe: "Un élément n’a pas pu être modifié\u00a0:",
  itemsCouldNotBeChanged: {
    one: "{count} élément n’a pas pu être modifié\u00a0:",
    many: "{count} d’éléments n’ont pas pu être modifiés\u00a0:",
    other: "{count} éléments n’ont pas pu être modifiés\u00a0:",
  },
  applying: "Application...",
  linkToAFolder: "Lien vers un dossier",
  linkToAFile: "Lien vers un fichier",
  specialFile: "Fichier spécial",
  extensionFile: "Fichier {extension}",
  nameWithId: "{name} ({id})",
  measuringFolderSize: "Calcul de la taille du dossier",
  changingPermissionsOf: "Modification des permissions de {what}",
  applyToEverythingInside: {
    one: "Appliquer à tout le contenu du dossier",
    many: "Appliquer à tout le contenu des dossiers",
    other: "Appliquer à tout le contenu des dossiers",
  },
  permissionOfClass: "{who}\u00a0: {permission}",

  // lib/deletion
  deletedCount: { one: "{count} supprimé", many: "{count} supprimés", other: "{count} supprimés" },
  countingToDelete: "Décompte de ce qu’il faut supprimer...",
  deletedFoundSoFar: {
    one: "{count} supprimé, {found} trouvés jusqu’ici",
    many: "{count} supprimés, {found} trouvés jusqu’ici",
    other: "{count} supprimés, {found} trouvés jusqu’ici",
  },
  deletedOfTotal: {
    one: "{count} supprimé sur {total}",
    many: "{count} supprimés sur {total}",
    other: "{count} supprimés sur {total}",
  },

  // lib/permissions
  setUserId: "Définir l’UID",
  runsAsTheFilesOwner: "S’exécute en tant que propriétaire du fichier",
  setGroupId: "Définir le GID",
  runsAsTheGroupNew: "S’exécute en tant que groupe\u202f; les nouveaux fichiers en héritent",
  sticky: "Bit collant",
  onlyOwnersMayDeleteOr: "Seuls les propriétaires peuvent supprimer ou renommer les entrées",
  others: "Autres",
  read: "Lecture",
  write: "Écriture",
  execute: "Exécution",
} satisfies TranslatedArea<"dialogs">;
