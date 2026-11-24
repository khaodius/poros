import type { MessageArea } from "../catalog";

export const rename = {
  // components/BulkRenameDialog
  nothingToRename: "Nothing to rename",
  renameItems: { one: "Rename {count} item", other: "Rename {count} items" },
  appliedInThisOrderEach: "Applied in this order, each to the result of the last.",
  findAndReplace: "Find and replace",
  find: "Find",
  regularExpression: "Regular expression",
  recentPatterns: "Recent patterns",
  recent: "Recent",
  replaceWith: "Replace with",
  replaceWith1OrName: "Replace with, $1 or $<name> for groups",
  patternIs: "Pattern is",
  matchCase: "Match case",
  everyMatch: "Every match",
  lookIn: "Look in",
  template: "Template",
  templateForTheNewName: "Template for the new name",
  insertAToken: "Insert a token",
  dateAs: "Date as",
  timeAs: "Time as",
  theDateAndTimeAre:
    "The date and time are when the item was last modified. Without {ext}, items keep their extension.",
  counter: "Counter",
  step: "Step",
  digits: "Digits",
  countsIn: "Counts in",
  listOrder: "List order",
  nameOrder: "Name order",
  theTemplatePlacesTheCounter: "The template places the counter where {n} is.",
  addToEachName: "Add to each name",
  separator: "Separator",
  case: "Case",
  extension: "Extension",
  newExtension: "New extension",
  newExtensionSuchAsJpg: "New extension, such as jpg",
  trimAndTidy: "Trim and tidy",
  trimSpacesAtTheEnds: "Trim spaces at the ends",
  makeRepeatedSpacesOne: "Make repeated spaces one",
  replaceSpacesWith: "Replace spaces with",
  characterThatReplacesSpaces: "Character that replaces spaces",
  rowsShown: "Rows shown",
  oldAndNewNames: "Old and new names",
  noNamesChangeYet: "No names change yet.",
  couldNotRefreshThisFolder: "Could not refresh this folder. This listing may be out of date.",
  resetRules: "Reset rules",
  ofItemsWillBeRenamed: {
    one: "{ready} of {count} item will be renamed{unchanged}.",
    other: "{ready} of {count} items will be renamed{unchanged}.",
  },

  // components/RenameReportDialog
  renamingStopped: "Renaming stopped",
  ofItemsIn: {
    one: "{renamedCount} of {count} item in",
    other: "{renamedCount} of {count} items in",
  },
  items: { one: "{count} item", other: "{count} items" },
  couldNotBeRenamedTo: "could not be renamed to",
  isNowNamed: "is now named",
  becauseItCouldNotBe:
    ", because it could not be given its name back: {message}. Rename it by hand.",
  whatBecameOfEachRename: "What became of each rename",
  noItems: "No items.",

  // components/RenameTable
  newName: "New name",
  status: "Status",
  noName: "No name",

  // lib/bulkRename
  theNameIsEmpty: "The name is empty",
  theNameIsTooLong: "The name is too long: {length} of {maxLength} {lengthUnit}",
  nowNamed: "Now named {temporary}: {message}",

  // state/bulkRename
  ofItems: { one: "{count} of {count} item", other: "{count} of {count} items" },
  renamingCancelledRenamed: "Renaming cancelled. {count} renamed.",
  renamedItemsIn: {
    one: "Renamed {count} item in {folder}",
    other: "Renamed {count} items in {folder}",
  },
  // components/BulkRenameDialog
  text: "Text",
  regex: "Regex",
  both: "Both",
  unchanged: "Unchanged",
  titleCase: "Title Case",
  sentenceCase: "Sentence case",
  beforeTheName: "Before the name",
  afterTheName: "After the name",
  all: "All",
  changed: "Changed",
  problems: "Problems",

  // components/RenameReportDialog
  renamed: "Renamed",
  notRenamed: "Not renamed",
  failed: "Failed",
  leftUnderAnotherName: "Left under another name",
  ofIn: "{renamedCount} of {t} in",

  // lib/bulkRename
  nameWithoutTheExtension: "Name without the extension",
  dateModified: "Date modified",
  timeModified: "Time modified",
  nameOfTheFolder: "Name of the folder",
} as const satisfies MessageArea;
