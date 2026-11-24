import type { MessageArea } from "../catalog";

export const compare = {
  // components/CompareDialog
  compareFiles: "Compare files",
  swapSides: "Swap sides",
  onlyChanges: "Only changes",
  ignoreWhitespace: "Ignore whitespace",
  ignoreCase: "Ignore case",
  onTheLeftOnThe: "{size} on the left, {size2} on the right.",
  readingBothFiles: "Reading both files...",
  previousChangeAltUp: "Previous change (Alt+Up)",
  previousChange: "Previous change",
  nextChangeAltDown: "Next change (Alt+Down)",
  nextChange: "Next change",
  changes: { one: "{count} change", other: "{count} changes" },
  showUnchangedLines: { one: "Show {count} unchanged line", other: "Show {count} unchanged lines" },
  unchangedLines: { one: "{count} unchanged line", other: "{count} unchanged lines" },

  // components/FolderCompareBar
  synchronizeTheseFolders: "Synchronize these folders",
  synchronizeTheseFolders2: "Synchronize these folders ({unavailable})",
  folderComparison: "Folder comparison",
  comparedWith: "Compared with",
  again: "{value} again",
  selectByState: "Select by state",
  comparisonOptions: "Comparison options",
  stopComparing: "Stop comparing ({value})",
  rowsByHowTheyCompare: "Rows by how they compare",
  comparingSubfoldersItemsListedSo: {
    one: "Comparing subfolders: {count} item listed so far",
    other: "Comparing subfolders: {count} items listed so far",
  },
  items: { one: "{count} item", other: "{count} items" },
  couldNotCompareSubfolders: "Could not compare subfolders: {message}",
  thereAreTooManySubfolders:
    "There are too many subfolders to mark them all. The deepest are not marked.",
  thePanesAreNoLonger: "{note}. The panes are no longer in step.",

  // components/folderCompareMenu
  selectNewerAndOnlyHere: "Select newer and only here",
  selectDifferent: "Select different",
  hideIdenticalItems: "Hide identical items",
  compareBySizeOnly: "Compare by size only",
  ignoreTimeZoneOffsetsWhole: "Ignore time zone offsets (whole hours)",
  includeHiddenFiles: "Include hidden files",
  browseBothPanesTogether: "Browse both panes together",
  compareFolders: "Compare folders",

  // lib/folderCompare
  differsInside: "Differs inside: {found}",
  noDifferencesFoundInsideBut: "No differences found inside, but not all of it: {found}",
  identicalInsideFiles: {
    one: "Identical inside: {count} file",
    other: "Identical inside: {count} files",
  },
  identicalInsideNoFilesOn: "Identical inside: no files on either side",
  // components/CompareDialog
  show: "Show {t}",

  // components/FolderCompareBar
  comparingSubfoldersListedSoFar: "Comparing subfolders: {t} listed so far",

  // lib/compare
  lfUnix: "LF (Unix)",
  crlfWindows: "CRLF (Windows)",

  // lib/folderCompare
  newerHere: "Newer here",
  olderHere: "Older here",
  onlyHere: "Only here",
  different: "Different",
  identical: "Identical",
  differentAndNeitherIsNewer: "Different, and neither is newer",
  onBothSidesTheContents: "On both sides. The contents are not compared.",
} as const satisfies MessageArea;
