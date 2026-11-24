import type { TranslatedArea } from "../catalog";

export const rename = {
  // components/BulkRenameDialog
  nothingToRename: "Nichts umzubenennen",
  renameItems: { one: "{count} Element umbenennen", other: "{count} Elemente umbenennen" },
  appliedInThisOrderEach:
    "In dieser Reihenfolge angewendet, jede Regel auf das Ergebnis der vorigen.",
  findAndReplace: "Suchen und ersetzen",
  find: "Suchen",
  regularExpression: "Regulärer Ausdruck",
  recentPatterns: "Letzte Muster",
  recent: "Zuletzt",
  replaceWith: "Ersetzen durch",
  replaceWith1OrName: "Ersetzen durch, $1 oder $<name> für Gruppen",
  patternIs: "Muster ist",
  matchCase: "Groß-/Kleinschreibung beachten",
  everyMatch: "Jeder Treffer",
  lookIn: "Suchen in",
  template: "Vorlage",
  templateForTheNewName: "Vorlage für den neuen Namen",
  insertAToken: "Platzhalter einfügen",
  dateAs: "Datum als",
  timeAs: "Uhrzeit als",
  theDateAndTimeAre:
    "Datum und Uhrzeit sind die der letzten Änderung des Elements. Ohne {ext} behalten Elemente ihre Erweiterung.",
  counter: "Zähler",
  step: "Schritt",
  digits: "Stellen",
  countsIn: "Zählt in",
  listOrder: "Listenreihenfolge",
  nameOrder: "Namensreihenfolge",
  theTemplatePlacesTheCounter: "Die Vorlage setzt den Zähler dort ein, wo {n} steht.",
  addToEachName: "An jeden Namen anfügen",
  separator: "Trennzeichen",
  case: "Schreibweise",
  extension: "Erweiterung",
  newExtension: "Neue Erweiterung",
  newExtensionSuchAsJpg: "Neue Erweiterung, etwa jpg",
  trimAndTidy: "Kürzen und aufräumen",
  trimSpacesAtTheEnds: "Leerzeichen an den Enden entfernen",
  makeRepeatedSpacesOne: "Mehrfache Leerzeichen zu einem machen",
  replaceSpacesWith: "Leerzeichen ersetzen durch",
  characterThatReplacesSpaces: "Zeichen, das Leerzeichen ersetzt",
  rowsShown: "Angezeigte Zeilen",
  oldAndNewNames: "Alte und neue Namen",
  noNamesChangeYet: "Noch ändert sich kein Name.",
  couldNotRefreshThisFolder:
    "Dieser Ordner konnte nicht aktualisiert werden. Die Liste ist womöglich veraltet.",
  resetRules: "Regeln zurücksetzen",
  ofItemsWillBeRenamed: {
    one: "Umzubenennen: {ready} von {count} Element.",
    other: "Umzubenennen: {ready} von {count} Elementen.",
  },
  ofItemsWillBeRenamedSomeUnchanged: {
    one: "Umzubenennen: {ready} von {count} Element, {unchanged} unverändert.",
    other: "Umzubenennen: {ready} von {count} Elementen, {unchanged} unverändert.",
  },
  text: "Text",
  regex: "Regex",
  both: "Beides",
  unchanged: "Unverändert",
  titleCase: "Jedes Wort Groß",
  sentenceCase: "Satzanfang groß",
  beforeTheName: "Vor dem Namen",
  afterTheName: "Nach dem Namen",
  all: "Alle",
  changed: "Geändert",
  problems: "Probleme",
  checkingTheFolder: "Ordner wird geprüft...",
  theseItemsAreNoLongerInTheFolder: "Diese Elemente sind nicht mehr im Ordner.",
  selectedItemsLeftOut: {
    one: "{count} ausgewähltes Element ist nicht mehr im Ordner und wird ausgelassen.",
    other: "{count} ausgewählte Elemente sind nicht mehr im Ordner und werden ausgelassen.",
  },

  // components/RenameReportDialog
  renamingStopped: "Umbenennen gestoppt",
  renamedOfItemsIn: {
    one: "{count} von {total} in {folder} hat den neuen Namen.",
    other: "{count} von {total} in {folder} haben ihren neuen Namen.",
  },
  theOthersKeepTheirOldNamesButOne:
    "Die übrigen behalten ihre alten Namen, bis auf das unten genannte.",
  theOthersKeepTheirOldNames: "Die übrigen behalten ihre alten Namen.",
  items: { one: "{count} Element", other: "{count} Elemente" },
  couldNotBeRenamedTo: "{from} konnte nicht in {to} umbenannt werden: {message}",
  isNowNamedBecause:
    "{from} heißt jetzt {temporary}, da der alte Name nicht wiederhergestellt werden konnte: {message}. Benennen Sie es von Hand um.",
  whatBecameOfEachRename: "Ergebnis jeder Umbenennung",
  noItems: "Keine Elemente.",
  renamed: "Umbenannt",
  notRenamed: "Nicht umbenannt",
  failed: "Fehlgeschlagen",
  leftUnderAnotherName: "Unter anderem Namen belassen",

  // components/RenameTable
  newName: "Neuer Name",
  status: "Status",
  noName: "Kein Name",

  // lib/bulkRename
  theNameIsEmpty: "Der Name ist leer",
  theNameIsTooLong: "Der Name ist zu lang: {length} von {maxLength} {lengthUnit}",
  nowNamed: "Heißt jetzt {temporary}: {message}",
  nameWithoutTheExtension: "Name ohne Erweiterung",
  dateModified: "Änderungsdatum",
  timeModified: "Änderungszeit",
  nameOfTheFolder: "Name des Ordners",
  isNotAName: "„{name}“ ist kein Name",
  namesCannotContainControlCharacters: "Namen dürfen keine Steuerzeichen enthalten",
  namesCannotContain: "Namen dürfen {character} nicht enthalten",
  namesCannotEndWithASpaceOrADot: "Namen dürfen nicht auf ein Leerzeichen oder einen Punkt enden",
  isAReservedNameOnWindows: "{name} ist unter Windows ein reservierter Name",
  keepsThisName: "{name} behält diesen Namen",
  keepsANameDifferingInCase:
    "{name} behält einen Namen, der sich nur in der Groß-/Kleinschreibung unterscheidet",
  keepsANameDifferingInAccents:
    "{name} behält einen Namen, der sich nur in der Speicherung seiner Akzente unterscheidet",
  getsTheSameName: "{name} erhält denselben Namen",
  getsANameDifferingInCase:
    "{name} erhält einen Namen, der sich nur in der Groß-/Kleinschreibung unterscheidet",
  getsANameDifferingInAccents:
    "{name} erhält einen Namen, der sich nur in der Speicherung seiner Akzente unterscheidet",
  anotherSelectedItemHasTheSameName:
    "Ein anderes ausgewähltes Element hat denselben Namen. Benennen Sie diese einzeln um",
  isSelectedTooAndCountsAsTheSameName:
    "{name} ist ebenfalls ausgewählt und gilt hier als derselbe Name. Benennen Sie diese einzeln um",
  noModifiedTimeFor: "Das Element hat keine Änderungszeit für {date} oder {time}",
  anItemThatIsNotSelectedHasThisName: "Ein nicht ausgewähltes Element hat diesen Namen",
  notSelectedAndHasANameDifferingInCase:
    "{name} ist nicht ausgewählt und hat einen Namen, der sich nur in der Groß-/Kleinschreibung unterscheidet",
  notSelectedAndHasANameDifferingInAccents:
    "{name} ist nicht ausgewählt und hat einen Namen, der sich nur in der Speicherung seiner Akzente unterscheidet",
  problemSharedName: {
    one: "{count} Element hat denselben Namen wie ein anderes ausgewähltes Element",
    other: "{count} Elemente haben denselben Namen wie ein anderes ausgewähltes Element",
  },
  problemEmpty: { one: "{count} Name ist leer", other: "{count} Namen sind leer" },
  problemInvalid: {
    one: "{count} Name kann hier nicht verwendet werden",
    other: "{count} Namen können hier nicht verwendet werden",
  },
  problemTooLong: { one: "{count} Name ist zu lang", other: "{count} Namen sind zu lang" },
  problemUndated: {
    one: "{count} Element hat keine Änderungszeit",
    other: "{count} Elemente haben keine Änderungszeit",
  },
  problemDuplicate: {
    one: "{count} Element hätte einen doppelten Namen",
    other: "{count} Elemente hätten einen doppelten Namen",
  },
  problemTaken: {
    one: "{count} Name ist schon vergeben",
    other: "{count} Namen sind schon vergeben",
  },
  theRegularExpressionIsNotValid: "Der reguläre Ausdruck ist ungültig: {error}",
  problemsSentence: "{problems}.",

  // state/bulkRename
  renamingCancelledRenamed: {
    one: "Umbenennen abgebrochen. {renamed} von {count} Element umbenannt.",
    other: "Umbenennen abgebrochen. {renamed} von {count} Elementen umbenannt.",
  },
  renamingItemsIn: {
    one: "{count} Element in {folder} wird umbenannt",
    other: "{count} Elemente in {folder} werden umbenannt",
  },
} satisfies TranslatedArea<"rename">;
