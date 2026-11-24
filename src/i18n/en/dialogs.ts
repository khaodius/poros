import type { MessageArea } from "../catalog";

export const dialogs = {
  // components/ConflictDialog
  fileAlreadyExists: "File already exists",
  alreadyExistsIn: "{name} already exists in {folder}.",
  modified: "Modified",
  existingFile: "Existing file",
  overwriteIfNewer: "Overwrite if newer",
  keepBoth: "Keep both",
  localFile: "Local file",
  serverFile: "Server file",
  doTheSameForAny: "Do the same for any further conflicts in these transfers",
  doTheSameForOthers: {
    one: "Do the same for the {count} other conflict and any that follow",
    other: "Do the same for the {count} other conflicts and any that follow",
  },

  // components/DeleteDialog
  nothingToDelete: "Nothing to delete",
  deleteItem: "Delete item",
  deleteItems: { one: "Delete {count} items", other: "Delete {count} items" },
  checkingTheFolder: "Checking the folder...",
  theseItemsAreNoLonger: "These items are no longer in the folder.",

  // components/PropertiesDialog
  sizeWithContents: "{size} ({contents})",
  items: { one: "{count} item", other: "{count} items" },
  files: { one: "{count} file", other: "{count} files" },
  folders: { one: "{count} folder", other: "{count} folders" },
  countingFiles: {
    one: "Counting: {count} file, {size}",
    other: "Counting: {count} files, {size}",
  },
  propertiesOf: "Properties of {name}",
  propertiesOfItems: { one: "Properties of {count} items", other: "Properties of {count} items" },
  items2: "Items",
  links: { one: "{count} link", other: "{count} links" },
  type: "Type",
  location: "Location",
  pointsTo: "Points to",
  calculate: "Calculate",
  accessed: "Accessed",
  mixedUnchanged: "Mixed, unchanged",
  permissions: "Permissions",
  linksHaveNoPermissionsOf:
    "Links have no permissions of their own, so they keep theirs. Owner and group changes apply to the links themselves.",
  andMoreListedInThe: {
    one: "and {count} more, listed in the log",
    other: "and {count} more, listed in the log",
  },
  changedItems: { one: "Changed {count} item", other: "Changed {count} items" },
  octal: "Octal",
  mixed: "Mixed",
  questionMarksAreLeftAs: "Question marks are left as each item has them",
  owner: "Owner",
  group: "Group",
  folders2: "Folders",
  files2: "Files",
  oneItemCouldNotBe: "One item could not be changed:",
  itemsCouldNotBeChanged: {
    one: "{count} items could not be changed:",
    other: "{count} items could not be changed:",
  },
  applying: "Applying...",
  linkToAFolder: "Link to a folder",
  linkToAFile: "Link to a file",
  specialFile: "Special file",
  extensionFile: "{extension} file",
  nameWithId: "{name} ({id})",
  measuringFolderSize: "Measuring folder size",
  changingPermissionsOf: "Changing permissions of {what}",
  applyToEverythingInside: {
    one: "Apply to everything inside the folder",
    other: "Apply to everything inside the folders",
  },
  permissionOfClass: "{who} {permission}",

  // lib/deletion
  deletedCount: { one: "{count} deleted", other: "{count} deleted" },
  countingToDelete: "Counting what to delete...",
  deletedFoundSoFar: {
    one: "{count} deleted, {found} found so far",
    other: "{count} deleted, {found} found so far",
  },
  deletedOfTotal: { one: "{count} of {total} deleted", other: "{count} of {total} deleted" },

  // lib/permissions
  setUserId: "Set user ID",
  runsAsTheFilesOwner: "Runs as the file's owner",
  setGroupId: "Set group ID",
  runsAsTheGroupNew: "Runs as the group; new files inherit it",
  sticky: "Sticky",
  onlyOwnersMayDeleteOr: "Only owners may delete or rename entries",
  others: "Others",
  read: "Read",
  write: "Write",
  execute: "Execute",
} as const satisfies MessageArea;
