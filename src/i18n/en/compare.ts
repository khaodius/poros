import type { MessageArea } from "../catalog";

export const compare = {
  // components/CompareDialog
  compareFiles: "Compare files",
  swapSides: "Swap sides",
  onlyChanges: "Only changes",
  ignoreWhitespace: "Ignore whitespace",
  ignoreCase: "Ignore case",
  onTheLeftOnThe: "{left} on the left, {right} on the right.",
  readingBothFiles: "Reading both files...",
  previousChangeAltUp: "Previous change (Alt+Up)",
  previousChange: "Previous change",
  nextChangeAltDown: "Next change (Alt+Down)",
  nextChange: "Next change",
  changes: { one: "{count} change", other: "{count} changes" },
  showUnchangedLines: { one: "Show {count} unchanged line", other: "Show {count} unchanged lines" },
  comparing: "Comparing...",
  noChanges: "No changes",
  changeOf: "Change {current} of {count}",

  // components/FolderCompareBar
  synchronizeTheseFolders: "Synchronize these folders",
  synchronizeTheseFolders2: "Synchronize these folders ({unavailable})",
  folderComparison: "Folder comparison",
  comparedWith: "Compared with {where}",
  comparedWithAt: "Compared with {where}: {path}",
  again: "{action} again",
  selectByState: "Select by state",
  comparisonOptions: "Comparison options",
  stopComparing: "Stop comparing ({shortcut})",
  rowsByHowTheyCompare: "Rows by how they compare",
  comparingSubfoldersItemsListedSo: {
    one: "Comparing subfolders: {count} item listed so far",
    other: "Comparing subfolders: {count} items listed so far",
  },
  couldNotCompareSubfolders: "Could not compare subfolders: {message}",
  thereAreTooManySubfolders:
    "There are too many subfolders to mark them all. The deepest are not marked.",
  thePanesAreNoLonger: "{note}. The panes are no longer in step.",
  inAnotherWindow: "{label} in another window",
  nothingToCompareYet: "nothing to compare yet",
  stateCount: "{state}: {amount}",
  stateCountHidden: "{state}: {amount}, not shown",

  // components/folderCompareMenu
  selectNewerAndOnlyHere: "Select newer and only here",
  selectDifferent: "Select different",
  hideIdenticalItems: "Hide identical items",
  compareBySizeOnly: "Compare by size only",
  ignoreTimeZoneOffsetsWhole: "Ignore time zone offsets (whole hours)",
  includeHiddenFiles: "Include hidden files",
  browseBothPanesTogether: "Browse both panes together",
  compareFolders: "Compare folders",
  compareContentsOfSubfolders: "Compare contents of subfolders",

  // hooks/useFolderCompare
  syncOnlyLocalAndSftp: "between this computer and an SFTP server only",
  syncFromServerWindow: "from the window with the server's tab",

  // lib/compare
  lfUnix: "LF (Unix)",
  crlfWindows: "CRLF (Windows)",
  lineEndingNone: "none",
  lineEndingMixed: "mixed",
  lineEndingsDiffer: "Line endings differ: {left} on the left, {right} on the right.",
  filesIdentical: "The files are identical.",
  filesDifferTooLarge: "The files differ. They are too large to show side by side.",
  filesDifferNotText: "The files differ. They are not text, so only their bytes were compared.",
  linesChanged: { one: "{count} changed line", other: "{count} changed lines" },
  linesAdded: { one: "{count} added line", other: "{count} added lines" },
  linesRemoved: { one: "{count} removed line", other: "{count} removed lines" },
  lineCounts: "{counts}.",
  onlyLineEndingsDiffer: "Only the line endings differ.",
  filesDifferInIgnored: "The files differ only in what is being ignored.",
  filesDifferInBytes: "The lines are the same; the files differ in their bytes.",

  // lib/folderCompare
  differsInside: "Differs inside: {found}",
  noDifferencesFoundInsideBut: "No differences found inside, but not all of it: {found}",
  identicalInsideFiles: {
    one: "Identical inside: {count} file",
    other: "Identical inside: {count} files",
  },
  identicalInsideNoFilesOn: "Identical inside: no files on either side",
  newerHere: "Newer here",
  olderHere: "Older here",
  onlyHere: "Only here",
  different: "Different",
  identical: "Identical",
  differentAndNeitherIsNewer: "Different, and neither is newer",
  onBothSidesTheContents: "On both sides. The contents are not compared.",
  neitherListingRefreshed:
    "Neither pane's listing could be refreshed, so the marks may be out of date until both are listed again",
  thisListingNotRefreshed:
    "This pane's listing could not be refreshed, so the marks may be out of date until it is listed again",
  partnerListingNotRefreshed:
    "The listing in {partner} could not be refreshed, so the marks may be out of date until it is listed again",
  pausedBothShowResults:
    "Comparing is paused while both panes show search results. The marks come back when the results are closed",
  pausedThisShowsResults:
    "Comparing is paused while this pane shows search results. The marks come back when the results are closed",
  pausedPartnerShowsResults:
    "Comparing is paused while {partner} shows search results. The marks come back when the results are closed",
  summaryNewerHere: { one: "{count} newer here", other: "{count} newer here" },
  summaryOlderHere: { one: "{count} older here", other: "{count} older here" },
  summaryOnlyHere: { one: "{count} only here", other: "{count} only here" },
  summaryOnlyThere: {
    one: "{count} only in the other pane",
    other: "{count} only in the other pane",
  },
  summaryDifferent: { one: "{count} different", other: "{count} different" },
  summaryIdentical: { one: "{count} identical", other: "{count} identical" },
  summaryNotCompared: { one: "{count} not compared", other: "{count} not compared" },

  // state/folderCompare
  noFolderToCompare: "This pane has no folder to compare",
  openASecondPane: "Open a second folder pane to compare this one with",
  paneShowsResults: "{pane} shows search results",
  noFolderNamed: "No folder named {name} in {pane}",
  noFolderAbove: "{pane} has no folder above this one",
  paneCouldNotOpen: "{pane} could not open {path}",
} as const satisfies MessageArea;
