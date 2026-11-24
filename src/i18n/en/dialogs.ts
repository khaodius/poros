import type { MessageArea } from "../catalog";

export const dialogs = {
  // components/ConflictDialog
  fileAlreadyExists: "File already exists",
  alreadyExistsIn: "already exists in {targetDirectory}.",
  modified: "Modified",
  existingFile: "Existing file",

  // components/DeleteDialog
  nothingToDelete: "Nothing to delete",
  deleteItem: "Delete item",
  deleteItems: "Delete {count} items",
  checkingTheFolder: "Checking the folder...",

  // components/PropertiesDialog
  permissionsOfItems: { one: "permissions of {count} item", other: "permissions of {count} items" },
  linksKeptTheirOwnPermissions: {
    one: "; {count} link kept their own permissions",
    other: "; {count} links kept their own permissions",
  },
  items: { one: "{count} item", other: "{count} items" },
  files: { one: "{count} file", other: "{count} files" },
  folders: { one: "{count} folder", other: "{count} folders" },
  countingFiles: {
    one: "Counting: {count} file, {size}",
    other: "Counting: {count} files, {size}",
  },
  propertiesOf: "Properties of {name}",
  propertiesOfItems: "Properties of {count} items",
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
  andMoreListedInThe: "and {value} more, listed in the log",
  changedItems: { one: "Changed {count} item", other: "Changed {count} items" },
  octal: "Octal",
  mixed: "Mixed",
  questionMarksAreLeftAs: "Question marks are left as each item has them",
  // components/ConflictDialog
  overwriteIfNewer: "Overwrite if newer",
  keepBoth: "Keep both",

  // lib/permissions
  setUserId: "Set user ID",
  runsAsTheFilesOwner: "Runs as the file's owner",
  setGroupId: "Set group ID",
  runsAsTheGroupNew: "Runs as the group; new files inherit it",
  sticky: "Sticky",
  onlyOwnersMayDeleteOr: "Only owners may delete or rename entries",
  owner: "Owner",
  group: "Group",
  others: "Others",
  read: "Read",
  write: "Write",
  execute: "Execute",
} as const satisfies MessageArea;
