import type { TranslatedArea } from "../catalog";

export const dialogs = {
  // components/ConflictDialog
  fileAlreadyExists: "Datei bereits vorhanden",
  alreadyExistsIn: "{name} ist in {folder} bereits vorhanden.",
  modified: "Geändert",
  existingFile: "Vorhandene Datei",
  overwriteIfNewer: "Überschreiben, wenn neuer",
  keepBoth: "Beide behalten",
  localFile: "Lokale Datei",
  serverFile: "Datei auf dem Server",
  doTheSameForAny: "Bei weiteren Konflikten in diesen Übertragungen dasselbe tun",
  doTheSameForOthers: {
    one: "Beim anderen Konflikt und allen folgenden dasselbe tun",
    other: "Bei den {count} anderen Konflikten und allen folgenden dasselbe tun",
  },

  // components/DeleteDialog
  nothingToDelete: "Nichts zu löschen",
  deleteItem: "Element löschen",
  deleteItems: { one: "{count} Element löschen", other: "{count} Elemente löschen" },
  checkingTheFolder: "Ordner wird geprüft...",
  theseItemsAreNoLonger: "Diese Elemente sind nicht mehr im Ordner.",

  // components/PropertiesDialog
  sizeWithContents: "{size} ({contents})",
  items: { one: "{count} Element", other: "{count} Elemente" },
  files: { one: "{count} Datei", other: "{count} Dateien" },
  folders: { one: "{count} Ordner", other: "{count} Ordner" },
  countingFiles: {
    one: "Wird gezählt: {count} Datei, {size}",
    other: "Wird gezählt: {count} Dateien, {size}",
  },
  propertiesOf: "Eigenschaften von {name}",
  propertiesOfItems: {
    one: "Eigenschaften von {count} Element",
    other: "Eigenschaften von {count} Elementen",
  },
  items2: "Elemente",
  links: { one: "{count} Verknüpfung", other: "{count} Verknüpfungen" },
  type: "Typ",
  location: "Ort",
  pointsTo: "Ziel",
  calculate: "Berechnen",
  accessed: "Letzter Zugriff",
  mixedUnchanged: "Gemischt, unverändert",
  permissions: "Berechtigungen",
  linksHaveNoPermissionsOf:
    "Verknüpfungen haben keine eigenen Berechtigungen und bleiben daher, wie sie sind. Änderungen an Besitzer und Gruppe gelten für die Verknüpfungen selbst.",
  andMoreListedInThe: {
    one: "und {count} weiteres, im Protokoll aufgeführt",
    other: "und {count} weitere, im Protokoll aufgeführt",
  },
  changedItems: { one: "{count} Element geändert", other: "{count} Elemente geändert" },
  octal: "Oktal",
  mixed: "Gemischt",
  questionMarksAreLeftAs: "Wo ein Fragezeichen steht, behält jedes Element seinen Wert",
  owner: "Besitzer",
  group: "Gruppe",
  folders2: "Ordner",
  files2: "Dateien",
  oneItemCouldNotBe: "Ein Element konnte nicht geändert werden:",
  itemsCouldNotBeChanged: {
    one: "{count} Element konnte nicht geändert werden:",
    other: "{count} Elemente konnten nicht geändert werden:",
  },
  applying: "Wird übernommen...",
  linkToAFolder: "Verknüpfung zu einem Ordner",
  linkToAFile: "Verknüpfung zu einer Datei",
  specialFile: "Spezialdatei",
  extensionFile: "{extension}-Datei",
  nameWithId: "{name} ({id})",
  measuringFolderSize: "Ordnergröße wird ermittelt",
  changingPermissionsOf: "Berechtigungen von {what} werden geändert",
  applyToEverythingInside: {
    one: "Auf alles im Ordner anwenden",
    other: "Auf alles in den Ordnern anwenden",
  },
  permissionOfClass: "{who}: {permission}",

  // lib/deletion
  deletedCount: { one: "{count} gelöscht", other: "{count} gelöscht" },
  countingToDelete: "Zu Löschendes wird gezählt...",
  deletedFoundSoFar: {
    one: "{count} gelöscht, bisher {found} gefunden",
    other: "{count} gelöscht, bisher {found} gefunden",
  },
  deletedOfTotal: { one: "{count} von {total} gelöscht", other: "{count} von {total} gelöscht" },

  // lib/permissions
  setUserId: "Benutzer-ID setzen",
  runsAsTheFilesOwner: "Läuft als Besitzer der Datei",
  setGroupId: "Gruppen-ID setzen",
  runsAsTheGroupNew: "Läuft als die Gruppe, neue Dateien erben sie",
  sticky: "Sticky",
  onlyOwnersMayDeleteOr: "Nur Besitzer dürfen Einträge löschen oder umbenennen",
  others: "Andere",
  read: "Lesen",
  write: "Schreiben",
  execute: "Ausführen",
} satisfies TranslatedArea<"dialogs">;
