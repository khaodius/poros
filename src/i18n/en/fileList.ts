import type { MessageArea } from "../catalog";

export const fileList = {
  // components/FileList
  expandGroup: "Expand group",
  collapseGroup: "Collapse group",
  newNameFor: "New name for {name}",
  resize: "Resize {label}",
  dragToResizeDoubleClick: "Drag to resize, double-click to fit",
  nothingMatchesTheFilter: "Nothing matches the filter",
  onlyHiddenItemsHere: "Only hidden items here",
  thisFolderIsEmpty: "This folder is empty",
  deletedTag: "deleted",

  // components/PropertiesDialog
  folder: "Folder",
  brokenLink: "Broken link",
  file: "File",

  // lib/columns
  type: "Type",
  modified: "Modified",
  permissions: "Permissions",
  owner: "Owner",

  // lib/dateGroups
  inTheFuture: "In the future",
  today: "Today",
  yesterday: "Yesterday",
  earlierThisWeek: "Earlier this week",
  lastWeek: "Last week",
  earlierThisMonth: "Earlier this month",
  lastMonth: "Last month",
  earlierThisYear: "Earlier this year",
  aLongTimeAgo: "A long time ago",
  dateUnknown: "Date unknown",
} as const satisfies MessageArea;
